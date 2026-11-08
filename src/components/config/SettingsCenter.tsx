import { lazy, Suspense, useState, useEffect, useRef } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { invokeCommand } from "../../platform/tauri";
import { useGlobalConfig } from "../../store/globalConfig";
import { useAccounts } from "../../store/accounts";
import { showToast } from "../ui/Toast";
import { flushWindowGeometrySaves } from "../../hooks/windowGeometryFlush";
import type { GlobalConfig } from "../../store/types";
import { validateTrackingTarget } from "../../utils/trackingTarget";
import { installationPathEditsAreInvalid } from "../../utils/installationPathChanges";
import { validatePathSelection } from "../../utils/pathSelection";
import { useGlobalSettingsDraft } from "../../features/settings/useGlobalSettingsDraft";
import { sortAccountsByCardOrder } from "../../utils/accountOrder";
import { PathsPanel } from "../../features/settings/panels/PathsPanel";
import { SettingsShell } from "../../features/settings/SettingsShell";
import { LaunchStrategyPanel } from "../../features/settings/panels/LaunchStrategyPanel";
import { ShortcutsPanel } from "../../features/settings/panels/ShortcutsPanel";
import { MaintenancePanel } from "../../features/settings/panels/MaintenancePanel";
import { AccountsPanel } from "../../features/settings/panels/AccountsPanel";
import { AppearancePanel } from "../../features/settings/panels/AppearancePanel";
import {
  appearanceFromConfig,
  useAppearanceSettingsController,
} from "../../features/settings/useAppearanceSettingsController";


import { useModWorkflow } from "../../features/mods/workflow/useModWorkflow";

import { ModuleManagementPanel } from "../../features/settings/panels/ModuleManagementPanel";
import { TaskRuntimePanel } from "../../features/tasks";
import { useAudioModuleController } from "../../features/settings/useAudioModuleController";
import { useAuxiliaryWindowActions } from "../../features/settings/useAuxiliaryWindowActions";
import { useMaintenanceController } from "../../features/settings/useMaintenanceController";
import { useModCapsulePool } from "../../features/modCapsules/useModCapsulePool";
import { useModFeatureCoordination } from "../../features/settings/useModFeatureCoordination";
import { useAccountSettingsController } from "../../features/settings/useAccountSettingsController";
import { useOptionalModuleController } from "../../features/settings/useOptionalModuleController";
import { useShortcutBindingController } from "../../features/settings/useShortcutBindingController";
import {
  isOptionalModuleTab,
  isSettingsTabAvailableInMinimal,
  isSettingsTabId,
  normalizeInstalledOptionalModules,
  normalizeSettingsLanguage,
  type SettingsTabId,
} from "../../features/settings/settingsRegistry";
import {
  isMinimalMode,
  normalizeFeatureProfile,
  type FeatureProfile,
} from "../../features/profile/featureProfile";
import { DisclosureDialog } from "../../features/disclosures/DisclosureDialog";
import "../../features/settings/settings.css";

const ModWorkspace = lazy(() => import("../../features/mods/workflow/ModWorkspace").then(module => ({ default: module.ModWorkspace })));
const AutomationPanel = lazy(() => import("../../features/settings/panels/AutomationPanel").then(module => ({ default: module.AutomationPanel })));
const RoomAutomationPanel = lazy(() => import("../../features/settings/panels/RoomAutomationPanel").then(module => ({ default: module.RoomAutomationPanel })));
const OverlayPanel = lazy(() => import("../../features/settings/panels/OverlayPanel").then(module => ({ default: module.OverlayPanel })));
const PetPanel = lazy(() => import("../../features/settings/panels/PetPanel").then(module => ({ default: module.PetPanel })));
const WindowLayoutPanel = lazy(() => import("../../features/windowLayouts/WindowLayoutPanel").then(module => ({ default: module.WindowLayoutPanel })));

interface Props {
  open: boolean;
  onClose: () => void;
  onReconfigure: () => void;
  onInitializeAccount: () => void;
  initialTab?: string | null;
  initialRequestRevision?: number;
  initialAccountId?: string | null;
}

