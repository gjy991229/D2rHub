import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AlertTriangle, Check, ListChecks, Trash2, UserPlus } from "lucide-react";
import { useGlobalConfig, initConfigListener } from "./store/globalConfig";
import { useAccounts } from "./store/accounts";
import { syncThemeFromConfig } from "./store/theme";
import { useWindowGeometrySave } from "./hooks/useWindowGeometrySave";
import { SetupWizard } from "./pages/SetupWizard";
import { AboutModal } from "./pages/AboutModal";
import { SettingsCenter } from "./components/config/SettingsCenter";
import { ToastContainer, showToast } from "./components/ui/Toast";
import { AppShell } from "./components/layout/AppShell";

import {
  useBongoCatWindow,
  useOverlayWindow,
  useLaunchEvents,
  useAutoUpdate,
  useFirstLaunch,
  usePreventDragRegionDoubleClick,
} from "./hooks/useAppEffects";

import UpdateConfirmModal from "./components/ui/UpdateConfirmModal";
import {
  Dashboard,
  AccountWorkspace,
  SortableAccountCard,
  AccountGridLoading,
  AccountGridEmpty,
  ActionBar,
  LaunchButton,
  LaunchGroupMenu,
} from "./components/dashboard";
import { useLaunch } from "./store/launch";
import { LaunchProgressView } from "./components/Launch/LaunchProgress";
import { AccountInitDialog } from "./components/accounts/AccountInitDialog";
import { requiresTokenMigration } from "./utils/regionPaths";
import { Modal } from "./components/ui/Modal";
import { Button } from "./components/ui/Button";
import type {
  AudioModSetupState,
  GlobalConfig,
  AccountMeta,
  LaunchGroup,
  LaunchGroupMember,
} from "./store/types";
import { setAuxiliaryWindowVisible } from "./utils/windowPlacement";
import { sortAccountsByCardOrder } from "./utils/accountOrder";
import { validateTrackingTarget } from "./utils/trackingTarget";
import {
  copyBattleReportToClipboard,
  type BattleReportStatsData,
  type BattleReportQuickRange,
  type StatsPagePreferences,
} from "./utils/battleReport";
import {
  inspectLaunchGroup,
  launchEntriesForGroup,
  launchGroupNameExists,
  materializeLaunchGroupMembers,
  nextLaunchGroupName,
} from "./utils/launchGroups";
import {
  completeWorkspaceOrder,
  insertAccountId,
  partitionAccountWorkspace,
} from "./utils/standbyPool";

type View =
  | { type: "loading" }
  | { type: "setup"; existingConfig?: GlobalConfig }
  | { type: "main"; };

interface LaunchGroupDraft {
  id: string | null;
  name: string;
  members: LaunchGroupMember[];
}

