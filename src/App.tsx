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
  AccountGrid,
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
import type { AudioModSetupState, GlobalConfig, AccountMeta, LaunchGroup } from "./store/types";
import { setAuxiliaryWindowVisible } from "./utils/windowPlacement";
import { sortAccountsByCardOrder } from "./utils/accountOrder";
import { validateTrackingTarget } from "./utils/trackingTarget";
import {
  copyBattleReportToClipboard,
  type BattleReportStatsData,
  type StatsPagePreferences,
} from "./utils/battleReport";
import {
  inspectLaunchGroup,
  launchGroupNameExists,
  nextLaunchGroupName,
} from "./utils/launchGroups";

type View =
  | { type: "loading" }
  | { type: "setup"; existingConfig?: GlobalConfig }
  | { type: "main"; };

interface LaunchGroupDraft {
  id: string | null;
  name: string;
  accountIds: string[];
}

function createLaunchGroupId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `launch-group-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

function App() {
  const { config, initialLoading, saving: configSaving, error: configError, load, save } = useGlobalConfig();
  const { loadAccounts, accounts, deleteAccount, renameAccount, reorderAccounts } = useAccounts();
  const { launching, startLaunch, startBattleNetOnly, cancelLaunch, logs, clearLogs } = useLaunch();


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

  const createLaunchGroup = () => {
    setLaunchGroupDraft({
      id: null,
      name: nextLaunchGroupName(config?.launch_groups ?? []),
      accountIds: [],
    });
  };

  const handleShareBattleReport = async () => {
    if (sharingReport) return;
    setSharingReport(true);
    try {
      const [stats, preferences] = await Promise.all([
        invoke<BattleReportStatsData>("get_stats_data"),
        invoke<StatsPagePreferences | null>("get_stats_page_preferences"),
      ]);
      const result = await copyBattleReportToClipboard(stats, preferences || {});
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
      accountIds: [...group.account_ids],
    });
  };

  const selectAllLaunchGroupAccounts = () => {
    const initializedIds = accounts
      .filter(a => a.initialized && !requiresTokenMigration(a.auth_mode, a.region, config))
      .map(a => a.id);
    setLaunchGroupDraft(draft => draft ? { ...draft, accountIds: initializedIds } : draft);
  };

  const clearLaunchGroupSelection = () => {
    setLaunchGroupDraft(draft => draft ? { ...draft, accountIds: [] } : draft);
  };

  const toggleLaunchGroupAccount = (id: string) => {
    setLaunchGroupDraft(draft => {
      if (!draft) return draft;
      if (draft.accountIds.includes(id)) {
        return { ...draft, accountIds: draft.accountIds.filter(accountId => accountId !== id) };
      }
      const account = accounts.find(candidate => candidate.id === id);
      if (!account?.initialized || requiresTokenMigration(account.auth_mode, account.region, config)) {
        return draft;
      }
      return { ...draft, accountIds: [...draft.accountIds, id] };
    });
  };

  const saveLaunchGroup = async () => {
    if (!config || !launchGroupDraft || configSaving) return;
    const name = launchGroupDraft.name.trim();
    if (!name) {
      showToast("warning", "请输入启动组名称");
      return;
    }
    if (launchGroupDraft.accountIds.length === 0) {
      showToast("warning", "启动组至少需要选择一个账号");
      return;
    }
    if (launchGroupNameExists(config.launch_groups, name, launchGroupDraft.id)) {
      showToast("warning", `启动组名称“${name}”已存在`);
      return;
    }

    const id = launchGroupDraft.id ?? createLaunchGroupId();
    const savedGroup: LaunchGroup = {
      id,
      name,
      account_ids: [...new Set(launchGroupDraft.accountIds)],
    };
    const nextGroups = launchGroupDraft.id
      ? config.launch_groups.map(group => group.id === launchGroupDraft.id ? savedGroup : group)
      : [...config.launch_groups, savedGroup];
    try {
      await save({ ...config, launch_groups: nextGroups });
      setLaunchGroupDraft(null);
      showToast("success", `启动组“${name}”已保存`);
    } catch (error) {
      showToast("error", `保存启动组失败: ${error}`);
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
      showToast("success", `启动组“${group.name}”已删除`);
    } catch (error) {
      showToast("error", `删除启动组失败: ${error}`);
    }
  };

  const launchSavedGroup = (group: LaunchGroup) => {
    const availability = inspectLaunchGroup(group, accounts, config);
    if (!availability.can_launch) {
      showToast("warning", `启动组“${group.name}”包含不可用账号，请先完成配置或编辑启动组`);
      return;
    }
    void startLaunch(availability.ordered_account_ids);
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
  const initialized = accounts.filter(
    a => a.initialized && !requiresTokenMigration(a.auth_mode, a.region, config),
  );
  const sortedAccounts = sortAccountsByCardOrder(accounts);

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
                    {launchGroupDraft.id ? "编辑启动组" : "新建启动组"}
                  </span>
                  <input
                    type="text"
                    className="line-input launch-group-name-input px-2.5"
                    value={launchGroupDraft.name}
                    maxLength={32}
                    aria-label="启动组名称"
                    placeholder="启动组名称"
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
                    disabled={configSaving || launchGroupDraft.accountIds.length === 0 || !launchGroupDraft.name.trim()}
                    onClick={() => void saveLaunchGroup()}
                    className="primary-cta"
                  >
                    <Check size={13} strokeWidth={2} />
                    保存 ({launchGroupDraft.accountIds.length})
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
                    disabled={configSaving || launchGroupDraft.accountIds.length === 0}
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
                    删除组
                  </button>
                )}
              </>
            ) : (
              <div className="flex items-center gap-2">
                <LaunchButton
                  count={initialized.length}
                  loading={launching}
                  onClick={() => startLaunch(sortedAccounts
                    .filter(a => a.initialized && !requiresTokenMigration(a.auth_mode, a.region, config))
                    .map(a => a.id))}
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
              <AccountGrid accounts={sortedAccounts} onReorder={reorderAccounts} isSelectionMode={!!launchGroupDraft}>
                {sortedAccounts.map(a => (
                  <SortableAccountCard
                    key={a.id}
                    account={a}
                    onRename={renameAccount}
                    onDelete={deleteAccount}
                    onConfigure={a => { setShowSettings(true); setSettingsTab("accounts"); setSettingsAccountId(a.id); }}
                    onLaunch={id => startLaunch([id])}
                    onBattleNetOnly={id => startBattleNetOnly([id])}
                    isSelectionMode={!!launchGroupDraft}
                    selected={launchGroupDraft?.accountIds.includes(a.id) ?? false}
                    onToggleSelect={toggleLaunchGroupAccount}
                    onUpdateToken={setTokenUpdateAccount}
                    config={config}
                  />
                ))}
              </AccountGrid>

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
        title="删除启动组"
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
              删除启动组
            </Button>
          </div>
        }
      >
        <div className="py-2 text-sm leading-relaxed text-text-secondary">
          确定删除启动组“{launchGroupPendingDelete?.name}”吗？组内账号及其配置不会被删除。
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
