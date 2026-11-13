import { LaunchGroupEditor } from "./components/dashboard/LaunchGroupEditor";
import { useEffect, useRef, useState } from "react";
import { useAccountBatch, refreshBatchRunning } from "./hooks/useAccountBatch";
import { flushAccountQuickSettings } from "./hooks/useAccountQuickSettings";
import { invokeCommand, listenEvent } from "./platform/tauri";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AlertTriangle } from "lucide-react";
import { useGlobalConfig, initConfigSync } from "./store/globalConfig";
import { useAccounts } from "./store/accounts";
import { syncThemeFromConfig } from "./store/theme";
import { useMiniMode } from "./hooks/useMiniMode";
import { MiniDashboard } from "./components/dashboard/MiniDashboard";
import { SetupWizard } from "./pages/SetupWizard";
import { FeatureProfileChooser } from "./pages/FeatureProfileChooser";
import { AboutModal } from "./pages/AboutModal";
import { SettingsCenter } from "./components/config/SettingsCenter";
import { ToastContainer, showToast } from "./components/ui/Toast";
import { AppShell } from "./components/layout/AppShell";

import {
  useBongoCatWindow,
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
  LaunchGroupPanel,
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
import { sortAccountsByCardOrder } from "./utils/accountOrder";
import { validateTrackingTarget } from "./utils/trackingTarget";
import {
  copyBattleReportToClipboard,
  type BattleReportStatsData,
  type BattleReportQuickRange,
  type StatsPagePreferences,
} from "./utils/battleReport";
import { useLaunchGroupController } from "./hooks/useLaunchGroupController";
import { useLaunchSchemeShortcut } from "./hooks/useLaunchSchemeShortcut";
import { useModCapsulePool } from "./features/modCapsules/useModCapsulePool";
import { DisclosureDialog } from "./features/disclosures/DisclosureDialog";
import { useApplicationDisclosure } from "./features/disclosures/useApplicationDisclosure";
import {
  isFeatureProfileDecisionCurrent,
  optionalFeaturesAreAvailable,
  type FeatureProfile,
} from "./features/profile/featureProfile";

type View =
  | { type: "loading" }
  | { type: "profile" }
  | { type: "setup"; existingConfig?: GlobalConfig }
  | { type: "main"; };

function App() {
  useEffect(() => {
    const listener = listenEvent<string>("global-hotkey-error", event => showToast("error", event.payload));
    return () => { void listener.then(stop => stop()).catch(() => {}); };
  }, []);
  const { config, initialLoading, saving: configSaving, restarting, error: configError, patch } = useGlobalConfig();
  const profileDecisionCurrent = isFeatureProfileDecisionCurrent(config);
  const applicationDisclosure = useApplicationDisclosure(
    !initialLoading && config !== null,
    profileDecisionCurrent,
  );
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
  const [settingsRequestRevision, setSettingsRequestRevision] = useState(0);
  const petModuleAvailable = !restarting && optionalFeaturesAreAvailable(config)
    && config?.installed_optional_modules?.includes("pet") === true;
  useEffect(() => {
    if (!petModuleAvailable) return;
    let cancelled = false;
    const listener = listenEvent("pet-open-wardrobe", () => {
      if (cancelled) return;
      setSettingsTab("pet"); setSettingsAccountId(null); setShowSettings(true);
      setSettingsRequestRevision(revision => revision + 1);
    });
    void listener.catch(error => {
      if (!cancelled) showToast("error", String(error));
    });
    return () => { cancelled = true; void listener.then(unlisten => unlisten()).catch(() => {}); };
  }, [petModuleAvailable]);
  const [audioModUpdate, setAudioModUpdate] = useState<AudioModSetupState | null>(null);
  const [sharingReport, setSharingReport] = useState(false);
  const [launchGroupPanelOpen, setLaunchGroupPanelOpen] = useState(false);
  const [logRevealRevision, setLogRevealRevision] = useState(0);
  const [launchIssueDetails, setLaunchIssueDetails] = useState<string | null>(null);
  const [exitingForDisclosure, setExitingForDisclosure] = useState(false);

  // Kill confirm modal states
  const [showKillConfirm, setShowKillConfirm] = useState(false);
  const [killing, setKilling] = useState(false);
  const [dispatchingBatch, setDispatchingBatch] = useState(false);
  const [killTargets, setKillTargets] = useState<string[] | null>(null);
  const batchLock = useRef(false);
  const disclosureBlockingStartup = initialLoading
    || applicationDisclosure.checking
    || applicationDisclosure.error !== null
    || applicationDisclosure.required;
  const profileDecisionPending = !!config && !profileDecisionCurrent;
  const startupServicesBlocked = disclosureBlockingStartup || profileDecisionPending || restarting;
  const optionalFeaturesAvailable = !restarting && optionalFeaturesAreAvailable(config);

  const handleKillAllD2R = async () => {
    if (batchLock.current || launching) return;
    batchLock.current = true;
    setKilling(true);
    try {
      if (killTargets !== null) {
        const running = await refreshBatchRunning();
        const ids = killTargets.filter(id => running.has(id));
        const results = ids.length ? await invokeCommand<{ account_id: string; error: string | null }[]>("close_selected_accounts", { accountIds: ids }) : [];
        const failures = results.filter(result => result.error);
        batch.setSelection(failures.length ? { mode: "close", ids: failures.map(result => result.account_id) } : { mode: null, ids: [] });
        if (failures.length) showToast("error", failures.map(result => `${accounts.find(a => a.id === result.account_id)?.display_name || result.account_id}：${result.error}`).join("；"));
        else showToast("success", `已关闭选中的 ${killTargets.length} 个账号（已退出的账号自动跳过）`);
      } else {
        await invokeCommand("kill_all_d2r_processes");
        showToast("success", "清理完成，所有暗黑2进程已关闭。");
      }
      setShowKillConfirm(false);
      await batch.refresh();
    } catch (e) {
      showToast("error", `关闭进程失败: ${e}`);
    } finally {
      setKilling(false);
      batchLock.current = false;
    }
  };

  const [tokenUpdateAccount, setTokenUpdateAccount] = useState<AccountMeta | null>(null);
  const [reinitializeAccount, setReinitializeAccount] = useState<AccountMeta | null>(null);
  const modCapsules = useModCapsulePool({
    active: view.type === "main" && !startupServicesBlocked && optionalFeaturesAvailable,
    onAssigned: loadAccounts,
  });
  const launchGroups = useLaunchGroupController(modCapsules.pool);
  const launchGroupDraft = launchGroups.draft;
  useEffect(() => {
    const editFromRoom = (event: Event) => {
      const ids = (event as CustomEvent<string[]>).detail;
      if (!Array.isArray(ids)) return;
      setShowSettings(false);
      setLaunchGroupPanelOpen(true);
      launchGroups.edit({ id: "", name: "自动跟房", account_ids: ids });
    };
    window.addEventListener("edit-launch-scheme-from-room", editFromRoom);
    return () => window.removeEventListener("edit-launch-scheme-from-room", editFromRoom);
  }, [launchGroups.edit]);

  const launchGroupPendingDelete = launchGroups.pendingDelete;
  const openModManager = (action?: "add", edition?: string | null) => {
    setSettingsTab(action === "add" ? `mod-processing:add:${edition ?? ""}` : "mod-processing");
    setSettingsAccountId(null);
    setShowSettings(true);
  };

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
      showToast("success", `${result.rangeLabel}战报已复制，可直接粘贴。`);
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

  const miniMode = useMiniMode(
    view.type === "main" && !startupServicesBlocked,
    showSettings || showInit || !!tokenUpdateAccount || !!reinitializeAccount
      || showAbout || showAutoUpdateConfirm || !!launchGroupDraft || startupServicesBlocked || view.type !== "main",
  );
  const batch = useAccountBatch(
    view.type === "main" && !startupServicesBlocked && !miniMode.mini && accounts.length > 0,
    !!launchGroupDraft || showSettings || showInit || !!tokenUpdateAccount || !!reinitializeAccount,
    config, launching || killing || dispatchingBatch,
  );
  const schemeShortcutEnabled = useRef(false);
  schemeShortcutEnabled.current = view.type === "main" && !startupServicesBlocked && !showSettings && !showInit
    && !tokenUpdateAccount && !reinitializeAccount && !launchGroupDraft && !launchGroups.busy && !showKillConfirm;
  useLaunchSchemeShortcut(
    schemeShortcutEnabled.current,
    async group => {
      if (batchLock.current) return;
      batchLock.current = true;
      setDispatchingBatch(true);
      try {
        await flushAccountQuickSettings(group.account_ids);
        const settings = useGlobalConfig.getState();
        if (!schemeShortcutEnabled.current || settings.initialLoading || settings.saving || settings.restarting
          || useLaunch.getState().launching) return;
        const current = settings.config?.launch_groups.find(saved => saved.id === group.id);
        if (!current) return;
        batch.clear();
        await useLaunch.getState().startSavedScheme(current.id, current.revision ?? 0);
      } finally {
        setDispatchingBatch(false);
        batchLock.current = false;
        await batch.refresh();
      }
    },
  );
  const startDashboardLaunch = async (requestedIds: string[]) => {
    if (batchLock.current || launching || configSaving) return;
    batchLock.current = true;
    setDispatchingBatch(true);
    try {
      await flushAccountQuickSettings(requestedIds);
      const running = await refreshBatchRunning();
      const requested = new Set(requestedIds);
      const ids = sortAccountsByCardOrder(useAccounts.getState().accounts)
        .filter(account => requested.has(account.id) && !running.has(account.id)).map(account => account.id);
      if (!ids.length) { batch.clear(); showToast("info", "所选账号已经运行或已移除"); return; }
      const health = await invokeCommand<{ account_id: string; error: string | null }[]>("inspect_account_launch_health");
      const invalid = ids.filter(id => !health.some(row => row.account_id === id && row.error === null));
      if (invalid.length) throw new Error(invalid.map(id => `${accounts.find(a => a.id === id)?.display_name || id}：${health.find(row => row.account_id === id)?.error || "账号状态待确认"}`).join("；"));
      batch.clear();
      await startLaunch(ids);
    } catch (error) { showToast("error", `未启动：${error}`); }
    finally {
      setDispatchingBatch(false);
      batchLock.current = false;
      await batch.refresh();
    }
  };
  const requestDashboardClose = () => {
    if (batchLock.current || launching) return;
    setKillTargets(batch.selection.mode === "close" ? [...batch.selection.ids] : null);
    setShowKillConfirm(true);
  };
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
    if (!config || config.legacy_path_migration) {
      setView({ type: "setup", existingConfig: config ?? undefined });
    } else if (!isFeatureProfileDecisionCurrent(config)) {
      setView({ type: "profile" });
    } else if (!config.first_run_complete) {
      setView({ type: "setup", existingConfig: config });
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
  useBongoCatWindow(startupServicesBlocked || !optionalFeaturesAvailable, config);
  useLaunchEvents(config, optionalFeaturesAvailable, miniMode.mini);
  useAutoUpdate(startupServicesBlocked, config, (version, url) => {
    setAutoUpdateVersion(version);
    setAutoUpdateUrl(url);
    setShowAutoUpdateConfirm(true);
  });
  useFirstLaunch(startupServicesBlocked, config, patch);

  const handleDisclosureExit = async () => {
    if (exitingForDisclosure) return;
    setExitingForDisclosure(true);
    try {
      await invokeCommand("exit_app");
    } catch (error) {
      setExitingForDisclosure(false);
      showToast("error", `退出 D2RHub 失败：${error}`);
    }
  };

  const handleDisclosureAccept = async () => {
    try {
      await applicationDisclosure.accept();
    } catch (error) {
      showToast("error", `${config?.app_language === "en-US" ? "Failed to save acceptance" : "保存使用须知确认记录失败"}：${error}`);
    }
  };

  const applicationDisclosureDialog = (
    <DisclosureDialog
      open={applicationDisclosure.required}
      language={config?.app_language === "en-US" ? "en-US" : "zh-CN"}
      target={{ type: "application", version: applicationDisclosure.version }}
      accepting={applicationDisclosure.accepting}
      canceling={exitingForDisclosure}
      onCancel={() => void handleDisclosureExit()}
      onAccept={handleDisclosureAccept}
    />
  );

  useEffect(() => {
    if (startupServicesBlocked || !optionalFeaturesAvailable || !config?.rune_audio_enabled || config.rune_audio_external_target) {
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
  }, [startupServicesBlocked, optionalFeaturesAvailable, config?.rune_audio_enabled, config?.rune_audio_target_account, config?.rune_audio_external_target, accounts, showSettings]);

  const handleReconfigure = () => {
    setView({ type: "setup", existingConfig: config ?? undefined });
  };

  const handleFeatureProfileConfirm = async (profile: FeatureProfile) => {
    try {
      await useGlobalConfig.getState().switchProfile(profile);
    } catch (error) {
      showToast("error", `保存使用模式失败：${error}`);
    }
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

  if (restarting) return (
    <AppShell>
      <div className="flex-1 flex items-center justify-center" role="status" aria-live="polite">
        <div className="flex flex-col items-center gap-4 text-center">
          <div className="w-10 h-10 rounded-full border-2 border-accent border-t-transparent animate-spin" />
          <p className="text-md text-text-primary">{config?.app_language === "en-US" ? "Restarting D2RHub…" : "正在重新启动 D2RHub…"}</p>
          <p className="text-sm text-text-muted">{config?.app_language === "en-US"
            ? "Your mode is saved. Running games will stay open."
            : "新模式已保存，正在运行的游戏会保持打开。"}</p>
        </div>
      </div>
    </AppShell>
  );

  if (applicationDisclosure.error && !applicationDisclosure.checking) return (
    <AppShell>
      <div className="flex-1 flex items-center justify-center px-6">
        <div className="w-[560px] account-line px-6 py-5" role="alert">
          <p className="text-sm font-semibold text-text-primary">
            {applicationDisclosure.error.stage === "storage"
              ? (config?.app_language === "en-US" ? "Unable to read or migrate notice acceptance" : "无法读取或迁移使用须知确认记录")
              : (config?.app_language === "en-US" ? "Unable to start D2RHub services" : "D2RHub 运行服务启动失败")}
          </p>
          <p className="text-sm text-text-secondary mt-2">
            {config?.app_language === "en-US"
              ? "Startup is paused. Retry after resolving the error. Your saved acceptance is retained."
              : "启动已暂停，请处理错误后重试。已保存的确认记录会保留。"}
          </p>
          <p className="text-xs font-mono text-error mt-3 break-all">{applicationDisclosure.error.message}</p>
          <div className="flex gap-3 mt-4">
            <Button onClick={applicationDisclosure.retry}>{config?.app_language === "en-US" ? "Retry" : "重试"}</Button>
            <Button variant="ghost" loading={exitingForDisclosure} onClick={() => void handleDisclosureExit()}>
              {config?.app_language === "en-US" ? "Exit" : "退出"}
            </Button>
          </div>
        </div>
      </div>
      <ToastContainer />
    </AppShell>
  );

  // ── loading ──
  if (view.type === "loading" || applicationDisclosure.checking) return (
    <AppShell>
      <div className="flex-1 flex items-center justify-center">
        <div className="flex flex-col items-center gap-4">
          <div className="w-10 h-10 rounded-full border-2 border-accent border-t-transparent animate-spin" />
          <p className="text-md text-text-muted">正在加载...</p>
        </div>
      </div>
    </AppShell>
  );

  // Do not mount setup or dashboard children until the application-level
  // disclosure has been accepted; several children perform detection work on mount.
  if (applicationDisclosure.required) return (
    <>
      <AppShell>
        <div className="flex-1" />
      </AppShell>
      <ToastContainer />
      {applicationDisclosureDialog}
    </>
  );

  if (view.type === "profile" && config) return (
    <>
      <AppShell>
        <FeatureProfileChooser
          config={config}
          saving={configSaving}
          onConfirm={handleFeatureProfileConfirm}
          onExit={() => void handleDisclosureExit()}
        />
        <ToastContainer />
      </AppShell>
      {applicationDisclosureDialog}
    </>
  );

  // ── setup ──
  if (view.type === "setup") return (
    <>
      <AppShell>
        <div className="flex-1 flex flex-col">
          <SetupWizard
            initialConfig={view.existingConfig}
            onComplete={() => {
              const latest = useGlobalConfig.getState().config;
              setView(latest && !isFeatureProfileDecisionCurrent(latest)
                ? { type: "profile" }
                : { type: "main" });
            }}
          />
        </div>
        <ToastContainer />
      </AppShell>
      {applicationDisclosureDialog}
    </>
  );

  // ── main ──
  const sortedAccounts = sortAccountsByCardOrder(accounts);
  const launchableAccountIds = sortedAccounts.filter(
    account => account.initialized
      && !account.is_running && batch.issue(account) === null
      && !requiresTokenMigration(account.auth_mode, account.region, config),
  ).map(account => account.id);

  return (
    <>
      <AppShell>
        {miniMode.mini ? <MiniDashboard
          busy={miniMode.busy}
          pinned={miniMode.pinned}
          onPin={miniMode.togglePin}
          onExpand={() => miniMode.switchMode(false)}
          onConfigure={account => {
            if (account && requiresTokenMigration(account.auth_mode, account.region, config)) {
              setTokenUpdateAccount(account);
            } else if (account && !account.initialized) {
              if (account.auth_mode === "token") setTokenUpdateAccount(account);
              else setReinitializeAccount(account);
            } else {
              setSettingsTab(account ? "accounts" : null);
              setSettingsAccountId(account?.id ?? null);
              setShowSettings(true);
              setSettingsRequestRevision(revision => revision + 1);
            }
          }}
          onManageGroups={() => setLaunchGroupPanelOpen(true)}
          onTasks={() => {
            const state = useLaunch.getState();
            const details = state.error || state.results.filter(result => !result.success)
              .map(result => `${accounts.find(account => account.id === result.account_id)?.display_name || result.account_id}：${result.error || "启动失败"}`)
              .join("\n\n");
            if (details) setLaunchIssueDetails(details);
            setLogRevealRevision(revision => revision + 1);
          }}
          onRoomAutomation={() => { setSettingsTab("room-automation"); setSettingsAccountId(null); setShowSettings(true); }}
          onAddAccount={() => setShowInit(true)}
          onKillAll={() => { setKillTargets(null); setShowKillConfirm(true); }}
        /> : null}
        <div className="flex-1 flex-col min-w-0 min-h-0" style={{ display: miniMode.mini ? "none" : "flex" }}>
        <Dashboard
          onMiniMode={() => {
            if (launchGroupDraft) {
              showToast("warning", "请先保存或取消当前启动方案编辑");
              return;
            }
            void miniMode.switchMode(true);
          }}
          miniModeDisabled={miniMode.busy || showSettings || showInit || showAbout || sharingReport}
          onAbout={() => setShowAbout(true)}
          onExit={async () => {
            await invokeCommand("hide_main_window");
          }}
          onOpenConfig={() => { setShowSettings(true); setSettingsTab(null); setSettingsAccountId(null); }}
          onStats={async () => {
            try {
              await invokeCommand("open_stats_page");
            } catch (e) {
              showToast("error", `打开统计失败: ${e}`);
            }
          }}
          statsModuleInstalled={optionalFeaturesAvailable
            && config?.installed_optional_modules?.includes("automation") === true}
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
            batchSelection={batch.selection}
            batchSelectableIds={batch.selectableIds}
            onSelectAllBatch={batch.selectAll}
            onClearBatch={batch.clear}
            batchBusy={killing || dispatchingBatch}
            batchUncertain={accounts.length > 0 && batch.uncertain}
            launching={launching}
            launchableAccountIds={launchableAccountIds}
            launchGroups={launchGroups}
            onCancelLaunch={cancelLaunch}
            onStartLaunch={ids => { void startDashboardLaunch(ids); }}
            onAddAccount={() => setShowInit(true)}
            onRequestKillAll={requestDashboardClose}
            launchGroupPanelOpen={launchGroupPanelOpen}
            onToggleLaunchGroupPanel={() => setLaunchGroupPanelOpen((open) => !open)}
            onOpenModManager={openModManager}
            onOpenRoomAutomation={() => {
              setSettingsTab("room-automation");
              setSettingsAccountId(null);
              setShowSettings(true);
            }}
            onOpenWindowLayouts={() => {
              setSettingsTab("window-layouts");
              setSettingsAccountId(null);
              setShowSettings(true);
            }}
            showOptionalFeatures={optionalFeaturesAvailable}
            modCapsulePool={modCapsules.pool}
          />

          {optionalFeaturesAvailable && audioModUpdate && (
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
                    setSettingsTab("mod-processing");
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

          {config?.launch_session && (launching || accounts.some(account => account.is_running && config.launch_session?.scheme.account_ids.includes(account.id))) && <p className="scheme-note px-8" role="status">本次启动：{config.launch_session.scheme.name} · 当前调整不会修改方案</p>}
          <div className="dashboard-workspace">
            {(launchGroupPanelOpen || launchGroupDraft) && (
              <LaunchGroupPanel
                editor={launchGroupDraft ? <LaunchGroupEditor launchGroups={launchGroups} modCapsulePool={modCapsules.pool} hidden={showSettings} onOpenSettings={tab => { setSettingsTab(tab); setSettingsAccountId(null); setShowSettings(true); setSettingsRequestRevision(revision => revision + 1); }} /> : undefined}
                groups={config?.launch_groups ?? []}
                accounts={accounts}
                config={config}
                modCapsulePool={modCapsules.pool}
                favoriteGroupIds={config?.favorite_launch_group_ids}
                disabled={launching || configSaving || killing || dispatchingBatch || !!batch.selection.mode || !!launchGroupDraft}
                onClose={() => { launchGroups.cancelDraft(); setLaunchGroupPanelOpen(false); }}
                onLaunch={group => {
                  setLaunchGroupPanelOpen(false);
                  launchGroups.launch(group);
                }}
                onCreate={() => {
                  setLaunchGroupPanelOpen(true);
                  launchGroups.create();
                }}
                onEdit={group => {
                  setLaunchGroupPanelOpen(true);
                  launchGroups.edit(group);
                }}
                onToggleFavorite={group => void launchGroups.toggleFavorite(group)}
              />
            )}
            {initialLoading ? (
              <AccountGridLoading />
            ) : accounts.length === 0 ? (
              <AccountGridEmpty onAddAccount={() => setShowInit(true)} onOpenRecognition={optionalFeaturesAvailable ? () => { setSettingsTab("automation"); setSettingsAccountId(null); setShowSettings(true); } : undefined} />
            ) : (
              <AccountGrid
                accounts={sortedAccounts}
                isSelectionMode={false}
                onReorder={reorderAccounts}
              >
                {sortedAccounts.map(a => {
                  const schemeMember = launchGroupDraft?.members.find(member => member.account_id === a.id);
                  return <SortableAccountCard
                    key={a.id}
                    runtimeStatus={{
                      issue: batch.issue(a),
                      mode: batch.selection.mode,
                      selected: batch.selection.ids.includes(a.id),
                      uncertain: batch.uncertain,
                      paused: batch.motionPaused,
                      activity: killing && killTargets?.includes(a.id) ? "关闭中" : undefined,
                      disabled: launching || killing || dispatchingBatch || showKillConfirm,
                      onToggle: () => batch.toggle(a),
                    }}
                    account={a}
                    onRename={renameAccount}
                    onDelete={deleteAccount}
                    onConfigure={a => { setShowSettings(true); setSettingsTab("accounts"); setSettingsAccountId(a.id); }}
                    onLaunch={id => { if (!batch.selection.mode) void startDashboardLaunch([id]); }}
                    onBattleNetOnly={id => { if (!batch.selection.mode && !batchLock.current) void startBattleNetOnly([id]); }}
                    isSelectionMode={false}
                    selected={!!schemeMember}
                    onToggleSelect={launchGroups.toggleAccount}
                    schemeMember={schemeMember}
                    onSchemeMemberChange={launchGroups.updateMember}
                    modCapsulePool={modCapsules.pool}
                    modCapsuleLoading={modCapsules.loading}
                    modCapsuleError={modCapsules.error}
                    onRequestModCapsules={modCapsules.refresh}
                    modCapsuleAssigningAccountId={modCapsules.assigningAccountId}
                    onAssignModCapsule={modCapsules.assign}
                    onOpenModManager={openModManager}
                    getPositionSchemeUsage={(accountId, positionId) => (config?.launch_groups ?? [])
                      .filter(group => group.id !== launchGroupDraft?.id)
                      .filter(group => group.members?.some(member =>
                        member.account_id === accountId
                        && member.position_configured
                        && member.position_preset_id === positionId))
                      .map(group => group.name)}
                    onUpdateToken={setTokenUpdateAccount}
                    onReinitialize={setReinitializeAccount}
                    config={config}
                  />;
                })}
              </AccountGrid>
            )}


          </div>

          {accounts.length > 0 && (
            <LaunchProgressView
              language={config?.app_language}
              accounts={accounts}
              logs={logs}
              onClear={clearLogs}
              revealRevision={logRevealRevision}
            />
          )}
        </Dashboard>
        </div>
      </AppShell>

      <AccountInitDialog
        open={showInit || !!tokenUpdateAccount || !!reinitializeAccount}
        onClose={() => { setShowInit(false); setTokenUpdateAccount(null); setReinitializeAccount(null); }}
        onDone={() => { setShowInit(false); setTokenUpdateAccount(null); setReinitializeAccount(null); }}
        updateAccount={tokenUpdateAccount}
        reinitializeAccount={reinitializeAccount}
      />
      <AboutModal open={showAbout} onClose={() => setShowAbout(false)} />
      <Modal open={launchIssueDetails !== null} onClose={() => setLaunchIssueDetails(null)}
        title={config?.app_language === "en-US" ? "Launch details" : "启动详情"}>
        <div className="max-h-[55vh] overflow-auto whitespace-pre-wrap break-words py-2 text-sm text-text-secondary">
          {launchIssueDetails}
        </div>
      </Modal>
      <Modal
        open={showKillConfirm}
        onClose={() => { if (!killing) setShowKillConfirm(false); }}
        title="确认关闭进程"
        footer={
          <div className="flex justify-end gap-2">
            <Button variant="ghost" disabled={killing} onClick={() => setShowKillConfirm(false)}>
              取消
            </Button>
            <Button variant="danger" onClick={handleKillAllD2R} loading={killing}>
              确认关闭
            </Button>
          </div>
        }
      >
        <div className="text-sm text-text-secondary py-2">
          {killTargets !== null
            ? `确定要关闭选中的 ${killTargets.length} 个账号：${killTargets.map(id => accounts.find(a => a.id === id)?.display_name || id).join("、")}？仅关闭这些账号的游戏进程。`
            : "确定要强制关闭当前系统内运行的所有暗黑破坏神II：重制版（D2R.exe）进程吗？"}
          此操作可能会导致未保存的游戏进度丢失。
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
          确定删除启动方案“{launchGroupPendingDelete?.name}”吗？账号及其配置不会被删除。
        </div>
      </Modal>
      <SettingsCenter
        open={showSettings}
        onClose={() => setShowSettings(false)}
        onReconfigure={handleReconfigure}
        onInitializeAccount={() => setShowInit(true)}
        initialTab={settingsTab}
        initialRequestRevision={settingsRequestRevision}
        initialAccountId={settingsAccountId}
      />

      <UpdateConfirmModal
        open={showAutoUpdateConfirm}
        onClose={() => setShowAutoUpdateConfirm(false)}
        version={autoUpdateVersion}
        downloadUrl={autoUpdateUrl}
      />

      <ToastContainer />
      {applicationDisclosureDialog}
    </>
  );
}

export default App;