function createLaunchGroupId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `launch-group-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

function createLaunchGroupMember(account: AccountMeta): LaunchGroupMember {
  return {
    account_id: account.id,
    mod_args: account.mod_args || "",
    position_preset_id: account.active_position_id ?? null,
    position_configured: true,
    graphics_configured: false,
    resolution: null,
    fps: null,
  };
}

function App() {
  const { config, initialLoading, saving: configSaving, error: configError, load, save } = useGlobalConfig();
  const { loadAccounts, accounts, deleteAccount, renameAccount, reorderAccounts } = useAccounts();
  const {
    launching,
    startLaunch,
    startSchemeLaunch,
    startBattleNetOnly,
    cancelLaunch,
    logs,
    clearLogs,
  } = useLaunch();


  const [view, setView]         = useState<View>({ type: "loading" });
  const [showAbout, setShowAbout] = useState(false);
  const [showInit, setShowInit]   = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [settingsTab, setSettingsTab] = useState<string | null>(null);
  const [settingsAccountId, setSettingsAccountId] = useState<string | null>(null);
  const [audioModUpdate, setAudioModUpdate] = useState<AudioModSetupState | null>(null);
  const [sharingReport, setSharingReport] = useState(false);

  // Kill confirm modal states
  const [showKillConfirm, setShowKillConfirm] = useState(false);
  const [killing, setKilling] = useState(false);

  const handleKillAllD2R = async () => {
    setKilling(true);
    try {
      await invoke("kill_all_d2r_processes");
      showToast("success", "清理完成，所有暗黑2进程已关闭。");
      setShowKillConfirm(false);
    } catch (e) {
      showToast("error", `关闭进程失败: ${e}`);
    } finally {
      setKilling(false);
    }
  };

  // Persistent launch groups
  const [launchGroupDraft, setLaunchGroupDraft] = useState<LaunchGroupDraft | null>(null);
  const [launchGroupPendingDelete, setLaunchGroupPendingDelete] = useState<LaunchGroup | null>(null);
  const [tokenUpdateAccount, setTokenUpdateAccount] = useState<AccountMeta | null>(null);
  const [optimisticStandbyIds, setOptimisticStandbyIds] = useState<string[] | null>(null);
  const [standbySaving, setStandbySaving] = useState(false);

  const createLaunchGroup = () => {
    setLaunchGroupDraft({
      id: null,
      name: nextLaunchGroupName(config?.launch_groups ?? []),
      members: [],
    });
  };

  const handleShareBattleReport = async (range: BattleReportQuickRange) => {
    if (sharingReport) return;
    setSharingReport(true);
    try {
      const [stats, preferences] = await Promise.all([
        invoke<BattleReportStatsData>("get_stats_data"),
        invoke<StatsPagePreferences | null>("get_stats_page_preferences"),
      ]);
      const syncedPreferences = preferences || {};
      const result = await copyBattleReportToClipboard(stats, {
        ...syncedPreferences,
        reportConfig: { ...syncedPreferences.reportConfig, range },
      });
      showToast("success", `${result.rangeLabel}战报已复制（${result.runs} 场），可直接粘贴。`);
    } catch (error) {
      showToast("error", `生成战报失败: ${error}`);
    } finally {
      setSharingReport(false);
    }
  };

  const editLaunchGroup = (group: LaunchGroup) => {
    setLaunchGroupDraft({
      id: group.id,
      name: group.name,
      members: materializeLaunchGroupMembers(group, accounts),
    });
  };

  const selectAllLaunchGroupAccounts = () => {
    const initializedAccounts = accounts
      .filter(a => a.initialized && !requiresTokenMigration(a.auth_mode, a.region, config))
    setLaunchGroupDraft(draft => {
      if (!draft) return draft;
      const existing = new Map(draft.members.map(member => [member.account_id, member]));
      return {
        ...draft,
        members: initializedAccounts.map(account => existing.get(account.id) ?? createLaunchGroupMember(account)),
      };
    });
  };

  const clearLaunchGroupSelection = () => {
    setLaunchGroupDraft(draft => draft ? { ...draft, members: [] } : draft);
  };

  const toggleLaunchGroupAccount = (id: string) => {
    setLaunchGroupDraft(draft => {
      if (!draft) return draft;
      if (draft.members.some(member => member.account_id === id)) {
        return { ...draft, members: draft.members.filter(member => member.account_id !== id) };
      }
      const account = accounts.find(candidate => candidate.id === id);
      if (!account?.initialized || requiresTokenMigration(account.auth_mode, account.region, config)) {
        return draft;
      }
      return { ...draft, members: [...draft.members, createLaunchGroupMember(account)] };
    });
  };

  const updateLaunchGroupMember = (accountId: string, patch: Partial<LaunchGroupMember>) => {
    setLaunchGroupDraft(draft => draft ? {
      ...draft,
      members: draft.members.map(member => member.account_id === accountId
        ? { ...member, ...patch, account_id: accountId }
        : member),
    } : draft);
  };

  const saveLaunchGroup = async () => {
    if (!config || !launchGroupDraft || configSaving) return;
    const name = launchGroupDraft.name.trim();
    if (!name) {
      showToast("warning", "请输入启动方案名称");
      return;
    }
    if (launchGroupDraft.members.length === 0) {
      showToast("warning", "启动方案至少需要选择一个账号");
      return;
    }
    if (launchGroupDraft.members.some(member =>
      !member.graphics_configured || !member.resolution || member.fps == null)) {
      showToast("warning", "请等待所有已选账号的分辨率与 FPS 加载完成");
      return;
    }
    if (launchGroupNameExists(config.launch_groups, name, launchGroupDraft.id)) {
      showToast("warning", `启动方案名称“${name}”已存在`);
      return;
    }

    const id = launchGroupDraft.id ?? createLaunchGroupId();
    const savedGroup: LaunchGroup = {
      id,
      name,
      account_ids: launchGroupDraft.members.map(member => member.account_id),
      members: launchGroupDraft.members.map(member => ({
        ...member,
        mod_args: member.mod_args ?? "",
        position_preset_id: member.position_preset_id ?? null,
        position_configured: true,
        graphics_configured: true,
        resolution: member.resolution,
        fps: member.fps,
      })),
    };
    const nextGroups = launchGroupDraft.id
      ? config.launch_groups.map(group => group.id === launchGroupDraft.id ? savedGroup : group)
      : [...config.launch_groups, savedGroup];
    try {
      await save({ ...config, launch_groups: nextGroups });
      setLaunchGroupDraft(null);
      showToast("success", `启动方案“${name}”已保存`);
    } catch (error) {
      showToast("error", `保存启动方案失败: ${error}`);
    }
  };

  const deleteLaunchGroup = async () => {
    if (!config || !launchGroupPendingDelete || configSaving) return;
    const group = launchGroupPendingDelete;
    try {
      await save({
        ...config,
        launch_groups: config.launch_groups.filter(candidate => candidate.id !== group.id),
      });
      if (launchGroupDraft?.id === group.id) setLaunchGroupDraft(null);
      setLaunchGroupPendingDelete(null);
      showToast("success", `启动方案“${group.name}”已删除`);
    } catch (error) {
      showToast("error", `删除启动方案失败: ${error}`);
    }
  };

  const launchSavedGroup = (group: LaunchGroup) => {
    const availability = inspectLaunchGroup(group, accounts, config);
    if (!availability.can_launch) {
      showToast("warning", `启动方案“${group.name}”配置不完整，请先修复后再启动`);
      return;
    }
    void startSchemeLaunch(launchEntriesForGroup(group, accounts));
  };

  // Auto-update modal states
  const [showAutoUpdateConfirm, setShowAutoUpdateConfirm] = useState(false);
  const [autoUpdateUrl, setAutoUpdateUrl] = useState("");
  const [autoUpdateVersion, setAutoUpdateVersion] = useState("");

  useWindowGeometrySave("save_window_geometry", 100, 100);
  usePreventDragRegionDoubleClick();

  // 等待 DOM 渲染完成后显示窗口（避免白屏闪烁）
  useEffect(() => {
    getCurrentWindow().show().catch(() => {});
    // 应用字体缩放预设
    try {
      const saved = localStorage.getItem("d2rhub-font-scale");
      if (saved && ["small","default","large"].includes(saved)) {
        document.documentElement.dataset.fontScale = saved;
      } else {
        document.documentElement.dataset.fontScale = "default";
      }
    } catch {
      document.documentElement.dataset.fontScale = "default";
    }
  }, []);

  // Load configuration, accounts, and start config listener
  useEffect(() => {
    (async () => { await load(); await loadAccounts(); })();
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    initConfigListener().then(fn => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Update view state
  useEffect(() => {
    if (initialLoading) return;
    if (!config || !config.first_run_complete) {
      setView({ type: "setup", existingConfig: config ?? undefined });
    }
    else setView({ type: "main" });
    // Sync theme from config (config as source of truth)
    if (config?.theme) syncThemeFromConfig(config.theme);
  }, [initialLoading, config]);

  // Sync font scale from config
  useEffect(() => {
    if (!config?.font_scale) return;
    if (["small","default","large"].includes(config.font_scale)) {
      document.documentElement.dataset.fontScale = config.font_scale;
      try { localStorage.setItem("d2rhub-font-scale", config.font_scale); } catch {}
    }
  }, [config?.font_scale]);

  // Execute App Side Effects
  useBongoCatWindow(initialLoading, config);
  useOverlayWindow(initialLoading, config);
  useLaunchEvents(config);
  useAutoUpdate(initialLoading, config, (version, url) => {
    setAutoUpdateVersion(version);
    setAutoUpdateUrl(url);
    setShowAutoUpdateConfirm(true);
  });
  useFirstLaunch(initialLoading, config, save);

  useEffect(() => {
    if (initialLoading || !config?.rune_audio_enabled) {
      setAudioModUpdate(null);
      return;
    }
    const target = validateTrackingTarget(config.rune_audio_target_account, accounts);
    if (!target.valid) {
      setAudioModUpdate(null);
      return;
    }

    let cancelled = false;
    void invoke<AudioModSetupState>("get_audio_mod_setup_state", { accountId: target.account.id })
      .then((state) => {
        if (!cancelled) setAudioModUpdate(state.update_required ? state : null);
      })
      .catch(() => {
        if (!cancelled) setAudioModUpdate(null);
      });
    return () => {
      cancelled = true;
    };
  }, [initialLoading, config?.rune_audio_enabled, config?.rune_audio_target_account, accounts, showSettings]);

  const handleReconfigure = () => {
    setView({ type: "setup", existingConfig: config ?? undefined });
  };

  if (!initialLoading && configError && !config) return (
    <AppShell>
      <div className="flex-1 flex items-center justify-center px-6">
        <div className="w-[560px] account-line px-6 py-5">
          <div className="flex items-start gap-3">
            <div className="swiss-mark shrink-0">
              <AlertTriangle size={16} className="text-error" strokeWidth={1.8} />
            </div>
            <div className="min-w-0 flex-1">
              <p className="text-sm font-semibold text-text-primary">配置文件无法安全读取</p>
              <p className="text-sm text-text-secondary mt-2 leading-relaxed">
                为避免默认值覆盖现有数据，D2RHub 已停止配置初始化。原文件仍保留；如果存在可用备份，程序会在启动时自动恢复。
              </p>
              <p className="text-xs font-mono text-error mt-3 break-all leading-relaxed">{configError}</p>
              <button type="button" onClick={() => void invoke("open_logs_dir")}
                className="control-btn h-9 mt-4">
                打开日志目录
              </button>
            </div>
          </div>
        </div>
      </div>
    </AppShell>
  );

  // ── loading ──
  if (view.type === "loading") return (
    <AppShell>
      <div className="flex-1 flex items-center justify-center">
        <div className="flex flex-col items-center gap-4">
          <div className="w-10 h-10 rounded-full border-2 border-accent border-t-transparent animate-spin" />
          <p className="text-md text-text-muted">正在加载...</p>
        </div>
      </div>
    </AppShell>
  );

  // ── setup ──
  if (view.type === "setup") return (
    <AppShell>
      <div className="flex-1 flex flex-col">
        <SetupWizard
          initialConfig={view.existingConfig}
          onComplete={() => setView({ type: "main" })}
        />
      </div>
      <ToastContainer />
    </AppShell>
  );

  // ── main ──
  const sortedAccounts = sortAccountsByCardOrder(accounts);
  const workspace = partitionAccountWorkspace(
    sortedAccounts,
    optimisticStandbyIds ?? config?.standby_account_ids ?? [],
  );
  const standbySet = new Set(workspace.standbyIds);
  const gridAccounts = launchGroupDraft ? sortedAccounts : workspace.active;
  const defaultLaunchAccountIds = workspace.active.filter(
    account => account.initialized
      && !requiresTokenMigration(account.auth_mode, account.region, config),
  ).map(account => account.id);
  const workspaceChanging = standbySaving || configSaving || launching;

  const persistStandbyWorkspace = async (
    standbyIds: string[],
    orderedAccountIds?: string[],
  ) => {
    if (!config || standbySaving || configSaving) return;
    const previousStandbyIds = workspace.standbyIds;
    setStandbySaving(true);
    setOptimisticStandbyIds(standbyIds);
    try {
      await save({ ...config, standby_account_ids: standbyIds });
      if (orderedAccountIds) await reorderAccounts(orderedAccountIds);
    } catch (error) {
      setOptimisticStandbyIds(previousStandbyIds);
      showToast("error", `更新待机池失败: ${error}`);
    } finally {
      setOptimisticStandbyIds(null);
      setStandbySaving(false);
    }
  };

  const moveToStandby = (accountId: string, beforeId?: string | null) => {
    if (workspaceChanging || standbySet.has(accountId)) return;
    const nextStandbyIds = insertAccountId(workspace.standbyIds, accountId, beforeId);
    void persistStandbyWorkspace(nextStandbyIds);
  };

  const moveToLaunchpad = (accountId: string, beforeId?: string | null) => {
    if (workspaceChanging || !standbySet.has(accountId)) return;
    const nextStandbyIds = workspace.standbyIds.filter(candidate => candidate !== accountId);
    const nextActiveIds = insertAccountId(
      workspace.active.map(account => account.id),
      accountId,
      beforeId,
    );
    void persistStandbyWorkspace(
      nextStandbyIds,
      completeWorkspaceOrder(nextActiveIds, nextStandbyIds),
    );
  };

  const toggleStandby = (accountId: string) => {
    if (standbySet.has(accountId)) moveToLaunchpad(accountId);
    else moveToStandby(accountId);
  };

  const reorderActiveWorkspace = (activeIds: string[]) => {
    if (workspaceChanging) return;
    void reorderAccounts(completeWorkspaceOrder(activeIds, workspace.standbyIds));
  };

  const reorderStandbyWorkspace = (standbyIds: string[]) => {
    if (workspaceChanging) return;
    void persistStandbyWorkspace(standbyIds);
  };

  return (
    <>
      <AppShell>
        <Dashboard
          onAbout={() => setShowAbout(true)}
          onExit={async () => {
            const mainWin = getCurrentWindow();
            await mainWin.hide();
            if (config?.enable_tz_overlay) {
              await setAuxiliaryWindowVisible('overlay', true);
            }
            if (config?.enable_stats_overlay) {
              await setAuxiliaryWindowVisible('stats-overlay', true);
            }
          }}
          onOpenConfig={() => { setShowSettings(true); setSettingsTab(null); setSettingsAccountId(null); }}
          onStats={async () => {
            try {
              await invoke("open_stats_page");
            } catch (e) {
              showToast("error", `打开统计失败: ${e}`);
            }
          }}
          onShareReport={handleShareBattleReport}
          sharingReport={sharingReport}
          onHelp={async () => {
            try {
              await invoke("open_user_guide");
            } catch (e) {
              showToast("error", `打开帮助文档失败: ${e}`);
            }
          }}
        >
          <ActionBar>
            {launching ? (
              <button
                onClick={cancelLaunch}
                className="danger-cta"
              >
                取消操作
              </button>
            ) : launchGroupDraft ? (
              <>
                <div className="launch-group-editor flex min-w-0 items-center gap-2">
                  <span className="launch-group-editor-label">
                    <ListChecks size={13} strokeWidth={1.9} aria-hidden="true" />
                    {launchGroupDraft.id ? "编辑启动方案" : "新建启动方案"}
                  </span>
                  <input
                    type="text"
                    className="line-input launch-group-name-input px-2.5"
                    value={launchGroupDraft.name}
                    maxLength={32}
                    aria-label="启动方案名称"
                    placeholder="启动方案名称"
                    autoFocus
                    disabled={configSaving}
                    onChange={event => setLaunchGroupDraft(draft => draft
                      ? { ...draft, name: event.target.value }
                      : draft)}
                    onKeyDown={event => {
                      if (event.key === "Enter") void saveLaunchGroup();
                      if (event.key === "Escape") setLaunchGroupDraft(null);
                    }}
                  />
                  <button
                    disabled={configSaving
                      || launchGroupDraft.members.length === 0
                      || !launchGroupDraft.name.trim()
                      || launchGroupDraft.members.some(member =>
                        !member.graphics_configured || !member.resolution || member.fps == null)}
                    onClick={() => void saveLaunchGroup()}
                    className="primary-cta"
                  >
                    <Check size={13} strokeWidth={2} />
                    保存方案 ({launchGroupDraft.members.length})
                  </button>
                  <button
                    onClick={selectAllLaunchGroupAccounts}
                    disabled={configSaving}
                    className="control-btn"
                  >
                    全选
                  </button>
                  <button
                    onClick={clearLaunchGroupSelection}
                    disabled={configSaving || launchGroupDraft.members.length === 0}
                    className="control-btn"
                  >
                    清空已选
                  </button>
                  <button
                    onClick={() => setLaunchGroupDraft(null)}
                    disabled={configSaving}
                    className="control-btn"
                  >
                    取消
                  </button>
                </div>
                <div className="flex-1" />
                {launchGroupDraft.id && (
                  <button
                    type="button"
                    className="control-btn danger-control"
                    disabled={configSaving}
                    onClick={() => {
                      const group = config?.launch_groups.find(candidate => candidate.id === launchGroupDraft.id);
                      if (group) setLaunchGroupPendingDelete(group);
                    }}
                  >
                    <Trash2 size={12} strokeWidth={1.8} aria-hidden="true" />
                    删除方案
                  </button>
                )}
              </>
            ) : (
              <div className="flex items-center gap-2">
                <LaunchButton
                  count={defaultLaunchAccountIds.length}
                  loading={launching}
                  onClick={() => startLaunch(defaultLaunchAccountIds)}
                />
                <LaunchGroupMenu
                  groups={config?.launch_groups ?? []}
                  accounts={accounts}
                  config={config}
                  disabled={launching || configSaving}
                  onLaunch={launchSavedGroup}
                  onCreate={createLaunchGroup}
                  onEdit={editLaunchGroup}
                />
                <button
                  onClick={() => setShowKillConfirm(true)}
                  title="一键关闭所有暗黑2进程"
                  className="control-btn danger-control min-w-[72px]"
                >
                  一键关闭
                </button>
              </div>
            )}
            {!launchGroupDraft && (
              <>
                <div className="flex-1" />
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => setShowInit(true)}
                    className="control-btn add-account-cta"
                  >
                    <UserPlus size={13} strokeWidth={1.9} />
                    添加账号
                  </button>
                </div>
              </>
            )}
          </ActionBar>

          {audioModUpdate && (
            <div className="shrink-0 px-5 pb-2.5">
              <div
                role="status"
                className="flex items-center justify-between gap-4 rounded-xl border border-warning/25 bg-warning/10 px-3.5 py-2.5"
              >
                <div className="flex min-w-0 items-start gap-2.5">
                  <AlertTriangle size={16} className="mt-0.5 shrink-0 text-warning" />
                  <div className="min-w-0">
                    <p className="text-xs font-semibold text-text-primary">
                      “{audioModUpdate.account_name}”的识别 Mod 有新版
                    </p>
                    <p className="mt-0.5 text-2xs leading-relaxed text-text-secondary">
                      {audioModUpdate.message}
                    </p>
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    setSettingsTab("automation");
                    setSettingsAccountId(null);
                    setShowSettings(true);
                  }}
                  className="control-btn shrink-0"
                >
                  更新识别 Mod
                </button>
              </div>
            </div>
          )}

          {initialLoading ? (
            <AccountGridLoading />
          ) : accounts.length === 0 ? (
            <AccountGridEmpty onAddAccount={() => setShowInit(true)} />
          ) : (
            <>
              <AccountWorkspace
                activeAccounts={workspace.active}
                standbyAccounts={workspace.standby}
                gridAccounts={gridAccounts}
                config={config}
                isSelectionMode={!!launchGroupDraft}
                disabled={workspaceChanging}
                onReorderActive={reorderActiveWorkspace}
                onReorderStandby={reorderStandbyWorkspace}
                onMoveToStandby={moveToStandby}
                onMoveToLaunchpad={moveToLaunchpad}
                onLaunchStandby={id => startLaunch([id])}
                onConfigure={account => {
                  setShowSettings(true);
                  setSettingsTab("accounts");
                  setSettingsAccountId(account.id);
                }}
              >
                {gridAccounts.map(a => {
                  const schemeMember = launchGroupDraft?.members.find(member => member.account_id === a.id);
                  return <SortableAccountCard
                    key={a.id}
                    account={a}
                    onRename={renameAccount}
                    onDelete={deleteAccount}
                    onConfigure={a => { setShowSettings(true); setSettingsTab("accounts"); setSettingsAccountId(a.id); }}
                    onLaunch={id => startLaunch([id])}
                    onBattleNetOnly={id => startBattleNetOnly([id])}
                    isSelectionMode={!!launchGroupDraft}
                    selected={!!schemeMember}
                    onToggleSelect={toggleLaunchGroupAccount}
                    schemeMember={schemeMember}
                    onSchemeMemberChange={updateLaunchGroupMember}
                    getModSchemeUsage={(accountId, modArgs) => (config?.launch_groups ?? [])
                      .filter(group => group.id !== launchGroupDraft?.id)
                      .filter(group => group.members?.some(member =>
                        member.account_id === accountId && member.mod_args === modArgs))
                      .map(group => group.name)}
                    getPositionSchemeUsage={(accountId, positionId) => (config?.launch_groups ?? [])
                      .filter(group => group.id !== launchGroupDraft?.id)
                      .filter(group => group.members?.some(member =>
                        member.account_id === accountId
                        && member.position_configured
                        && member.position_preset_id === positionId))
                      .map(group => group.name)}
                    onUpdateToken={setTokenUpdateAccount}
                    config={config}
                    isStandby={standbySet.has(a.id)}
                    onToggleStandby={toggleStandby}
                    standbyChanging={workspaceChanging}
                  />;
                })}
              </AccountWorkspace>

              <LaunchProgressView
                accounts={accounts}
                logs={logs}
                onClear={clearLogs}
              />
            </>
          )}
        </Dashboard>
      </AppShell>

      <AccountInitDialog
        open={showInit || !!tokenUpdateAccount}
        onClose={() => { setShowInit(false); setTokenUpdateAccount(null); }}
        onDone={() => { setShowInit(false); setTokenUpdateAccount(null); }}
        updateAccount={tokenUpdateAccount}
      />
      <AboutModal open={showAbout} onClose={() => setShowAbout(false)} />
      <Modal
        open={showKillConfirm}
        onClose={() => setShowKillConfirm(false)}
        title="确认关闭进程"
        footer={
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setShowKillConfirm(false)}>
              取消
            </Button>
            <Button variant="danger" onClick={handleKillAllD2R} loading={killing}>
              确认关闭
            </Button>
          </div>
        }
      >
        <div className="text-sm text-text-secondary py-2">
          确定要强制关闭当前系统内运行的所有暗黑破坏神II：重制版（D2R.exe）进程吗？此操作可能会导致未保存的游戏进度丢失。
        </div>
      </Modal>
      <Modal
        open={!!launchGroupPendingDelete}
        onClose={() => {
          if (!configSaving) setLaunchGroupPendingDelete(null);
        }}
        title="删除启动方案"
        footer={
          <div className="flex justify-end gap-2">
            <Button
              variant="ghost"
              disabled={configSaving}
              onClick={() => setLaunchGroupPendingDelete(null)}
            >
              取消
            </Button>
            <Button variant="danger" loading={configSaving} onClick={() => void deleteLaunchGroup()}>
              删除启动方案
            </Button>
          </div>
        }
      >
        <div className="py-2 text-sm leading-relaxed text-text-secondary">
          确定删除启动方案“{launchGroupPendingDelete?.name}”吗？账号及账号胶囊库不会被删除。
        </div>
      </Modal>
      <SettingsCenter
        open={showSettings}
        onClose={() => setShowSettings(false)}
        onReconfigure={handleReconfigure}
        onInitializeAccount={() => setShowInit(true)}
        initialTab={settingsTab}
        initialAccountId={settingsAccountId}
      />

      <UpdateConfirmModal
        open={showAutoUpdateConfirm}
        onClose={() => setShowAutoUpdateConfirm(false)}
        version={autoUpdateVersion}
        downloadUrl={autoUpdateUrl}
      />

      <ToastContainer />
    </>
  );
}

export default App;
