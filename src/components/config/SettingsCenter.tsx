import { useState, useEffect, useRef } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { invokeCommand } from "../../platform/tauri";
import { useGlobalConfig } from "../../store/globalConfig";
import { useAccounts } from "../../store/accounts";
import { useTheme } from "../../store/theme";
import { showToast } from "../ui/Toast";
import { parseShortcutFromKeyEvent, useShortcutRecorder } from "../../hooks/useShortcutRecorder";
import type { GlobalConfig } from "../../store/types";
import { validateTrackingTarget } from "../../utils/trackingTarget";
import { installationPathEditsAreInvalid } from "../../utils/installationPathChanges";
import { diffGlobalConfig } from "../../utils/globalConfigPatch";
import { sortAccountsByCardOrder } from "../../utils/accountOrder";
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
import { ModuleManagementPanel } from "../../features/settings/panels/ModuleManagementPanel";
import { TaskRuntimePanel } from "../../features/tasks";
import { useAudioModuleController } from "../../features/settings/useAudioModuleController";
import { useAuxiliaryWindowActions } from "../../features/settings/useAuxiliaryWindowActions";
import { useMaintenanceController } from "../../features/settings/useMaintenanceController";
import { useModCapsulePool } from "../../features/modCapsules/useModCapsulePool";
import { useModFeatureCoordination } from "../../features/settings/useModFeatureCoordination";
import { useAccountSettingsController } from "../../features/settings/useAccountSettingsController";
import {
  isOptionalModuleTab,
  isSettingsTabId,
  normalizeInstalledOptionalModules,
  normalizeSettingsLanguage,
  optionalModulesAfterInstall,
  optionalModulesAfterUninstall,
  type OptionalModuleTabId,
  type SettingsLanguage,
  type SettingsTabId,
} from "../../features/settings/settingsRegistry";
import { roomAutomationGateway } from "../../features/roomAutomation/gateway";
import type { RoomAutomationConfig } from "../../features/roomAutomation/types";
import { DisclosureDialog } from "../../features/disclosures/DisclosureDialog";
import {
  acceptModuleDisclosure,
  hasAcceptedModuleDisclosure,
} from "../../features/disclosures/disclosureStorage";
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

function moduleInstallSuccessMessage(language: SettingsLanguage, module: OptionalModuleTabId): string {
  if (language === "en-US") {
    if (module === "automation") {
      return "Recognition & Stats and its required Desktop Overlays module were added. Statistics and Terror Zone windows are now on.";
    }
    if (module === "overlays") {
      return "Desktop Overlays was added. Terror Zone is now on; the statistics window keeps its current setting.";
    }
    return `${module === "pet" ? "Desktop Companion" : "Room Automation"} was added to Optional Features.`;
  }
  if (module === "automation") {
    return "已添加“识别与统计”和所需的悬浮窗模块；场景统计与 TZ 播报窗口已开启";
  }
  if (module === "overlays") {
    return "已添加“桌面悬浮窗”；TZ 播报窗口已开启，场景统计窗口保持原状态";
  }
  return `“${module === "pet" ? "桌宠" : "自动跟房"}”已添加到可选功能导航`;
}

function moduleUninstallSuccessMessage(
  language: SettingsLanguage,
  module: OptionalModuleTabId,
  cascadesRecognition: boolean,
): string {
  if (language === "en-US") {
    if (module === "automation") {
      return "Recognition & Stats was removed. Recognition and statistics are off; Terror Zone keeps its current setting.";
    }
    if (cascadesRecognition) {
      return "Desktop Overlays and dependent Recognition & Stats were removed. Their running features are now off.";
    }
    if (module === "overlays") {
      return "Desktop Overlays was removed. Terror Zone is now off.";
    }
    return "The module was removed. Its settings are kept for the next time you add it.";
  }
  if (module === "automation") {
    return "“识别与统计”已卸载，识别与场景统计已关闭；TZ 播报保持当前状态";
  }
  if (cascadesRecognition) {
    return "悬浮窗依赖已移除，“识别与统计”也已卸载，相关运行功能均已关闭";
  }
  if (module === "overlays") {
    return "“桌面悬浮窗”已卸载，TZ 播报窗口已关闭";
  }
  return "模块已卸载，配置内容会保留供下次添加时继续使用";
}

