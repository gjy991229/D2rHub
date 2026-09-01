import { useState, useEffect, useRef } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { emitEvent, invokeCommand, listenEvent } from "../../platform/tauri";
import { useGlobalConfig } from "../../store/globalConfig";
import { useAccounts } from "../../store/accounts";
import { useTheme } from "../../store/theme";
import { showToast } from "../ui/Toast";
import { parseShortcutFromKeyEvent, useShortcutRecorder } from "../../hooks/useShortcutRecorder";
import type { SettingsMap } from "../../pages/SettingsEditor";
import type { GlobalConfig } from "../../store/types";
import { validateTrackingTarget } from "../../utils/trackingTarget";
import { installationPathEditsAreInvalid } from "../../utils/installationPathChanges";
import { diffGlobalConfig } from "../../utils/globalConfigPatch";
import { sortAccountsByCardOrder } from "../../utils/accountOrder";
import { FRAMERATE_CAP_KEY, writeFramerateCap } from "../../utils/gameSettings";
import { PathsPanel } from "../../features/settings/panels/PathsPanel";
import { SettingsShell } from "../../features/settings/SettingsShell";
import { LaunchStrategyPanel } from "../../features/settings/panels/LaunchStrategyPanel";
import { ShortcutsPanel } from "../../features/settings/panels/ShortcutsPanel";
import { MaintenancePanel } from "../../features/settings/panels/MaintenancePanel";
import { PetPanel } from "../../features/settings/panels/PetPanel";
import { AccountsPanel } from "../../features/settings/panels/AccountsPanel";
import { AppearancePanel, type AppearanceSettingsDraft } from "../../features/settings/panels/AppearancePanel";
import { OverlayPanel } from "../../features/settings/panels/OverlayPanel";
import { AutomationPanel } from "../../features/settings/panels/AutomationPanel";
import { ModProcessingPanel } from "../../features/settings/panels/ModProcessingPanel";
import { RoomAutomationPanel } from "../../features/settings/panels/RoomAutomationPanel";
import { TaskRuntimePanel } from "../../features/tasks";
import { useAudioModuleController } from "../../features/settings/useAudioModuleController";
import { useAuxiliaryWindowActions } from "../../features/settings/useAuxiliaryWindowActions";
import { useMaintenanceController } from "../../features/settings/useMaintenanceController";
import { useModCapsulePool } from "../../features/modCapsules/useModCapsulePool";
import {
  isSettingsTabId,
  type SettingsTabId,
} from "../../features/settings/settingsRegistry";
import "../../features/settings/settings.css";

interface Props {
  open: boolean;
  onClose: () => void;
  onReconfigure: () => void;
  onInitializeAccount: () => void;
  initialTab?: string | null;
  initialAccountId?: string | null;
}

function appearanceFromConfig(config: GlobalConfig): AppearanceSettingsDraft {
  return {
    app_language: config.app_language,
    theme: config.theme === "onyx" ? "onyx" : "light",
    main_opacity: config.main_opacity ?? 95,
    font_scale: config.font_scale || "default",
    separate_game_taskbar_icons: !!config.separate_game_taskbar_icons,
  };
}

function appearanceSettingsEqual(config: GlobalConfig | null, draft: AppearanceSettingsDraft | null): boolean {
  return !!config && !!draft && JSON.stringify(appearanceFromConfig(config)) === JSON.stringify(draft);
}