export function SettingsCenter({ open, onClose, onReconfigure, onInitializeAccount, initialTab, initialAccountId, initialRequestRevision }: Props) {
  const { config: committedConfig, patch: patchConfig, detectSavedGamesPath, detectGlobalSavedGamesPath, detectProgramDataAgentPath, detectAppDataRoamingBnetPath, detectBrowserPath } = useGlobalConfig();
  const { config, updateConfig, persistDraft, getDraft, hasPendingChanges, hasChanges: globalHasChanges } = useGlobalSettingsDraft({
    open,
    committedConfig,
    readCommitted: () => useGlobalConfig.getState().config,
    persistPatch: patchConfig,
  });
  const { accounts, loadAccounts, renameAccount } = useAccounts();
  const initializedTrackingAccounts = accounts.filter((account) => account.initialized);
  const shortcutAccounts = sortAccountsByCardOrder(accounts);
  const trackingTarget = validateTrackingTarget(config?.rune_audio_target_account ?? "", accounts);
  const trackingTargetId = trackingTarget.valid ? trackingTarget.account.id : "";
  const installedModules = normalizeInstalledOptionalModules(config?.installed_optional_modules);
  const installedModuleKey = installedModules.join("|");
  const settingsLanguage = normalizeSettingsLanguage(config?.app_language);
  const minimalMode = isMinimalMode(config);

  const [activeTab, setActiveTab] = useState<SettingsTabId>("accounts");
  const [settingsJsonAvailable, setSettingsJsonAvailable] = useState<Record<"CN" | "Global", boolean | null>>({ CN: null, Global: null });
  const { windowPlacementBusy, locateWindow, recoverAllWindows } = useAuxiliaryWindowActions(
    config?.app_language,
  );

  const navigationSaveRef = useRef(false);
  const [navigationSaving, setNavigationSaving] = useState(false);
  const [layoutDraftDirty, setLayoutDraftDirty] = useState(false);
  const [layoutSaving, setLayoutSaving] = useState(false);
  const [saveIssue, setSaveIssue] = useState<{ tab: SettingsTabId; message: string } | null>(null);
  useEffect(() => { if (!open) setSaveIssue(null); }, [open]);
  const [profileChanging, setProfileChanging] = useState(false);

  // 快捷键录入、冲突校验与写回由控制器持有；shell 只负责渲染与保存编排。
  const {
    recordingPos,
    setRecordingPos,
    shortcutErrors,
    checkingShortcut,
    handleShortcutKeyDown,
    handleClearShortcut,
  } = useShortcutBindingController({ open, activeTab, config, updateConfig });

  useEffect(() => {
    let active = true;
    const check = async (edition: "CN" | "Global", path: string | undefined) => {
      if (!open || activeTab !== "paths" || !path) {
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
  }, [open, activeTab, config?.cn_saved_games_path, config?.global_saved_games_path]);

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

  useEffect(() => {
    if (!open) {
      setExportPickerOpen(false);
      setExportPlaintextRiskAcknowledged(false);
      setRecordingPos(null);
      navigationSaveRef.current = false;
    }
  }, [open, setRecordingPos]);

  useEffect(() => {
    if (!open) return;
    if (minimalMode && !isSettingsTabAvailableInMinimal(activeTab)) {
      setActiveTab("accounts");
    } else if (!minimalMode && isOptionalModuleTab(activeTab) && !installedModules.includes(activeTab)) {
      setActiveTab("module-management");
    }
  }, [activeTab, installedModuleKey, minimalMode, open]);

  useEffect(() => {
    if (open) {
      if (initialTab?.startsWith("mod-processing") || isSettingsTabId(initialTab)) {
        const requested = initialTab?.startsWith("mod-processing") ? "mod-processing" : initialTab as SettingsTabId;
        if (initialTab?.startsWith("mod-processing")) {
          const edition = initialTab.split(":")[2];
          modWorkflow.actions.openLibrary(edition === "CN" || edition === "Global" ? edition : undefined, initialTab.startsWith("mod-processing:add"));
        }
        setActiveTab(minimalMode && !isSettingsTabAvailableInMinimal(requested)
          ? "accounts"
          : isOptionalModuleTab(requested) && !installedModules.includes(requested)
            ? "module-management"
            : requested);
      } else {
        setActiveTab("accounts");
      }

    }
  }, [open, initialTab, initialRequestRevision]);

  // Navigation to a companion panel must not replace an account draft by
  // resetting the selected account to the first item in the list.
  const accountPickerOpened = useRef(false);
  useEffect(() => {
    if (!open) { accountPickerOpened.current = false; return; }
    const opening = !accountPickerOpened.current;
    accountPickerOpened.current = true;
    if (!opening && !initialAccountId) return;
    const activeAccounts = accounts.filter(a => a.initialized);
    if (initialAccountId) {
      setSelectedAccountId(initialAccountId);
    } else if (activeAccounts.length > 0) {
      setSelectedAccountId(activeAccounts[0].id);
    } else if (accounts.length > 0) {
      setSelectedAccountId(accounts[0].id);
    }
  }, [open, initialAccountId]);

  // Path discovery belongs to the path page, not every settings visit.
  useEffect(() => {
    if (!open || activeTab !== "paths") return;
    let cancelled = false;
    void Promise.allSettled([
      detectSavedGamesPath(), detectGlobalSavedGamesPath(), detectProgramDataAgentPath(),
      detectAppDataRoamingBnetPath(), detectBrowserPath(),
    ]).then(results => {
      if (cancelled) return;
      const value = (index: number) => results[index].status === "fulfilled" ? results[index].value : null;
      const browser = value(4);
      setDetectedPaths({ cnSavedGames: value(0) as string | null, globalSavedGames: value(1) as string | null,
        agent: value(2) as string | null, roaming: value(3) as string | null,
        browser: Array.isArray(browser) ? browser[0] : null });
    });
    return () => { cancelled = true; };
  }, [open, activeTab, detectSavedGamesPath, detectGlobalSavedGamesPath, detectProgramDataAgentPath, detectAppDataRoamingBnetPath, detectBrowserPath]);

  // Close / Rollback
  const handleClose = () => {
    if (layoutSaving) return;
    if (config && installationPathEditsAreInvalid(committedConfig, config)) {
      setSaveIssue({ tab: "paths", message: "游戏安装目录尚未填写完整。修改仍保留在当前草稿中。" });
      return;
    }
    if (navigationSaveRef.current) return;
    navigationSaveRef.current = true;
    setNavigationSaving(true);
    void commitPendingSettings().then((saved) => {
      if (saved) { setSaveIssue(null); onClose(); }
      else setSaveIssue({ tab: activeTab, message: "部分修改尚未保存，草稿已保留。你可以继续浏览，稍后重试。" });
    }).finally(() => {
      navigationSaveRef.current = false;
      setNavigationSaving(false);
    });
  };

  // Global Config Save
  const persistGlobalDraft = async (draft: GlobalConfig, quiet = false) => {
    if (installationPathEditsAreInvalid(useGlobalConfig.getState().config, draft)) {
      showToast("error", "请至少配置一组国服或国际服的游戏安装目录；存档目录仅影响画质覆盖");
      return null;
    }
    try {
      const saved = await persistDraft(draft);
      if (!quiet) showToast("success", "全局设置已成功保存");
      return saved;
    } catch (e) {
      showToast("error", `保存全局设置失败: ${e}`);
      return null;
    }
  };

  const {
    pendingDisclosureModule,
    disclosureInstallBusy,
    requestInstallModule,
    acceptAndInstallModule,
    handleUninstallModule,
    dismissPendingDisclosure,
  } = useOptionalModuleController({
    open,
    config,
    installedModules,
    language: settingsLanguage,
    activeTab,
    persistGlobalDraft,
    setActiveTab,
  });

  const {
    draft: appearanceDraft,
    setDraft: setAppearanceDraft,
    applying: appearanceApplying,
    hasChanges: appearanceHasChanges,
    apply: applyAppearanceDraft,
  } = useAppearanceSettingsController({
    open,
    config,
    committedConfig,
    updateConfig,
    persistConfig: persistGlobalDraft,
  });

  const commitPendingSettings = async () => {
    const latestDraft = getDraft();
    if (latestDraft && !(await persistGlobalDraft(latestDraft, true))) return false;
    if (accountHasChanges && !(await handleSaveAccount(true))) return false;
    if (hasPendingChanges()) {
      showToast("info", "保存期间有新的修改，请确认后再次保存");
      return false;
    }
    return true;
  };

  const handleChangeFeatureProfile = async (profile: FeatureProfile) => {
    const current = useGlobalConfig.getState().config;
    if (!current || normalizeFeatureProfile(current.feature_profile) === profile) return true;
    if (navigationSaveRef.current) return false;
    navigationSaveRef.current = true;
    setNavigationSaving(true);
    setProfileChanging(true);
    setRecordingPos(null);
    try {
      if (!(await commitPendingSettings())) return false;
      await flushWindowGeometrySaves();
      await useGlobalConfig.getState().switchProfile(profile);
      if (useGlobalConfig.getState().restarting) return true;
      setActiveTab("advanced");
      showToast("success", profile === "minimal"
        ? "已选择纯净模式；扩展运行实例将保持未加载"
        : "已切换到正常模式；模块将按原配置启动，启动异常可在模块设置查看");
      return true;
    } catch (error) {
      showToast("error", `切换使用模式失败：${error}`);
      return false;
    } finally {
      setProfileChanging(false);
      navigationSaveRef.current = false;
      setNavigationSaving(false);
    }
  };

  const modCapsules = useModCapsulePool({
    active: open && ["automation", "mod-processing", "room-automation"].includes(activeTab),
    onAssigned: loadAccounts,
  });
  const audio = useAudioModuleController({
    open, activeTab, config, initializedAccounts: initializedTrackingAccounts, trackingTargetId,
    updateConfig, persistConfig: persistGlobalDraft, optionalFeaturesAvailable: !minimalMode,
    requestProcessing: request => modWorkflow.actions.requestProcessing(request),
  });
  const modWorkflow = useModWorkflow({
    open, active: activeTab === "mod-processing", accounts, catalog: modCapsules,
    language: config?.app_language, optionalFeaturesAvailable: !minimalMode,
    installationPaths: { CN: config?.cn_game_path ?? "", Global: config?.global_game_path ?? "" },
    onNavigate: origin => setActiveTab(origin === "recognition" ? "automation" : origin === "room-automation" ? "room-automation" : "mod-processing"),
    onApplied: async result => { if (!result.installationEdition) await loadAccounts(); await audio.completeModProcessing(result); },
  });
  const modFeatures = useModFeatureCoordination({
    accounts, trackingTargetId, modCatalog: modCapsules, toggleAudio: audio.handleAudioToggle,
    openProcessing: modWorkflow.actions.requestProcessing,
    onGlobalCommitted: () => { /* The edit session rebases from the committed store. */ },
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
        await validatePathSelection(field, sel as string);
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
        await validatePathSelection(field, sel as string);
        updateConfig(c => {
          (c as any)[field] = sel;
        });
      }
    } catch (e) {
      showToast("error", `选择目录失败: ${e}`);
    }
  };

  const applyDetectedPath = async (field: keyof GlobalConfig, value: string | null) => {
    if (value) {
      try {
        await validatePathSelection(field, value);
      } catch (error) {
        showToast("error", String(error));
        return;
      }
      updateConfig(c => {
        (c as any)[field] = value;
      });
      showToast("success", "成功自动应用检测到的路径");
    } else {
      showToast("warning", "未能检测到默认路径，请手动选择");
    }
  };

  const hasAnyUnsavedChanges = !!globalHasChanges || !!accountHasChanges || appearanceHasChanges;

  const accountRegionLabel = (region?: string | null) =>
    region === "KR" ? "亚服" : region === "NA" ? "美服" : region === "EU" ? "欧服" : region === "Global" ? "国际服" : "国服";
  const saveStatusText = gameSettingsSaving || appearanceApplying || navigationSaving || layoutSaving
    ? "保存中"
    : saveIssue ? (settingsLanguage === "en-US" ? "Unsaved draft kept" : "未保存 · 草稿保留")
    : layoutDraftDirty
      ? (settingsLanguage === "en-US" ? "Layout draft unsaved" : "布局草稿未保存")
      : hasAnyUnsavedChanges
      ? "有未保存改动"
      : "已保存";

  const handleTabChange = async (nextTab: SettingsTabId): Promise<boolean> => {
    if (layoutSaving || navigationSaveRef.current) return false;
    if (nextTab === activeTab) {
      if (nextTab === "mod-processing" && !modWorkflow.busy) modWorkflow.actions.openLibrary();
      return true;
    }
    navigationSaveRef.current = true;
    setNavigationSaving(true);
    try {
      if (!saveIssue && !(await commitPendingSettings())) {
        setSaveIssue({ tab: activeTab, message: "部分修改尚未保存，草稿已保留。你可以继续浏览，稍后重试。" });
      }
      if (nextTab === "mod-processing" && !modWorkflow.busy) modWorkflow.actions.openLibrary();
      setActiveTab(nextTab);
      return true;
    } finally {
      navigationSaveRef.current = false;
      setNavigationSaving(false);
    }
  };

  const retryPendingSettings = async () => {
    if (navigationSaveRef.current) return;
    navigationSaveRef.current = true;
    setNavigationSaving(true);
    try { if (await commitPendingSettings()) setSaveIssue(null); }
    finally { navigationSaveRef.current = false; setNavigationSaving(false); }
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
      dismissible={!pendingDisclosureModule && !profileChanging && !layoutSaving}
    >
      {modWorkflow.busy && activeTab !== "mod-processing" && <div className="settings-save-feedback" role="status">
        <span>{settingsLanguage === "en-US" ? "Mod processing continues in the background." : "Mod 正在后台加工，可以继续使用其他设置。"}</span>
        <button type="button" className="control-btn" onClick={() => setActiveTab("mod-processing")}>{settingsLanguage === "en-US" ? "View progress" : "查看进度"}</button>
      </div>}
      {saveIssue && <div className="settings-save-feedback" role="alert">
        <p>{settingsLanguage === "en-US" ? "Some changes are unsaved. Your draft is kept while you browse settings." : saveIssue.message}</p>
        <div>
          <button type="button" className="control-btn" onClick={() => setActiveTab(saveIssue.tab)}>{settingsLanguage === "en-US" ? "Return to edits" : "返回修改处"}</button>
          <button type="button" className="control-btn" disabled={navigationSaving} onClick={() => void retryPendingSettings()}>{settingsLanguage === "en-US" ? "Retry save" : "重试保存"}</button>
          <button type="button" className="control-btn" disabled={navigationSaving} onClick={() => { setSelectedAccountId(""); setSaveIssue(null); onClose(); }}>{settingsLanguage === "en-US" ? "Discard unsaved changes and close" : "放弃未保存修改并关闭"}</button>
        </div>
      </div>}
      <Suspense fallback={<div role="status" className="p-3 text-sm text-text-muted">{settingsLanguage === "en-US" ? "Loading settings…" : "正在加载设置…"}</div>}>
            {!minimalMode && activeTab === "module-management" && config && (
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
                onOpenModManager={() => { void handleTabChange("mod-processing"); }}
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
              <LaunchStrategyPanel config={config} accounts={accounts} updateConfig={updateConfig} />
            )}

            {activeTab === "window-layouts" && config && (
              <WindowLayoutPanel config={config} updateConfig={updateConfig} persistConfig={persistGlobalDraft} readDraft={getDraft} onDraftChange={setLayoutDraftDirty} onSavingChange={setLayoutSaving} />
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
                navigationSaving={navigationSaving}
                onOpenOverlaySettings={!minimalMode && installedModules.includes("overlays")
                  ? () => { void handleTabChange("overlays"); }
                  : undefined}
              />
            )}

            {!minimalMode && activeTab === "overlays" && config && (
              <OverlayPanel
                config={config}
                updateConfig={updateConfig}
                persistConfig={persistGlobalDraft}
                windowPlacementBusy={windowPlacementBusy}
                locateWindow={locateWindow}
                recoverAllWindows={recoverAllWindows}
              />
            )}

            {!minimalMode && activeTab === "automation" && config && (
              <AutomationPanel
                config={config}
                updateConfig={updateConfig}
                persistConfig={persistGlobalDraft}
                initializedTrackingAccounts={initializedTrackingAccounts}
                trackingTarget={trackingTarget}
                audioStatus={audio.audioStatus}
                audioModState={audio.audioModState}
                audioModStateLoading={audio.audioModStateLoading}
                modCapsulePool={modCapsules.pool}
                assigningCapsuleAccountId={modCapsules.assigningAccountId}
                onAssignModCapsule={async (accountId, capsuleId) => {
                  const next = await modCapsules.assign(accountId, capsuleId);
                  if (next && accountId === trackingTargetId) await audio.refreshAudioModState();
                  return next;
                }}
                onOpenModProcessing={() => modWorkflow.actions.requestProcessing(config.rune_audio_external_target
                  ? { origin: "recognition", installationOnly: true, edition: config.rune_audio_external_target.edition,
                    ...(config.rune_audio_external_target.mod_name ? { source: { name: config.rune_audio_external_target.mod_name, processed: !!audio.audioModState?.feature_groups.length || !!audio.audioModState?.update_required } } : {}) }
                  : { origin: "recognition", accountId: trackingTargetId })}
                audioPreparing={modWorkflow.busy}
                hasInitializedAudioAccount={audio.hasInitializedAudioAccount}
                hasAudioTarget={audio.hasAudioTarget}
                hasReadyAudioMod={audio.hasReadyAudioMod}
                isAudioEnableRequested={audio.isAudioEnableRequested}
                isAudioRecognitionActive={audio.isAudioRecognitionActive}
                externalInstances={audio.externalInstances}
                externalInstancesError={audio.externalInstancesError}
                onExternalTargetChange={audio.handleExternalTargetChange}
                onSelectExternalInstance={audio.selectExternalInstance}
                onAudioTargetChange={audio.handleAudioTargetChange}
                onAudioToggle={audio.handleAudioToggle}
                onPrepareModCapsule={(accountId, capsuleId) => {
                  void modFeatures.prepareFeature(accountId, capsuleId, "recognition");
                }}
                onToggleDiagnosticRecording={audio.toggleAudioDiagnosticRecording}
                onClose={handleClose}
                onInitializeAccount={onInitializeAccount}
              />
            )}

            {activeTab === "mod-processing" && config && (
              <ModWorkspace workflow={modWorkflow} />
            )}

            {!minimalMode && activeTab === "room-automation" && (
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
                  : modWorkflow.actions.requestProcessing({ origin: "room-automation", accountId })}
                onSaveLaunchScheme={modFeatures.saveRoomLaunchScheme}
              />
            )}

            {open && !minimalMode && installedModules.includes("pet") && activeTab === "pet" && config && (
              <Suspense fallback={<div role="status" className="p-3 text-sm text-text-muted">{config.app_language === "en-US" ? "Loading companion settings…" : "正在加载桌宠设置…"}</div>}>
                <PetPanel
                  config={config}
                  windowPlacementBusy={windowPlacementBusy}
                  updateConfig={updateConfig}
                  persistConfig={persistGlobalDraft}
                  onLocate={() => locateWindow("bongo-cat")}
                />
              </Suspense>
            )}

            {activeTab === "shortcuts" && config && (
              <ShortcutsPanel
                config={config}
                accounts={shortcutAccounts}
                errors={shortcutErrors}
                checkingTarget={checkingShortcut}
                recordingPosition={recordingPos}
                setRecordingPosition={setRecordingPos}
                onKeyDown={handleShortcutKeyDown}
                onClear={handleClearShortcut}
                onSchemeChange={(slot, groupId) => updateConfig(c => {
                  const bindings = c.launch_scheme_shortcuts ?? [];
                  while (bindings.length <= slot) bindings.push({ group_id: "", shortcut: "" });
                  bindings[slot] = { group_id: groupId, shortcut: groupId ? bindings[slot].shortcut : "" };
                  c.launch_scheme_shortcuts = bindings;
                })}
              />
            )}

            {!minimalMode && activeTab === "tasks" && config && (
              <TaskRuntimePanel language={config.app_language} />
            )}

            {activeTab === "advanced" && config && (
              <MaintenancePanel
                featureProfile={normalizeFeatureProfile(config.feature_profile)}
                profileChanging={profileChanging}
                installedOptionalModuleCount={installedModules.length}
                onChangeFeatureProfile={handleChangeFeatureProfile}
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
      </Suspense>
      </SettingsShell>
      {pendingDisclosureModule && (
        <DisclosureDialog
          open={open}
          language={settingsLanguage}
          target={{ type: "module", module: pendingDisclosureModule }}
          accepting={disclosureInstallBusy}
          onCancel={dismissPendingDisclosure}
          onAccept={acceptAndInstallModule}
        />
      )}
    </>
  );
}
