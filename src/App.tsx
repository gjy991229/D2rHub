import { useEffect, useState } from "react";
import { invokeCommand } from "./platform/tauri";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AlertTriangle } from "lucide-react";
import { useGlobalConfig, initConfigSync } from "./store/globalConfig";
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
  MainActionBar,
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
import { useLaunchGroupController } from "./hooks/useLaunchGroupController";

type View =
  | { type: "loading" }
  | { type: "setup"; existingConfig?: GlobalConfig }
  | { type: "main"; };

function App() {
  const { config, initialLoading, saving: configSaving, error: configError, patch } = useGlobalConfig();
  const { loadAccounts, accounts, deleteAccount, renameAccount, reorderAccounts } = useAccounts();
  const {
    launching,
    startLaunch,
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
      await invokeCommand("kill_all_d2r_processes");
      showToast("success", "清理完成，所有暗黑2进程已关闭。");
      setShowKillConfirm(false);
    } catch (e) {
      showToast("error", `关闭进程失败: ${e}`);
    } finally {
      setKilling(false);
    }
  };

  const [tokenUpdateAccount, setTokenUpdateAccount] = useState<AccountMeta | null>(null);
  const launchGroups = useLaunchGroupController();
  const launchGroupDraft = launchGroups.draft;
  const launchGroupPendingDelete = launchGroups.pendingDelete;

  const handleShareBattleReport = async (range: BattleReportQuickRange) => {
    if (sharingReport) return;
    setSharingReport(true);
    try {
      const [stats, preferences] = await Promise.all([
        invokeCommand<BattleReportStatsData>("get_stats_data"),
        invokeCommand<StatsPagePreferences | null>("get_stats_page_preferences"),
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

  // Subscribe before loading so no cross-window configuration commit is lost.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      const stopListening = await initConfigSync();
      if (cancelled) {
        stopListening();
        return;
      }
      unlisten = stopListening;
      await loadAccounts();
    })();
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
  useFirstLaunch(initialLoading, config, patch);

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
    void invokeCommand<AudioModSetupState>("get_audio_mod_setup_state", { accountId: target.account.id })
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
              <button type="button" onClick={() => void invokeCommand("open_logs_dir")}
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
  const launchableAccountIds = sortedAccounts.filter(
    account => account.initialized
      && !requiresTokenMigration(account.auth_mode, account.region, config),
  ).map(account => account.id);

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
              await invokeCommand("open_stats_page");
            } catch (e) {
              showToast("error", `打开统计失败: ${e}`);
            }
          }}
          onShareReport={handleShareBattleReport}
          sharingReport={sharingReport}
          onHelp={async () => {
            try {
              await invokeCommand("open_user_guide");
            } catch (e) {
              showToast("error", `打开帮助文档失败: ${e}`);
            }
          }}
        >
          <MainActionBar
            launching={launching}
            launchableAccountIds={launchableAccountIds}
            launchGroups={launchGroups}
            onCancelLaunch={cancelLaunch}
            onStartLaunch={startLaunch}
            onAddAccount={() => setShowInit(true)}
            onRequestKillAll={() => setShowKillConfirm(true)}
          />

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
              <AccountGrid
                accounts={sortedAccounts}
                isSelectionMode={!!launchGroupDraft}
                onReorder={reorderAccounts}
              >
                {sortedAccounts.map(a => {
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
                    onToggleSelect={launchGroups.toggleAccount}
                    schemeMember={schemeMember}
                    onSchemeMemberChange={launchGroups.updateMember}
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
                  />;
                })}
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
          if (!configSaving) launchGroups.closeDelete();
        }}
        title="删除启动方案"
        footer={
          <div className="flex justify-end gap-2">
            <Button
              variant="ghost"
              disabled={configSaving}
              onClick={launchGroups.closeDelete}
            >
              取消
            </Button>
            <Button variant="danger" loading={configSaving} onClick={() => void launchGroups.removePending()}>
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