export function SettingsCenter({ open, onClose, onReconfigure, onInitializeAccount, initialTab, initialAccountId }: Props) {
  const { config, patch: patchConfig, detectSavedGamesPath, detectGlobalSavedGamesPath, detectProgramDataAgentPath, detectAppDataRoamingBnetPath, detectBrowserPath } = useGlobalConfig();
  const { accounts, loadAccounts, renameAccount } = useAccounts();
  const { previewTheme } = useTheme();
  const initializedTrackingAccounts = accounts.filter((account) => account.initialized);
  const shortcutAccounts = sortAccountsByCardOrder(accounts);
  const trackingTarget = validateTrackingTarget(config?.rune_audio_target_account ?? "", accounts);
  const trackingTargetId = trackingTarget.valid ? trackingTarget.account.id : "";

  // Tab and search state
  const [activeTab, setActiveTab] = useState<SettingsTabId>("accounts");
  const [settingsJsonAvailable, setSettingsJsonAvailable] = useState<Record<"CN" | "Global", boolean | null>>({ CN: null, Global: null });
  const { windowPlacementBusy, locateWindow, recoverAllWindows } = useAuxiliaryWindowActions(
    config?.app_language,
  );

  // Config backup for rollback
  const [originalConfig, setOriginalConfig] = useState<GlobalConfig | null>(null);
  const navigationSaveRef = useRef(false);
  const [navigationSaving, setNavigationSaving] = useState(false);
  const [appearanceDraft, setAppearanceDraft] = useState<AppearanceSettingsDraft | null>(null);
  const [appearanceApplying, setAppearanceApplying] = useState(false);

  useEffect(() => {
    let active = true;
    const check = async (edition: "CN" | "Global", path: string | undefined) => {
      if (!open || !path) {
        if (active) setSettingsJsonAvailable(previous => ({ ...previous, [edition]: null }));
        return;
      }
      try {
        const exists = await invokeCommand<boolean>("check_saved_games_settings", { path });
        if (active) setSettingsJsonAvailable(previous => ({ ...previous, [edition]: exists }));
      } catch {
        if (active) setSettingsJsonAvailable(previous => ({ ...previous, [edition]: false }));
      }
    };
    void check("CN", config?.cn_saved_games_path);
    void check("Global", config?.global_saved_games_path);
    return () => {
      active = false;
    };
  }, [open, config?.cn_saved_games_path, config?.global_saved_games_path]);

  // Game settings edit states (per account)
  const [selectedAccountId, setSelectedAccountId] = useState<string>("");
  const [gameSettings, setGameSettings] = useState<SettingsMap>({});
  const [gameSettingsLoading, setGameSettingsLoading] = useState(false);
  const [gameSettingsLoadError, setGameSettingsLoadError] = useState<string | null>(null);
  const [gameSettingsChanged, setGameSettingsChanged] = useState(false);
  const [gameSettingsSaving, setGameSettingsSaving] = useState(false);
  const [gameSettingsTab, setGameSettingsTab] = useState<"launch" | "game_display" | "game_graphics" | "game_audio" | "game_gameplay" | "game_automap">("launch");

  // Account card values draft states
  const [accountNicknameDraft, setAccountNicknameDraft] = useState("");
  const [accountWinXDraft, setAccountWinXDraft] = useState<number | null>(null);
  const [accountWinYDraft, setAccountWinYDraft] = useState<number | null>(null);

  // Shortcut key recording state (shared hook)
  const { recordingPos, setRecordingPos } = useShortcutRecorder();

  // Local detected paths
  const [detectedPaths, setDetectedPaths] = useState<Record<string, string | null>>({});
  const {
    exportPickerOpen,
    setExportPickerOpen,
    exportAccountIds,
    setExportAccountIds,
    plaintextRiskAcknowledged: exportPlaintextRiskAcknowledged,
    setPlaintextRiskAcknowledged: setExportPlaintextRiskAcknowledged,
    transferBusy: accountTransferBusy,
    diagnosticBusy: diagnosticExportBusy,
    toggleExportAccount,
    exportAccounts: handleExportAccounts,
    importAccounts: handleImportAccounts,
    openLogs: handleOpenLogs,
    exportDiagnostics: handleExportDiagnostics,
  } = useMaintenanceController(accounts, loadAccounts);

  // Auto initialize selected account and active tab
  // Backup config for rollback when modal opens
  useEffect(() => {
    if (open && config) {
      setOriginalConfig(JSON.parse(JSON.stringify(config)));
      setAppearanceDraft(appearanceFromConfig(config));
    }
  }, [open]);

  useEffect(() => {
    if (!open) {
      setExportPickerOpen(false);
      setExportPlaintextRiskAcknowledged(false);
      navigationSaveRef.current = false;
    }
  }, [open]);

  // Auto initialize selected account and active tab
  useEffect(() => {
    if (open) {
      if (initialTab?.startsWith("mod-processing") || isSettingsTabId(initialTab)) {
        setActiveTab(initialTab?.startsWith("mod-processing") ? "mod-processing" : initialTab as SettingsTabId);
      } else {
        setActiveTab("accounts");
      }

      // Pre-select account if provided
      const activeAccounts = accounts.filter(a => a.initialized);
      if (initialAccountId) {
        setSelectedAccountId(initialAccountId);
      } else if (activeAccounts.length > 0) {
        setSelectedAccountId(activeAccounts[0].id);
      } else if (accounts.length > 0) {
        setSelectedAccountId(accounts[0].id);
      }

      // Detect paths
      (async () => {
        const cnSavedGames = await detectSavedGamesPath();
        const globalSavedGames = await detectGlobalSavedGamesPath();
        const agent = await detectProgramDataAgentPath();
        const roaming = await detectAppDataRoamingBnetPath();
        const browser = await detectBrowserPath();
        setDetectedPaths({
          cnSavedGames,
          globalSavedGames,
          agent,
          roaming,
          browser: browser ? browser[0] : null,
        });
      })();
    }
  }, [open, initialTab, initialAccountId]);

  // Load account parameters draft when selected account changes
  useEffect(() => {
    if (selectedAccountId) {
      const acc = accounts.find(a => a.id === selectedAccountId);
      if (acc) {
        setAccountNicknameDraft(acc.display_name || acc.id);
        setAccountWinXDraft(acc.window_x !== undefined ? acc.window_x : null);
        setAccountWinYDraft(acc.window_y !== undefined ? acc.window_y : null);
        loadGameSettings(selectedAccountId);
      }
    }
  }, [selectedAccountId, accounts]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const setupListener = async () => {
      const stopListening = await listenEvent<{ accountId: string }>("account-settings-updated", (event) => {
        if (event.payload.accountId === selectedAccountId && !gameSettingsSaving) {
          loadGameSettings(selectedAccountId);
        }
      });
      if (cancelled) stopListening();
      else unlisten = stopListening;
    };
    if (selectedAccountId) {
      setupListener();
    }
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [selectedAccountId, gameSettingsSaving]);

  const loadGameSettings = async (accId: string) => {
    setGameSettingsLoading(true);
    setGameSettingsLoadError(null);
    try {
      const data = await invokeCommand<SettingsMap>("get_account_settings", { accountId: accId });
      setGameSettings(data);
      setGameSettingsChanged(false);
    } catch (e) {
      setGameSettings({});
      setGameSettingsChanged(false);
      setGameSettingsLoadError(String(e));
      showToast("error", `加载账号游戏配置失败: ${e}`);
    } finally {
      setGameSettingsLoading(false);
    }
  };

  // Close / Rollback
  const handleClose = () => {
    if (config && installationPathEditsAreInvalid(originalConfig, config)) {
      setActiveTab("paths");
      showToast("error", "请至少保留一组国服或国际服的游戏安装目录；Battle.net 仅供国服兼容模式使用");
      return;
    }
    if (navigationSaveRef.current) return;
    navigationSaveRef.current = true;
    setNavigationSaving(true);
    void commitPendingSettings().then((saved) => {
      if (saved) onClose();
    }).finally(() => {
      navigationSaveRef.current = false;
      setNavigationSaving(false);
    });
  };

  // Global Config Save
  const persistGlobalDraft = async (draft: GlobalConfig, quiet = false) => {
    if (installationPathEditsAreInvalid(originalConfig, draft)) {
      if (!quiet) showToast("error", "请至少配置一组国服或国际服的游戏安装目录；存档目录仅影响画质覆盖");
      return null;
    }
    try {
      const saved = await patchConfig(diffGlobalConfig(originalConfig, draft));
      setOriginalConfig(JSON.parse(JSON.stringify(saved)));
      if (!quiet) showToast("success", "全局设置已成功保存");
      return saved;
    } catch (e) {
      showToast("error", `保存全局设置失败: ${e}`);
      return null;
    }
  };

  // Selected Account Config Save (includes basic metadata and game settings file)
  const handleSaveAccount = async (quiet = false) => {
    if (!selectedAccountId) return true;
    setGameSettingsSaving(true);
    try {
      const acc = accounts.find(a => a.id === selectedAccountId);
      if (!acc) return false;

      // 1. Rename if modified
      if (accountNicknameDraft.trim() && accountNicknameDraft.trim() !== (acc.display_name || acc.id)) {
        const renamed = await renameAccount(selectedAccountId, accountNicknameDraft.trim());
        if (!renamed) return false;
      }

      // 2. Set Window position if modified
      if (accountWinXDraft !== acc.window_x || accountWinYDraft !== acc.window_y) {
        await invokeCommand("set_account_window_position", {
          accountId: selectedAccountId,
          windowX: accountWinXDraft,
          windowY: accountWinYDraft,
        });
      }

      // 3. Save game settings if modified
      if (gameSettingsChanged) {
        await invokeCommand("save_account_settings", {
          accountId: selectedAccountId,
          settings: gameSettings,
        });
        setGameSettingsChanged(false);
        await emitEvent("account-settings-updated", { accountId: selectedAccountId });
      }

      await loadAccounts();
      const savedName = accountNicknameDraft.trim() || acc.display_name || acc.id;
      if (!quiet) showToast("success", `账号 "${savedName}" 的设置已保存`);
      return true;
    } catch (e) {
      showToast("error", `保存账号设置失败: ${e}`);
      return false;
    } finally {
      setGameSettingsSaving(false);
    }
  };

  const applyAppearanceDraft = async (quiet = false) => {
    const current = useGlobalConfig.getState().config;
    if (!current || !appearanceDraft) return true;
    const next: GlobalConfig = {
      ...current,
      app_language: appearanceDraft.app_language,
      theme: appearanceDraft.theme,
      main_opacity: appearanceDraft.main_opacity,
      font_scale: appearanceDraft.font_scale,
      separate_game_taskbar_icons: appearanceDraft.separate_game_taskbar_icons,
    };

    setAppearanceApplying(true);
    useGlobalConfig.setState({ config: next });
    previewTheme(appearanceDraft.theme);
    document.documentElement.dataset.fontScale = appearanceDraft.font_scale;
    try { localStorage.setItem("d2rhub-font-scale", appearanceDraft.font_scale); } catch {}
    const saved = await persistGlobalDraft(next, quiet);
    setAppearanceApplying(false);
    if (!saved) {
      useGlobalConfig.setState({ config: current });
      previewTheme(current.theme === "onyx" ? "onyx" : "light");
      document.documentElement.dataset.fontScale = current.font_scale || "default";
      try { localStorage.setItem("d2rhub-font-scale", current.font_scale || "default"); } catch {}
      return false;
    }
    setAppearanceDraft(appearanceFromConfig(saved));
    return true;
  };

  const commitPendingSettings = async () => {
    const appearanceDirtyNow = !appearanceSettingsEqual(
      useGlobalConfig.getState().config,
      appearanceDraft,
    );
    if (appearanceDirtyNow) {
      if (!(await applyAppearanceDraft(true))) return false;
    } else {
      const latestConfig = useGlobalConfig.getState().config;
      if (latestConfig && originalConfig && JSON.stringify(latestConfig) !== JSON.stringify(originalConfig)) {
        if (!(await persistGlobalDraft(latestConfig, true))) return false;
      }
    }
    if (accountHasChanges && !(await handleSaveAccount(true))) return false;
    return true;
  };

  const handleSnapshotSystemSettings = async () => {
    if (!selectedAccountId) return;
    try {
      const settings = await invokeCommand<SettingsMap>("snapshot_system_settings_to_account", {
        accountId: selectedAccountId,
      });
      setGameSettings(settings);
      setGameSettingsLoadError(null);
      setGameSettingsChanged(false);
      await loadAccounts();
      await emitEvent("account-settings-updated", { accountId: selectedAccountId });
      showToast("success", "已快照系统配置到当前账号");
    } catch (e) {
      showToast("error", `快照系统配置失败: ${e}`);
    }
  };

  const handleToggleAccountSettingsMode = async (accountId: string, customized: boolean) => {
    try {
      if (customized) {
        await invokeCommand("snapshot_system_settings_to_account", { accountId });
      } else {
        await invokeCommand("set_settings_customized", { accountId, customized: false });
      }
      await loadAccounts();
      if (accountId === selectedAccountId) {
        await loadGameSettings(accountId);
      }
      await emitEvent("account-settings-updated", { accountId });
    } catch (e) {
      showToast("error", `切换配置模式失败: ${e}`);
    }
  };

  // Local Config Mutation helper
  const updateConfig = (updater: (c: GlobalConfig) => void) => {
    if (config) {
      const clone = { ...config };
      updater(clone);
      useGlobalConfig.setState({ config: clone });
    }
  };

  const {
    audioStatus,
    audioModState,
    audioModStateLoading,
    audioSetupOpen,
    setAudioSetupOpen,
    audioSetupPurpose,
    modProcessingTargetId,
    audioSetupMode,
    setAudioSetupMode,
    audioSetupSource,
    setAudioSetupSource,
    audioSetupName,
    setAudioSetupName,
    includeAudioTelemetry,
    setIncludeAudioTelemetry,
    includeRoomTools,
    setIncludeRoomTools,
    audioPreparing,
    audioPrepareProgress,
    audioModScannedAt,
    isAudioModUpgrade,
    isAudioModFeatureManagement,
    audioSetupNameError,
    showAudioSetupNameError,
    hasInitializedAudioAccount,
    hasAudioTarget,
    hasReadyAudioMod,
    isAudioEnableRequested,
    isAudioRecognitionActive,
    audioPrepareBlockedReason,
    refreshAudioModState,
    handleAudioTargetChange,
    handleModProcessingTargetChange,
    handleAudioToggle,
    handleOpenAudioSetup,
    handleOpenModProcessing,
    handlePrepareAudioMod,
    toggleAudioDiagnosticRecording,
  } = useAudioModuleController({
    open,
    activeTab,
    config,
    initializedAccounts: initializedTrackingAccounts,
    trackingTargetId,
    updateConfig,
    persistConfig: persistGlobalDraft,
    loadAccounts,
    setActiveTab,
  });
  const modProcessingTarget = validateTrackingTarget(modProcessingTargetId, accounts);
  const modCapsules = useModCapsulePool({
    active: open && ["automation", "mod-processing", "room-automation"].includes(activeTab),
    onAssigned: loadAccounts,
  });

  const updateGameSetting = (key: string, value: unknown) => {
    if (gameSettingsLoadError) return;
    setGameSettings(prev => key === FRAMERATE_CAP_KEY
      ? writeFramerateCap(prev, Number(value))
      : ({ ...prev, [key]: value }));
    setGameSettingsChanged(true);
  };

  // Path pickers
  const pickFile = async (field: keyof GlobalConfig, title: string, extensions?: string[]) => {
    try {
      const sel = await openDialog({
        multiple: false,
        title,
        filters: extensions ? [{ name: title, extensions }] : undefined,
      });
      if (sel) {
        updateConfig(c => {
          (c as any)[field] = sel;
        });
      }
    } catch (e) {
      showToast("error", `选择文件失败: ${e}`);
    }
  };

  const pickFolder = async (field: keyof GlobalConfig, title: string) => {
    try {
      const sel = await openDialog({
        multiple: false,
        directory: true,
        title,
      });
      if (sel) {
        updateConfig(c => {
          (c as any)[field] = sel;
        });
      }
    } catch (e) {
      showToast("error", `选择目录失败: ${e}`);
    }
  };

  const applyDetectedPath = (field: keyof GlobalConfig, value: string | null) => {
    if (value) {
      updateConfig(c => {
        (c as any)[field] = value;
      });
      showToast("success", "成功自动应用检测到的路径");
    } else {
      showToast("warning", "未能检测到默认路径，请手动选择");
    }
  };

  // Keyboard shortcut listener
  const handleShortcutKeyDown = (e: React.KeyboardEvent<HTMLInputElement>, pos: string) => {
    if (e.key === "Tab") {
      setRecordingPos(null);
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      setRecordingPos(null);
      e.currentTarget.blur();
      return;
    }

    e.preventDefault();
    e.stopPropagation();

    const combo = parseShortcutFromKeyEvent(e);
    if (!combo) return;

    if (config) {
      let bindings: Record<string, string> = {};
      try {
        bindings = config.shortcut_bindings_json ? JSON.parse(config.shortcut_bindings_json) : {};
      } catch {
        bindings = {};
      }
      bindings[pos] = combo;
      updateConfig(c => {
        c.shortcut_bindings_json = JSON.stringify(bindings);
      });
    }

    setRecordingPos(null);
    (e.target as HTMLInputElement).blur();
    showToast("success", `快捷键已配置为: ${combo}`);
  };

  const handleClearShortcut = (pos: string) => {
    if (config) {
      let bindings: Record<string, string> = {};
      try {
        bindings = config.shortcut_bindings_json ? JSON.parse(config.shortcut_bindings_json) : {};
      } catch {
        bindings = {};
      }
      delete bindings[pos];
      updateConfig(c => {
        c.shortcut_bindings_json = JSON.stringify(bindings);
      });
      showToast("info", "快捷键已清除");
    }
  };

  // Check if global config has changes compared to original
  const globalHasChanges = config && originalConfig && JSON.stringify(config) !== JSON.stringify(originalConfig);

  // Check if account-level settings have unsaved changes
  const accountHasChanges = (() => {
    if (!selectedAccountId) return false;
    const acc = accounts.find(a => a.id === selectedAccountId);
    if (!acc) return false;
    return (
      (accountNicknameDraft.trim() && accountNicknameDraft.trim() !== (acc.display_name || acc.id)) ||
      accountWinXDraft !== (acc.window_x ?? null) ||
      accountWinYDraft !== (acc.window_y ?? null) ||
      gameSettingsChanged
    );
  })();

  const appearanceHasChanges = !!config && !!appearanceDraft
    && !appearanceSettingsEqual(config, appearanceDraft);
  const hasAnyUnsavedChanges = !!globalHasChanges || !!accountHasChanges || appearanceHasChanges;

  const selectedAccount = accounts.find(a => a.id === selectedAccountId);
  const accountRegionLabel = (region?: string | null) =>
    region === "KR" ? "亚服" : region === "NA" ? "美服" : region === "EU" ? "欧服" : region === "Global" ? "国际服" : "国服";
  const saveStatusText = gameSettingsSaving || appearanceApplying || navigationSaving
    ? "保存中"
    : hasAnyUnsavedChanges
      ? "有未保存改动"
      : "已保存";

  const handleTabChange = (nextTab: SettingsTabId) => {
    if (nextTab === activeTab) return true;
    if (navigationSaveRef.current) return false;
    navigationSaveRef.current = true;
    setNavigationSaving(true);
    void commitPendingSettings().then((saved) => {
      if (!saved) return;
      if (nextTab === "mod-processing" && activeTab !== "mod-processing") {
        handleOpenAudioSetup("manage");
      }
      setActiveTab(nextTab);
    }).finally(() => {
      navigationSaveRef.current = false;
      setNavigationSaving(false);
    });
    return true;
  };

  const handleSelectedAccountChange = (nextAccountId: string) => {
    if (nextAccountId === selectedAccountId || navigationSaveRef.current) return;
    if (!accountHasChanges) {
      setSelectedAccountId(nextAccountId);
      return;
    }
    navigationSaveRef.current = true;
    setNavigationSaving(true);
    void handleSaveAccount(true).then((saved) => {
      if (saved) setSelectedAccountId(nextAccountId);
    }).finally(() => {
      navigationSaveRef.current = false;
      setNavigationSaving(false);
    });
  };

  return (
    <SettingsShell
      open={open}
      title={`设置中心 · ${saveStatusText}`}
      activeTab={activeTab}
      config={config}
      onClose={handleClose}
      onTabChange={handleTabChange}
    >
            {/* 1. Paths Tab */}
            {activeTab === "paths" && config && (
              <PathsPanel
                config={config}
                settingsAvailable={settingsJsonAvailable}
                detectedPaths={detectedPaths}
                updateConfig={updateConfig}
                pickFile={pickFile}
                pickFolder={pickFolder}
                applyDetectedPath={applyDetectedPath}
              />
            )}

            {/* Non-disableable multi-instance core. */}
            {activeTab === "accounts" && (
              <AccountsPanel
                accounts={accounts}
                selectedAccountId={selectedAccountId}
                selectedAccount={selectedAccount}
                setSelectedAccountId={handleSelectedAccountChange}
                accountHasChanges={!!accountHasChanges}
                saveAccount={handleSaveAccount}
                toggleCustomizedSettings={handleToggleAccountSettingsMode}
                accountRegionLabel={accountRegionLabel}
                gameSettingsTab={gameSettingsTab}
                setGameSettingsTab={setGameSettingsTab}
                snapshotSystemSettings={handleSnapshotSystemSettings}
                accountNicknameDraft={accountNicknameDraft}
                setAccountNicknameDraft={setAccountNicknameDraft}
                onOpenModManager={() => { handleOpenAudioSetup("manage"); setActiveTab("mod-processing"); }}
                accountWinXDraft={accountWinXDraft}
                setAccountWinXDraft={setAccountWinXDraft}
                accountWinYDraft={accountWinYDraft}
                setAccountWinYDraft={setAccountWinYDraft}
                gameSettings={gameSettings}
                gameSettingsLoading={gameSettingsLoading}
                gameSettingsLoadError={gameSettingsLoadError}
                updateGameSetting={updateGameSetting}
                loadGameSettings={loadGameSettings}
              />
            )}

            {/* Required launch strategy for the multi-instance core. */}
            {activeTab === "agent" && config && (
              <LaunchStrategyPanel config={config} updateConfig={updateConfig} />
            )}

            {/* Required application appearance preferences. */}
            {activeTab === "appearance" && config && (
              <AppearancePanel
                draft={appearanceDraft ?? appearanceFromConfig(config)}
                dirty={appearanceHasChanges}
                applying={appearanceApplying}
                onChange={(patch) => setAppearanceDraft((current) => ({
                  ...(current ?? appearanceFromConfig(config)),
                  ...patch,
                }))}
                onApply={() => applyAppearanceDraft(false)}
              />
            )}

            {/* Optional desktop overlay capability. */}
            {activeTab === "overlays" && config && (
              <OverlayPanel
                config={config}
                updateConfig={updateConfig}
                persistConfig={persistGlobalDraft}
                windowPlacementBusy={windowPlacementBusy}
                locateWindow={locateWindow}
                recoverAllWindows={recoverAllWindows}
              />
            )}

            {/* Optional audio recognition and run statistics module. */}
            {activeTab === "automation" && config && (
              <AutomationPanel
                config={config}
                updateConfig={updateConfig}
                persistConfig={persistGlobalDraft}
                initializedTrackingAccounts={initializedTrackingAccounts}
                trackingTarget={trackingTarget}
                audioStatus={audioStatus}
                audioModState={audioModState}
                audioModStateLoading={audioModStateLoading}
                modCapsulePool={modCapsules.pool}
                assigningCapsuleAccountId={modCapsules.assigningAccountId}
                onAssignModCapsule={async (accountId, capsuleId) => {
                  const next = await modCapsules.assign(accountId, capsuleId);
                  if (next && accountId === trackingTargetId) await refreshAudioModState();
                  return next;
                }}
                audioSetupOpen={audioSetupOpen}
                onOpenModProcessing={() => handleOpenModProcessing("recognition")}
                onOpenAudioSetup={handleOpenAudioSetup}
                onCloseAudioSetup={() => setAudioSetupOpen(false)}
                audioSetupMode={audioSetupMode}
                setAudioSetupMode={setAudioSetupMode}
                audioSetupSource={audioSetupSource}
                setAudioSetupSource={setAudioSetupSource}
                audioSetupName={audioSetupName}
                setAudioSetupName={setAudioSetupName}
                includeAudioTelemetry={includeAudioTelemetry}
                setIncludeAudioTelemetry={setIncludeAudioTelemetry}
                includeRoomTools={includeRoomTools}
                setIncludeRoomTools={setIncludeRoomTools}
                audioPreparing={audioPreparing}
                audioPrepareProgress={audioPrepareProgress}
                isAudioModUpgrade={isAudioModUpgrade}
                isAudioModFeatureManagement={isAudioModFeatureManagement}
                audioSetupNameError={audioSetupNameError}
                showAudioSetupNameError={showAudioSetupNameError}
                hasInitializedAudioAccount={hasInitializedAudioAccount}
                hasAudioTarget={hasAudioTarget}
                hasReadyAudioMod={hasReadyAudioMod}
                isAudioEnableRequested={isAudioEnableRequested}
                isAudioRecognitionActive={isAudioRecognitionActive}
                audioPrepareBlockedReason={audioPrepareBlockedReason}
                onAudioTargetChange={handleAudioTargetChange}
                onAudioToggle={handleAudioToggle}
                onPrepareAudioMod={handlePrepareAudioMod}
                onToggleDiagnosticRecording={toggleAudioDiagnosticRecording}
                onClose={handleClose}
                onInitializeAccount={onInitializeAccount}
              />
            )}

            {activeTab === "mod-processing" && config && (
              <ModProcessingPanel
                config={config}
                initializedAccounts={initializedTrackingAccounts}
                trackingTarget={modProcessingTarget}
                audioModState={audioModState}
                audioModStateLoading={audioModStateLoading}
                audioModScannedAt={audioModScannedAt}
                purpose={audioSetupPurpose}
                audioSetupMode={audioSetupMode}
                setAudioSetupMode={setAudioSetupMode}
                audioSetupSource={audioSetupSource}
                setAudioSetupSource={setAudioSetupSource}
                audioSetupName={audioSetupName}
                setAudioSetupName={setAudioSetupName}
                includeAudioTelemetry={includeAudioTelemetry}
                setIncludeAudioTelemetry={setIncludeAudioTelemetry}
                includeRoomTools={includeRoomTools}
                setIncludeRoomTools={setIncludeRoomTools}
                audioPreparing={audioPreparing}
                audioPrepareProgress={audioPrepareProgress}
                isAudioModUpgrade={isAudioModUpgrade}
                isAudioModFeatureManagement={isAudioModFeatureManagement}
                audioSetupNameError={audioSetupNameError}
                showAudioSetupNameError={showAudioSetupNameError}
                audioPrepareBlockedReason={audioPrepareBlockedReason}
                modCapsulePool={modCapsules.pool}
                modCapsulePoolLoading={modCapsules.loading}
                modCapsulePoolError={modCapsules.error}
                modCatalog={modCapsules} openAddRequest={initialTab?.startsWith("mod-processing:add")} initialEdition={initialTab?.split(":")[2]}
                onTargetChange={handleModProcessingTargetChange}
                onPrepare={async () => {
                  await handlePrepareAudioMod();
                  await modCapsules.refresh();
                }}
                onRefresh={async () => {
                  await refreshAudioModState();
                  await modCapsules.refresh();
                }}
                onBackToRecognition={() => setActiveTab("automation")}
              />
            )}

            {/* Optional keyboard-only room creation and follower joining module. */}
            {activeTab === "room-automation" && (
              <RoomAutomationPanel
                accounts={accounts}
                language={config?.app_language}
                modCapsulePool={modCapsules.pool}
                modCapsulePoolLoading={modCapsules.loading}
                modCapsulePoolError={modCapsules.error}
                assigningAccountId={modCapsules.assigningAccountId}
                onAssignModCapsule={modCapsules.assign}
                onRequireRoomTools={(accountId) => { handleOpenModProcessing("room-tools", accountId); }}
              />
            )}

            {/* Optional desktop companion module. */}
            {activeTab === "pet" && config && (
              <PetPanel
                config={config}
                windowPlacementBusy={windowPlacementBusy}
                updateConfig={updateConfig}
                persistConfig={persistGlobalDraft}
                onLocate={() => locateWindow("bongo-cat")}
              />
            )}

            {/* Required shortcut routing for multi-instance windows. */}
            {activeTab === "shortcuts" && config && (
              <ShortcutsPanel
                config={config}
                accounts={shortcutAccounts}
                recordingPosition={recordingPos}
                setRecordingPosition={setRecordingPos}
                onKeyDown={handleShortcutKeyDown}
                onClear={handleClearShortcut}
              />
            )}

            {/* Required application maintenance and compatibility tools. */}
            {activeTab === "tasks" && config && (
              <TaskRuntimePanel language={config.app_language} />
            )}

            {/* Required application maintenance and compatibility tools. */}
            {activeTab === "advanced" && config && (
              <MaintenancePanel
                accounts={accounts}
                transferBusy={accountTransferBusy}
                exportPickerOpen={exportPickerOpen}
                setExportPickerOpen={setExportPickerOpen}
                exportAccountIds={exportAccountIds}
                setExportAccountIds={setExportAccountIds}
                plaintextRiskAcknowledged={exportPlaintextRiskAcknowledged}
                setPlaintextRiskAcknowledged={setExportPlaintextRiskAcknowledged}
                onToggleExportAccount={toggleExportAccount}
                onExport={handleExportAccounts}
                onImport={handleImportAccounts}
                onOpenLogs={handleOpenLogs}
                diagnosticBusy={diagnosticExportBusy}
                onExportDiagnostics={handleExportDiagnostics}
                onRunSetup={() => {
                  onClose();
                  onReconfigure();
                }}
              />
            )}
    </SettingsShell>
  );
}