function moduleActionFailureMessage(
  language: SettingsLanguage,
  action: "install" | "uninstall",
  error: unknown,
): string {
  if (language === "en-US") {
    return action === "install"
      ? "The module could not be added. Try again; if the problem continues, review the application logs."
      : "The module could not be removed. Try again; if the problem continues, review the application logs.";
  }
  return `${action === "install" ? "添加" : "卸载"}模块失败：${error}`;
}

export function SettingsCenter({ open, onClose, onReconfigure, onInitializeAccount, initialTab, initialAccountId }: Props) {
  const { config, patch: patchConfig, detectSavedGamesPath, detectGlobalSavedGamesPath, detectProgramDataAgentPath, detectAppDataRoamingBnetPath, detectBrowserPath } = useGlobalConfig();
  const { accounts, loadAccounts, renameAccount } = useAccounts();
  const { previewTheme } = useTheme();
  const initializedTrackingAccounts = accounts.filter((account) => account.initialized);
  const shortcutAccounts = sortAccountsByCardOrder(accounts);
  const trackingTarget = validateTrackingTarget(config?.rune_audio_target_account ?? "", accounts);
  const trackingTargetId = trackingTarget.valid ? trackingTarget.account.id : "";
  const installedModules = normalizeInstalledOptionalModules(config?.installed_optional_modules);
  const installedModuleKey = installedModules.join("|");
  const settingsLanguage = normalizeSettingsLanguage(config?.app_language);

  const [activeTab, setActiveTab] = useState<SettingsTabId>("accounts");
  const [settingsJsonAvailable, setSettingsJsonAvailable] = useState<Record<"CN" | "Global", boolean | null>>({ CN: null, Global: null });
  const { windowPlacementBusy, locateWindow, recoverAllWindows } = useAuxiliaryWindowActions(
    config?.app_language,
  );

  const [originalConfig, setOriginalConfig] = useState<GlobalConfig | null>(null);
  const navigationSaveRef = useRef(false);
  const [navigationSaving, setNavigationSaving] = useState(false);
  const [appearanceDraft, setAppearanceDraft] = useState<AppearanceSettingsDraft | null>(null);
  const [appearanceApplying, setAppearanceApplying] = useState(false);
  const [pendingDisclosureModule, setPendingDisclosureModule] = useState<OptionalModuleTabId | null>(null);
  const [disclosureInstallBusy, setDisclosureInstallBusy] = useState(false);

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

  const { recordingPos, setRecordingPos } = useShortcutRecorder();

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
  const {
    selectedAccountId,
    setSelectedAccountId,
    selectedAccount,
    accountHasChanges,
    accountNicknameDraft,
    setAccountNicknameDraft,
    accountWinXDraft,
    setAccountWinXDraft,
    accountWinYDraft,
    setAccountWinYDraft,
    gameSettings,
    gameSettingsLoading,
    gameSettingsLoadError,
    gameSettingsSaving,
    gameSettingsTab,
    setGameSettingsTab,
    loadGameSettings,
    updateGameSetting,
    saveAccount: handleSaveAccount,
    snapshotSystemSettings: handleSnapshotSystemSettings,
    toggleCustomizedSettings: handleToggleAccountSettingsMode,
  } = useAccountSettingsController({ accounts, loadAccounts, renameAccount });

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
      setPendingDisclosureModule(null);
      setDisclosureInstallBusy(false);
      navigationSaveRef.current = false;
    }
  }, [open]);

  useEffect(() => {
    if (open && isOptionalModuleTab(activeTab) && !installedModules.includes(activeTab)) {
      setActiveTab("module-management");
    }
  }, [activeTab, installedModuleKey, open]);

  useEffect(() => {
    if (open) {
      if (initialTab?.startsWith("mod-processing") || isSettingsTabId(initialTab)) {
        const requested = initialTab?.startsWith("mod-processing") ? "mod-processing" : initialTab as SettingsTabId;
        setActiveTab(isOptionalModuleTab(requested) && !installedModules.includes(requested)
          ? "module-management"
          : requested);
      } else {
        setActiveTab("accounts");
      }

      const activeAccounts = accounts.filter(a => a.initialized);
      if (initialAccountId) {
        setSelectedAccountId(initialAccountId);
      } else if (activeAccounts.length > 0) {
        setSelectedAccountId(activeAccounts[0].id);
      } else if (accounts.length > 0) {
        setSelectedAccountId(accounts[0].id);
      }

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

  const handleInstallModule = async (module: OptionalModuleTabId): Promise<boolean> => {
    try {
      const current = useGlobalConfig.getState().config;
      if (!current) throw new Error("全局配置尚未加载");
      const nextModules = optionalModulesAfterInstall(installedModules, module);
      const candidate: GlobalConfig = {
        ...current,
        installed_optional_modules: nextModules,
        enable_tz_overlay: module === "automation" || module === "overlays"
          ? true
          : current.enable_tz_overlay,
        enable_stats_overlay: module === "automation" ? true : current.enable_stats_overlay,
        enable_overlay: module === "automation" || module === "overlays"
          ? true
          : current.enable_overlay,
      };
      const saved = await persistGlobalDraft(candidate, true);
      if (!saved) return false;
      showToast("success", moduleInstallSuccessMessage(settingsLanguage, module));
      return true;
    } catch (error) {
      showToast("error", moduleActionFailureMessage(settingsLanguage, "install", error));
      return false;
    }
  };

  const requestInstallModule = async (module: OptionalModuleTabId) => {
    if (hasAcceptedModuleDisclosure(module)) {
      await handleInstallModule(module);
      return;
    }
    setPendingDisclosureModule(module);
  };

  const acceptAndInstallModule = async () => {
    if (!pendingDisclosureModule || disclosureInstallBusy) return;
    const module = pendingDisclosureModule;
    setDisclosureInstallBusy(true);
    try {
      const installed = await handleInstallModule(module);
      if (installed) {
        acceptModuleDisclosure(module);
        setPendingDisclosureModule(null);
      }
    } finally {
      setDisclosureInstallBusy(false);
    }
  };

  const handleUninstallModule = async (module: OptionalModuleTabId) => {
    const removesRecognitionGroup = module === "automation"
      || (module === "overlays" && installedModules.includes("automation"));
    let roomRollback: { generation: number; config: RoomAutomationConfig } | null = null;
    try {
      const current = useGlobalConfig.getState().config;
      if (!current) throw new Error("全局配置尚未加载");
      const nextModules = optionalModulesAfterUninstall(installedModules, module);
      if (module === "room-automation") {
        const snapshot = await roomAutomationGateway.getConfig();
        if (snapshot.config.enabled) {
          const disabled = await roomAutomationGateway.saveConfig(snapshot.generation, {
            ...snapshot.config,
            enabled: false,
          });
          roomRollback = {
            generation: disabled.snapshot.generation,
            config: snapshot.config,
          };
        }
      }
      const candidate: GlobalConfig = {
        ...current,
        installed_optional_modules: nextModules,
        rune_audio_enabled: removesRecognitionGroup ? false : current.rune_audio_enabled,
        enable_tz_overlay: module === "overlays"
          ? false
          : current.enable_tz_overlay,
        enable_stats_overlay: removesRecognitionGroup ? false : current.enable_stats_overlay,
        enable_bongo_cat: module === "pet" ? false : current.enable_bongo_cat,
      };
      candidate.enable_overlay = candidate.enable_tz_overlay || candidate.enable_stats_overlay;
      const saved = await persistGlobalDraft(candidate, true);
      if (!saved) {
        if (roomRollback) {
          try {
            await roomAutomationGateway.saveConfig(roomRollback.generation, roomRollback.config);
          } catch (rollbackError) {
            showToast("error", settingsLanguage === "en-US"
              ? "The module change failed and Room Automation could not be restored. Reopen its settings before continuing."
              : `模块卸载未提交，且自动跟房状态恢复失败：${rollbackError}`);
          }
        }
        return;
      }
      if (removesRecognitionGroup) {
        await invokeCommand("stop_rune_audio_monitor").catch(() => undefined);
      }
      if (isOptionalModuleTab(activeTab) && !nextModules.includes(activeTab)) setActiveTab("module-management");
      showToast(
        "success",
        moduleUninstallSuccessMessage(
          settingsLanguage,
          module,
          removesRecognitionGroup && module === "overlays",
        ),
      );
    } catch (error) {
      if (roomRollback) {
        try {
          await roomAutomationGateway.saveConfig(roomRollback.generation, roomRollback.config);
        } catch (rollbackError) {
          showToast("error", settingsLanguage === "en-US"
            ? "Room Automation could not be restored after the failed module change. Reopen its settings before continuing."
            : `卸载失败后无法恢复自动跟房状态：${rollbackError}`);
        }
      }
      showToast("error", moduleActionFailureMessage(settingsLanguage, "uninstall", error));
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
    audioProcessingMode,
    setAudioProcessingMode,
    audioProcessingTarget,
    setAudioProcessingTarget,
    includeAudioTelemetry,
    setIncludeAudioTelemetry,
    includeRoomTools,
    setIncludeRoomTools,
    includeAutoExitOnDeath,
    setIncludeAutoExitOnDeath,
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
    autoPrepareRequest,
    consumeAutoPrepareRequest,
    refreshAudioModState,
    handleAudioTargetChange,
    handleModProcessingTargetChange,
    handleAudioToggle,
    handleOpenAudioSetup,
    handleOpenModProcessing,
    handlePrepareSelectedMod,
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
  const modFeatures = useModFeatureCoordination({
    accounts, trackingTargetId, modCatalog: modCapsules, toggleAudio: handleAudioToggle,
    openProcessing: handlePrepareSelectedMod,
    onGlobalCommitted: (saved) => setOriginalConfig(JSON.parse(JSON.stringify(saved))),
  });

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

  const appearanceHasChanges = !!config && !!appearanceDraft
    && !appearanceSettingsEqual(config, appearanceDraft);
  const hasAnyUnsavedChanges = !!globalHasChanges || !!accountHasChanges || appearanceHasChanges;

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
    <>
      <SettingsShell
      open={open}
      title={`设置中心 · ${saveStatusText}`}
      activeTab={activeTab}
      config={config}
      installedModules={installedModules}
      onClose={handleClose}
      onTabChange={handleTabChange}
      dismissible={!pendingDisclosureModule}
    >
            {activeTab === "module-management" && config && (
              <ModuleManagementPanel
                config={config}
                installedModules={installedModules}
                onInstall={requestInstallModule}
                onUninstall={handleUninstallModule}
                onOpen={(module) => { void handleTabChange(module); }}
              />
            )}

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

            {activeTab === "agent" && config && (
              <LaunchStrategyPanel config={config} updateConfig={updateConfig} />
            )}

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
                includeAutoExitOnDeath={includeAutoExitOnDeath}
                setIncludeAutoExitOnDeath={setIncludeAutoExitOnDeath}
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
                onAudioToggle={modFeatures.toggleRecognition}
                onPrepareModCapsule={(accountId, capsuleId) => {
                  void modFeatures.prepareFeature(accountId, capsuleId, "recognition");
                }}
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
                audioProcessingMode={audioProcessingMode}
                setAudioProcessingMode={setAudioProcessingMode}
                audioProcessingTarget={audioProcessingTarget}
                setAudioProcessingTarget={setAudioProcessingTarget}
                includeAudioTelemetry={includeAudioTelemetry}
                setIncludeAudioTelemetry={setIncludeAudioTelemetry}
                includeRoomTools={includeRoomTools}
                setIncludeRoomTools={setIncludeRoomTools}
                includeAutoExitOnDeath={includeAutoExitOnDeath}
                setIncludeAutoExitOnDeath={setIncludeAutoExitOnDeath}
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
                autoPrepareRequest={autoPrepareRequest}
                onAutoPrepareConsumed={consumeAutoPrepareRequest}
              />
            )}

            {activeTab === "room-automation" && (
              <RoomAutomationPanel
                accounts={accounts}
                language={config?.app_language}
                modCapsulePool={modCapsules.pool}
                modCapsulePoolLoading={modCapsules.loading}
                modCapsulePoolError={modCapsules.error}
                assigningAccountId={modCapsules.assigningAccountId}
                onAssignModCapsule={modCapsules.assign}
                onRequireRoomTools={(accountId, capsuleId, autoStart) => capsuleId
                  ? void modFeatures.prepareFeature(accountId, capsuleId, "room-tools", autoStart)
                  : handleOpenModProcessing("room-tools", accountId)}
                onSaveLaunchScheme={modFeatures.saveRoomLaunchScheme}
              />
            )}

            {activeTab === "pet" && config && (
              <PetPanel
                config={config}
                windowPlacementBusy={windowPlacementBusy}
                updateConfig={updateConfig}
                persistConfig={persistGlobalDraft}
                onLocate={() => locateWindow("bongo-cat")}
              />
            )}

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

            {activeTab === "tasks" && config && (
              <TaskRuntimePanel language={config.app_language} />
            )}

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
      {pendingDisclosureModule && (
        <DisclosureDialog
          open={open}
          language={settingsLanguage}
          target={{ type: "module", module: pendingDisclosureModule }}
          accepting={disclosureInstallBusy}
          onCancel={() => {
            if (!disclosureInstallBusy) setPendingDisclosureModule(null);
          }}
          onAccept={acceptAndInstallModule}
        />
      )}
    </>
  );
}
