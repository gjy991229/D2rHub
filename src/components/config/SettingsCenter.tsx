import { useState, useEffect, useRef } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { emitEvent, invokeCommand, listenEvent } from "../../platform/tauri";
import { useGlobalConfig } from "../../store/globalConfig";
import { useAccounts } from "../../store/accounts";
import { useTheme } from "../../store/theme";
import { showToast } from "../ui/Toast";
import { parseShortcutFromKeyEvent, useShortcutRecorder } from "../../hooks/useShortcutRecorder";
import type { SettingsMap } from "../../pages/SettingsEditor";
import type { AudioModSetupState, GlobalConfig } from "../../store/types";
import { validateTrackingTarget } from "../../utils/trackingTarget";
import { installationPathEditsAreInvalid } from "../../utils/installationPathChanges";
import { diffGlobalConfig } from "../../utils/globalConfigPatch";
import { validateAudioModName } from "../../utils/audioModName";
import { sortAccountsByCardOrder } from "../../utils/accountOrder";
import { FRAMERATE_CAP_KEY, writeFramerateCap } from "../../utils/gameSettings";
import {
  locateAuxiliaryWindow,
  recoverAuxiliaryWindows,
  type AuxiliaryWindowLabel,
} from "../../utils/windowPlacement";
import { PathsPanel } from "../../features/settings/panels/PathsPanel";
import { SettingsShell } from "../../features/settings/SettingsShell";
import { LaunchStrategyPanel } from "../../features/settings/panels/LaunchStrategyPanel";
import { ShortcutsPanel } from "../../features/settings/panels/ShortcutsPanel";
import { MaintenancePanel } from "../../features/settings/panels/MaintenancePanel";
import { PetPanel } from "../../features/settings/panels/PetPanel";
import { AccountsPanel } from "../../features/settings/panels/AccountsPanel";
import { AppearancePanel } from "../../features/settings/panels/AppearancePanel";
import { OverlayPanel } from "../../features/settings/panels/OverlayPanel";
import { AutomationPanel } from "../../features/settings/panels/AutomationPanel";
import {
  ModProcessingPanel,
  type ModProcessingPurpose,
} from "../../features/settings/panels/ModProcessingPanel";
import { RoomAutomationPanel } from "../../features/settings/panels/RoomAutomationPanel";
import { TaskRuntimePanel } from "../../features/tasks";
import {
  AUDIO_TELEMETRY_FEATURE_ID,
  audioModFeatureInvokeOptions,
  audioSetupDefaults,
  hasAudioTelemetry,
  hasSelectedAudioModFeature,
  IN_GAME_ROOM_TOOLS_FEATURE_ID,
  selectedAudioModFeatureAddsCapability,
  type AudioModPrepareProgress,
  type AudioModPrepareResult,
  type RuneAudioStatus,
} from "../../features/settings/audioModuleModel";
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

interface ExportAccountsSummary {
  path: string;
  account_count: number;
  plaintext_token_count: number;
}

interface ImportAccountsSummary {
  imported: { id: string; display_name: string; initialized: boolean }[];
  warnings: string[];
  reencrypted_token_count: number;
}

export function SettingsCenter({ open, onClose, onReconfigure, onInitializeAccount, initialTab, initialAccountId }: Props) {
  const { config, patch: patchConfig, detectSavedGamesPath, detectGlobalSavedGamesPath, detectProgramDataAgentPath, detectAppDataRoamingBnetPath, detectBrowserPath } = useGlobalConfig();
  const { accounts, loadAccounts, renameAccount, updateAccountMods } = useAccounts();
  const { theme, setTheme } = useTheme();
  const initializedTrackingAccounts = accounts.filter((account) => account.initialized);
  const shortcutAccounts = sortAccountsByCardOrder(accounts);
  const trackingTarget = validateTrackingTarget(config?.rune_audio_target_account ?? "", accounts);
  const trackingTargetId = trackingTarget.valid ? trackingTarget.account.id : "";

  // Tab and search state
  const [activeTab, setActiveTab] = useState<SettingsTabId>("accounts");
  const [roomAutomationDirty, setRoomAutomationDirty] = useState(false);
  const [settingsJsonAvailable, setSettingsJsonAvailable] = useState<Record<"CN" | "Global", boolean | null>>({ CN: null, Global: null });
  const [audioStatus, setAudioStatus] = useState<RuneAudioStatus | null>(null);
  const [audioModState, setAudioModState] = useState<AudioModSetupState | null>(null);
  const [audioModStateLoading, setAudioModStateLoading] = useState(false);
  const [audioSetupOpen, setAudioSetupOpen] = useState(false);
  const [audioSetupPurpose, setAudioSetupPurpose] = useState<ModProcessingPurpose>("manage");
  const [audioSetupMode, setAudioSetupMode] = useState<"original" | "existing">("original");
  const [audioSetupSource, setAudioSetupSource] = useState("");
  const [audioSetupName, setAudioSetupName] = useState("");
  const [includeAudioTelemetry, setIncludeAudioTelemetry] = useState(true);
  const [includeRoomTools, setIncludeRoomTools] = useState(true);
  const [audioPreparing, setAudioPreparing] = useState(false);
  const [audioPrepareProgress, setAudioPrepareProgress] = useState<AudioModPrepareProgress | null>(null);
  const [audioModScannedAt, setAudioModScannedAt] = useState<number | null>(null);
  const audioModStateCacheRef = useRef(new Map<string, { state: AudioModSetupState; scannedAt: number }>());
  const [windowPlacementBusy, setWindowPlacementBusy] = useState<string | null>(null);
  const normalizedAudioSetupName = audioSetupName.trim();
  const isAudioModUpgrade = !!audioModState?.current_mod_name && (
    audioModState.update_required
    || audioModState.ready
    || audioModState.feature_groups.length > 0
  );
  const isAudioModFeatureManagement = isAudioModUpgrade && !audioModState?.update_required;
  const installedAudioModNames = audioModState?.installed_mods.map((mod) => mod.name) ?? [];
  const audioSetupNameError = isAudioModUpgrade
    ? ""
    : validateAudioModName(audioSetupName, installedAudioModNames);
  const showAudioSetupNameError = !isAudioModUpgrade && audioSetupName.length > 0 && !!audioSetupNameError;
  const hasInitializedAudioAccount = initializedTrackingAccounts.length > 0;
  const hasAudioTarget = trackingTarget.valid;
  const hasReadyAudioMod = hasAudioTarget && !!audioModState?.ready;
  const isAudioEnableRequested = !!config?.rune_audio_enabled;
  const isAudioRecognitionActive = isAudioEnableRequested && hasReadyAudioMod;
  const installedAudioFeatureGroups = audioModState?.feature_groups ?? [];
  const selectedAudioSourceFeatureGroups = audioSetupMode === "existing"
    ? audioModState?.installed_mods.find((mod) => mod.name === audioSetupSource)?.feature_groups ?? []
    : [];
  const inheritedAudioFeatureGroups = isAudioModUpgrade
    ? installedAudioFeatureGroups
    : selectedAudioSourceFeatureGroups;
  const audioFeatureSelection = {
    includeAudioTelemetry: includeAudioTelemetry
      || audioSetupPurpose === "recognition"
      || inheritedAudioFeatureGroups.includes(AUDIO_TELEMETRY_FEATURE_ID),
    includeRoomTools: includeRoomTools
      || inheritedAudioFeatureGroups.includes(IN_GAME_ROOM_TOOLS_FEATURE_ID),
  };
  const audioPrepareBlockedReason = !hasSelectedAudioModFeature(audioFeatureSelection)
    ? config?.app_language === "en-US"
      ? "Select at least one Mod feature"
      : "请至少选择一个 Mod 功能"
    : isAudioModFeatureManagement
        && !selectedAudioModFeatureAddsCapability(
          audioFeatureSelection,
          installedAudioFeatureGroups,
        )
      ? config?.app_language === "en-US"
        ? "The current Mod already contains every selected feature"
        : "当前 Mod 已包含所选功能，请选择一个尚未安装的功能"
      : !isAudioModUpgrade && audioSetupNameError
        ? audioSetupNameError
        : !isAudioModFeatureManagement && audioSetupMode === "existing" && !audioSetupSource
          ? config?.app_language === "en-US"
            ? "Select the original Mod whose features should be preserved"
            : "请选择一个要保留功能的原始 Mod"
          : "";

  const locateWindow = async (label: AuxiliaryWindowLabel) => {
    const names = config?.app_language === "en-US"
      ? { overlay: "Terror Zone Broadcast", "stats-overlay": "Run Statistics", "bongo-cat": "Cat Overlay" }
      : { overlay: "邪恶区域播报窗口", "stats-overlay": "场景统计窗口", "bongo-cat": "猫咪悬浮窗" };
    const name = names[label];
    setWindowPlacementBusy(label);
    try {
      await locateAuxiliaryWindow(label);
      showToast(
        "success",
        config?.app_language === "en-US"
          ? `${name} was moved to this display`
          : `${name}已移到当前屏幕`,
      );
    } catch (error) {
      showToast(
        "error",
        config?.app_language === "en-US"
          ? `Failed to locate ${name}: ${error}`
          : `定位${name}失败: ${error}`,
      );
    } finally {
      setWindowPlacementBusy(null);
    }
  };

  const recoverAllWindows = async () => {
    setWindowPlacementBusy("all");
    try {
      const recovered = await recoverAuxiliaryWindows("main");
      if (recovered.length === 0) {
        showToast("info", "当前没有已启用的悬浮窗");
      } else {
        showToast(
          "success",
          config?.app_language === "en-US"
            ? `Moved ${recovered.length} overlay windows to this display`
            : `已将 ${recovered.length} 个悬浮窗移到当前屏幕`,
        );
      }
    } catch (error) {
      showToast(
        "error",
        config?.app_language === "en-US"
          ? `Failed to recover overlay windows: ${error}`
          : `找回悬浮窗失败: ${error}`,
      );
    } finally {
      setWindowPlacementBusy(null);
    }
  };

  // Config backup for rollback
  const [originalConfig, setOriginalConfig] = useState<GlobalConfig | null>(null);
  const autoSaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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

  useEffect(() => {
    if (!open || (activeTab !== "automation" && activeTab !== "mod-processing")) return;
    let cancelled = false;
    let timer: number | undefined;
    const poll = async () => {
      try {
        const next = await invokeCommand<RuneAudioStatus>("get_rune_audio_status");
        if (!cancelled) setAudioStatus(next);
      } catch (error) {
        console.warn("读取音频遥测状态失败", error);
      }
      if (!cancelled) timer = window.setTimeout(poll, 1000);
    };
    void poll();
    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [open, activeTab]);

  useEffect(() => {
    if (!open || (activeTab !== "automation" && activeTab !== "mod-processing") || !trackingTargetId) return;
    let cancelled = false;
    const cached = audioModStateCacheRef.current.get(trackingTargetId);
    if (cached) {
      setAudioModState(cached.state);
      setAudioModScannedAt(cached.scannedAt);
      setAudioModStateLoading(false);
      if (audioModState?.account_id === trackingTargetId) return;
      const defaults = audioSetupDefaults(cached.state);
      const availableSources = cached.state.installed_mods.filter((mod) => mod.source_eligible);
      setAudioSetupSource((current) => (
        current && availableSources.some((mod) => mod.name === current)
          ? current
          : defaults.source
      ));
      setAudioSetupMode(defaults.mode);
      setAudioSetupName(defaults.name);
      return;
    }
    setAudioModStateLoading(true);
    void invokeCommand<AudioModSetupState>("get_audio_mod_setup_state", { accountId: trackingTargetId })
      .then((next) => {
        const scannedAt = Date.now();
        audioModStateCacheRef.current.set(trackingTargetId, { state: next, scannedAt });
        if (cancelled) return;
        setAudioModState(next);
        setAudioModScannedAt(scannedAt);
        const defaults = audioSetupDefaults(next);
        const availableSources = next.installed_mods.filter((mod) => mod.source_eligible);
        setAudioSetupSource((current) => (
          current && availableSources.some((mod) => mod.name === current)
            ? current
            : defaults.source
        ));
        setAudioSetupMode(defaults.mode);
        setAudioSetupName(defaults.name);
        if (!next.ready && config?.rune_audio_enabled) {
          setIncludeAudioTelemetry(true);
          setIncludeRoomTools(true);
          setAudioSetupOpen(true);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setAudioModState(null);
          console.warn("读取识别 Mod 状态失败", error);
        }
      })
      .finally(() => {
        if (!cancelled) setAudioModStateLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, activeTab, trackingTargetId]);

  useEffect(() => {
    if (!open || (activeTab !== "automation" && activeTab !== "mod-processing")) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listenEvent<AudioModPrepareProgress>("audio-mod-prepare-progress", (event) => {
      if (!cancelled) setAudioPrepareProgress(event.payload);
    }).then((stopListening) => {
      if (cancelled) stopListening();
      else unlisten = stopListening;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [open, activeTab]);

  // Game settings edit states (per account)
  const [selectedAccountId, setSelectedAccountId] = useState<string>("");
  const [gameSettings, setGameSettings] = useState<SettingsMap>({});
  const [gameSettingsLoading, setGameSettingsLoading] = useState(false);
  const [gameSettingsLoadError, setGameSettingsLoadError] = useState<string | null>(null);
  const [gameSettingsChanged, setGameSettingsChanged] = useState(false);
  const [gameSettingsSaving, setGameSettingsSaving] = useState(false);
  const [_fontScaleKey, setFontScaleKey] = useState(0);
  const [gameSettingsTab, setGameSettingsTab] = useState<"launch" | "game_display" | "game_graphics" | "game_audio" | "game_gameplay" | "game_automap">("launch");

  // Account card values draft states
  const [accountNicknameDraft, setAccountNicknameDraft] = useState("");
  const [accountModArgsDraft, setAccountModArgsDraft] = useState("");
  const [accountWinXDraft, setAccountWinXDraft] = useState<number | null>(null);
  const [accountWinYDraft, setAccountWinYDraft] = useState<number | null>(null);

  // Shortcut key recording state (shared hook)
  const { recordingPos, setRecordingPos } = useShortcutRecorder();

  // Local detected paths
  const [detectedPaths, setDetectedPaths] = useState<Record<string, string | null>>({});
  const [exportPickerOpen, setExportPickerOpen] = useState(false);
  const [exportAccountIds, setExportAccountIds] = useState<string[]>([]);
  const [exportPlaintextRiskAcknowledged, setExportPlaintextRiskAcknowledged] = useState(false);
  const [accountTransferBusy, setAccountTransferBusy] = useState<"export" | "import" | null>(null);

  // Auto initialize selected account and active tab
  // Backup config for rollback when modal opens
  useEffect(() => {
    if (open && config) {
      setOriginalConfig(JSON.parse(JSON.stringify(config)));
    }
  }, [open]);

  useEffect(() => {
    if (!open) {
      setExportPickerOpen(false);
      setExportPlaintextRiskAcknowledged(false);
      setRoomAutomationDirty(false);
    }
  }, [open]);

  // Auto initialize selected account and active tab
  useEffect(() => {
    if (open) {
      if (isSettingsTabId(initialTab)) {
        setActiveTab(initialTab);
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
        setAccountModArgsDraft(acc.mod_args || "");
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
    if (roomAutomationDirty) {
      setActiveTab("room-automation");
      showToast(
        "warning",
        config?.app_language === "en-US"
          ? "Apply or discard the Room Automation changes before closing settings"
          : "请先应用或放弃自动跟房的更改，再关闭设置",
      );
      return;
    }
    if (config && installationPathEditsAreInvalid(originalConfig, config)) {
      setActiveTab("paths");
      showToast("error", "请至少保留一组国服或国际服的游戏安装目录；Battle.net 仅供国服兼容模式使用");
      return;
    }
    if (autoSaveTimerRef.current) {
      clearTimeout(autoSaveTimerRef.current);
      autoSaveTimerRef.current = null;
    }

    const closeAfterSave = async () => {
      if (config && globalHasChanges && !(await handleSaveGlobal(true))) {
        return;
      }
      if (accountHasChanges && !(await handleSaveAccount(true))) {
        return;
      }
      onClose();
    };

    void closeAfterSave();
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

  const handleSaveGlobal = async (quiet = false) => {
    if (!config) return true;
    return (await persistGlobalDraft(config, quiet)) !== null;
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

      // 2. Set Mod args if modified
      if (accountModArgsDraft !== acc.mod_args) {
        const mods = acc.mod_list || [];
        const val = accountModArgsDraft.trim();
        const newList = (val && !mods.includes(val)) ? [...mods, val] : mods;
        await updateAccountMods(selectedAccountId, val, newList);
      }

      // 3. Set Window position if modified
      if (accountWinXDraft !== acc.window_x || accountWinYDraft !== acc.window_y) {
        await invokeCommand("set_account_window_position", {
          accountId: selectedAccountId,
          windowX: accountWinXDraft,
          windowY: accountWinYDraft,
        });
      }

      // 4. Save game settings if modified
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

  const toggleAudioDiagnosticRecording = async () => {
    try {
      if (audioStatus?.diagnostic_recording) {
        const path = await invokeCommand<string | null>("stop_rune_audio_diagnostic_recording");
        setAudioStatus(previous => previous ? {
          ...previous,
          diagnostic_recording: false,
          diagnostic_recording_path: path ?? previous.diagnostic_recording_path,
        } : previous);
        if (path) showToast("success", `诊断录音已保存：${path}`);
      } else {
        const path = await invokeCommand<string>("start_rune_audio_diagnostic_recording");
        setAudioStatus(previous => previous ? {
          ...previous,
          diagnostic_recording: true,
          diagnostic_recording_path: path,
        } : previous);
        showToast("success", "诊断录音已开始，仅录制目标 D2R 进程的声音");
      }
    } catch (error) {
      showToast("error", `切换诊断录音失败: ${error}`);
    }
  };

  const persistAudioEnabledState = async (accountId: string, enabled: boolean) => {
    const current = useGlobalConfig.getState().config;
    if (!current) return;
    const next = {
      ...current,
      rune_audio_target_account: accountId,
      rune_audio_enabled: enabled,
    };
    useGlobalConfig.setState({ config: next });
    await persistGlobalDraft(next, true);
  };

  const refreshAudioModState = async () => {
    const accountId = trackingTargetId || initializedTrackingAccounts[0]?.id;
    if (!accountId) return;
    setAudioModStateLoading(true);
    try {
      const next = await invokeCommand<AudioModSetupState>("get_audio_mod_setup_state", { accountId });
      const scannedAt = Date.now();
      audioModStateCacheRef.current.set(accountId, { state: next, scannedAt });
      setAudioModState(next);
      setAudioModScannedAt(scannedAt);
      const defaults = audioSetupDefaults(next);
      setAudioSetupSource(defaults.source);
      setAudioSetupMode(defaults.mode);
      setAudioSetupName(defaults.name);
    } catch (error) {
      showToast("error", `重新扫描 Mod 失败: ${error}`);
    } finally {
      setAudioModStateLoading(false);
    }
  };

  const handleAudioTargetChange = async (accountId: string) => {
    const wasEnabled = !!useGlobalConfig.getState().config?.rune_audio_enabled;
    updateConfig(next => {
      next.rune_audio_target_account = accountId;
      next.rune_audio_enabled = false;
    });
    setAudioSetupOpen(false);
    setAudioSetupName("");
    setIncludeAudioTelemetry(true);
    setIncludeRoomTools(true);
    setAudioModStateLoading(true);
    try {
      const next = await invokeCommand<AudioModSetupState>("get_audio_mod_setup_state", { accountId });
      const scannedAt = Date.now();
      audioModStateCacheRef.current.set(accountId, { state: next, scannedAt });
      setAudioModState(next);
      setAudioModScannedAt(scannedAt);
      const defaults = audioSetupDefaults(next);
      setAudioSetupSource(defaults.source);
      setAudioSetupMode(defaults.mode);
      setAudioSetupName(defaults.name);
      if (wasEnabled && next.ready) {
        await persistAudioEnabledState(accountId, true);
      } else if (wasEnabled) {
        setIncludeAudioTelemetry(true);
        setIncludeRoomTools(true);
        setAudioSetupPurpose("recognition");
        setAudioSetupOpen(true);
        setActiveTab("mod-processing");
      }
    } catch (error) {
      showToast("error", `无法检查账号的识别 Mod：${error}`);
    } finally {
      setAudioModStateLoading(false);
    }
  };

  const handleAudioToggle = async (enabled: boolean) => {
    if (!enabled) {
      if (trackingTargetId) await persistAudioEnabledState(trackingTargetId, false);
      else updateConfig(next => { next.rune_audio_enabled = false; });
      setAudioSetupOpen(false);
      await invokeCommand("stop_rune_audio_monitor").catch(() => {});
      return;
    }

    const accountId = trackingTargetId || initializedTrackingAccounts[0]?.id;
    if (!accountId) {
      showToast("warning", "请先初始化一个账号");
      return;
    }
    setAudioModStateLoading(true);
    try {
      const next = await invokeCommand<AudioModSetupState>("get_audio_mod_setup_state", { accountId });
      const scannedAt = Date.now();
      audioModStateCacheRef.current.set(accountId, { state: next, scannedAt });
      setAudioModState(next);
      setAudioModScannedAt(scannedAt);
      if (next.ready) {
        await persistAudioEnabledState(accountId, true);
        setAudioSetupOpen(false);
        if (next.update_required) {
          showToast("warning", "旧版识别 Mod 仍可使用；建议更新以获得即时恐怖区域识别");
        }
        if (next.running_pid && next.active_session_ready === true) {
          await invokeCommand("start_rune_audio_monitor").catch(() => {});
        } else if (next.restart_required) {
          showToast("warning", "设置已生效，请重启该账号的游戏后开始识别");
        }
        return;
      }

      const defaults = audioSetupDefaults(next);
      setAudioSetupSource(defaults.source);
      setAudioSetupMode(defaults.mode);
      setAudioSetupName(defaults.name);
      updateConfig(current => {
        current.rune_audio_target_account = accountId;
        current.rune_audio_enabled = false;
      });
      setIncludeAudioTelemetry(true);
      setIncludeRoomTools(true);
      setAudioSetupPurpose("recognition");
      setAudioSetupOpen(true);
      setActiveTab("mod-processing");
    } catch (error) {
      showToast("error", `无法开启声纹识别：${error}`);
    } finally {
      setAudioModStateLoading(false);
    }
  };

  const handleOpenAudioSetup = (purpose: ModProcessingPurpose = "manage") => {
    if (audioModState) {
      const defaults = audioSetupDefaults(audioModState);
      setAudioSetupMode(defaults.mode);
      setAudioSetupSource(defaults.source);
      setAudioSetupName(defaults.name);
    }
    setIncludeAudioTelemetry(true);
    setIncludeRoomTools(true);
    setAudioSetupPurpose(purpose);
    setAudioSetupOpen(true);
  };

  const handleOpenModProcessing = (purpose: ModProcessingPurpose = "manage") => {
    handleOpenAudioSetup(purpose);
    setActiveTab("mod-processing");
  };

  const handlePrepareAudioMod = async () => {
    const accountId = trackingTargetId || initializedTrackingAccounts[0]?.id;
    if (!accountId) return;
    if (audioPrepareBlockedReason) {
      showToast("warning", audioPrepareBlockedReason);
      return;
    }

    const featureOptions = audioModFeatureInvokeOptions(audioFeatureSelection);

    setAudioPreparing(true);
    setAudioPrepareProgress({
      account_id: accountId,
      phase: "starting",
      percent: 1,
      message: "正在开始准备…",
    });
    try {
      if (isAudioModUpgrade) {
        const wasEnabled = !!useGlobalConfig.getState().config?.rune_audio_enabled;
        const next = await invokeCommand<AudioModSetupState>("upgrade_audio_mod", {
          accountId,
          sourceModName: isAudioModFeatureManagement
            ? null
            : audioSetupMode === "existing" ? audioSetupSource : null,
          ...featureOptions,
        });
        const scannedAt = Date.now();
        audioModStateCacheRef.current.set(accountId, { state: next, scannedAt });
        setAudioModState(next);
        setAudioModScannedAt(scannedAt);
        await persistAudioEnabledState(
          accountId,
          wasEnabled && hasAudioTelemetry(next.feature_groups),
        );
        setAudioSetupOpen(false);
        showToast(
          "success",
          config?.app_language === "en-US"
            ? `Mod “${next.current_mod_name ?? audioSetupName}” was updated in place; its name and launch arguments are unchanged`
            : `Mod“${next.current_mod_name ?? audioSetupName}”已原位更新，名称和启动参数均未改变`,
        );
        return;
      }
      const result = await invokeCommand<AudioModPrepareResult>("prepare_audio_mod", {
        accountId,
        modName: normalizedAudioSetupName,
        sourceModName: audioSetupMode === "existing" ? audioSetupSource : null,
        ...featureOptions,
      });
      const next = await invokeCommand<AudioModSetupState>("apply_audio_mod_to_account", {
        accountId,
        modName: result.mod_name,
      });
      const scannedAt = Date.now();
      audioModStateCacheRef.current.set(accountId, { state: next, scannedAt });
      await loadAccounts();
      setAudioModState(next);
      setAudioModScannedAt(scannedAt);
      const preparedAudioTelemetry = hasAudioTelemetry(result.feature_groups)
        && hasAudioTelemetry(next.feature_groups);
      await persistAudioEnabledState(
        accountId,
        preparedAudioTelemetry,
      );
      setAudioSetupOpen(false);
      setAudioSetupName("");
      if (next.restart_required) {
        showToast(
          "warning",
          config?.app_language === "en-US"
            ? preparedAudioTelemetry
              ? "The recognition Mod is ready. Restart this game session once to enable it"
              : "The selected Mod features are ready. Restart this game session once to enable them"
            : preparedAudioTelemetry
              ? "识别 Mod 已准备完成。当前游戏需重启一次，之后会自动识别"
              : "所选 Mod 功能已准备完成。当前游戏需重启一次后生效",
        );
      } else {
        showToast(
          "success",
          config?.app_language === "en-US"
            ? preparedAudioTelemetry
              ? "Audio recognition is ready and will start with the next game session"
              : "The selected Mod features are ready for the next game session"
            : preparedAudioTelemetry
              ? "声纹识别已准备完成，下次启动会自动生效"
              : "所选 Mod 功能已准备完成，下次启动游戏时生效",
        );
      }
    } catch (error) {
      showToast("error", `准备识别 Mod 失败：${error}`);
    } finally {
      setAudioPreparing(false);
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

  const toggleExportAccount = (accountId: string) => {
    setExportAccountIds(current => current.includes(accountId)
      ? current.filter(id => id !== accountId)
      : [...current, accountId]);
  };

  const handleExportAccounts = async () => {
    if (exportAccountIds.length === 0) {
      showToast("warning", "请至少选择一个要导出的账号");
      return;
    }
    if (!exportPlaintextRiskAcknowledged) {
      showToast("warning", "请先确认已理解明文 Token 的账号安全风险");
      return;
    }
    const date = new Date().toISOString().slice(0, 10);
    const destination = await saveDialog({
      title: "导出 D2RHub 账号",
      defaultPath: `D2RHub-accounts-${date}.json`,
      filters: [{ name: "D2RHub 账号包", extensions: ["json"] }],
    });
    if (!destination) return;

    setAccountTransferBusy("export");
    try {
      const summary = await invokeCommand<ExportAccountsSummary>("export_accounts", {
        accountIds: exportAccountIds,
        destination,
        acknowledgePlaintextRisk: true,
      });
      showToast("success", `已导出 ${summary.account_count} 个账号`);
      showToast(
        "warning",
        summary.plaintext_token_count > 0
          ? `导出文件包含 ${summary.plaintext_token_count} 个明文 Token；任何获得文件的人都可以登录对应账号，请妥善保管并在迁移后删除`
          : "导出文件仍包含账号认证快照，请妥善保管并在迁移后删除",
      );
      setExportPickerOpen(false);
      setExportPlaintextRiskAcknowledged(false);
    } catch (error) {
      showToast("error", `导出账号失败: ${error}`);
    } finally {
      setAccountTransferBusy(null);
    }
  };

  const handleImportAccounts = async () => {
    const source = await openDialog({
      title: "选择 D2RHub 账号导出文件",
      multiple: false,
      filters: [{ name: "D2RHub 账号包", extensions: ["json"] }],
    });
    if (!source || Array.isArray(source)) return;

    setAccountTransferBusy("import");
    try {
      const summary = await invokeCommand<ImportAccountsSummary>("import_accounts", { source });
      await loadAccounts();
      showToast(
        "success",
        `已导入 ${summary.imported.length} 个账号，本机重新加密 ${summary.reencrypted_token_count} 个 Token`,
      );
      if (summary.warnings.length > 0) {
        const extra = summary.warnings.length > 1 ? `（另有 ${summary.warnings.length - 1} 项提示）` : "";
        showToast("warning", `${summary.warnings[0]}${extra}`);
      }
    } catch (error) {
      showToast("error", `导入账号失败: ${error}`);
    } finally {
      setAccountTransferBusy(null);
    }
  };

  const handleOpenLogs = async () => {
    try {
      await invokeCommand("open_logs_dir");
    } catch (error) {
      showToast("error", `打开日志失败: ${error}`);
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
      accountModArgsDraft !== (acc.mod_args || "") ||
      accountWinXDraft !== (acc.window_x ?? null) ||
      accountWinYDraft !== (acc.window_y ?? null) ||
      gameSettingsChanged
    );
  })();

  const hasUnsavedChanges = globalHasChanges || accountHasChanges;
  const hasAnyUnsavedChanges = hasUnsavedChanges || roomAutomationDirty;

  const autoSaveKey = JSON.stringify({
    selectedAccountId,
    accountNicknameDraft,
    accountModArgsDraft,
    accountWinXDraft,
    accountWinYDraft,
    gameSettingsChanged,
    gameSettings,
    config,
  });

  useEffect(() => {
    if (!open || !hasUnsavedChanges || (config && installationPathEditsAreInvalid(originalConfig, config))) return;

    if (autoSaveTimerRef.current) {
      clearTimeout(autoSaveTimerRef.current);
    }

    autoSaveTimerRef.current = setTimeout(() => {
      autoSaveTimerRef.current = null;
      (async () => {
        if (config && globalHasChanges) {
          await handleSaveGlobal(true);
        }
        if (accountHasChanges) {
          await handleSaveAccount(true);
        }
      })().catch(e => showToast("error", `自动保存失败: ${e}`));
    }, 800);

    return () => {
      if (autoSaveTimerRef.current) {
        clearTimeout(autoSaveTimerRef.current);
      }
    };
  }, [open, hasUnsavedChanges, autoSaveKey]);

  const selectedAccount = accounts.find(a => a.id === selectedAccountId);
  const accountRegionLabel = (region?: string | null) =>
    region === "KR" ? "亚服" : region === "NA" ? "美服" : region === "EU" ? "欧服" : region === "Global" ? "国际服" : "国服";
  const saveStatusText = gameSettingsSaving
    ? "自动保存中"
    : hasAnyUnsavedChanges
      ? "有未保存改动"
      : "已自动保存";

  const handleTabChange = (nextTab: SettingsTabId) => {
    if (roomAutomationDirty && nextTab !== "room-automation") {
      showToast(
        "warning",
        config?.app_language === "en-US"
          ? "Apply or discard the Room Automation changes before leaving this section"
          : "请先应用或放弃自动跟房的更改，再离开此页面",
      );
      return false;
    }
    if (nextTab === "mod-processing" && activeTab !== "mod-processing") {
      handleOpenAudioSetup("manage");
    }
    setActiveTab(nextTab);
    return true;
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
                setSelectedAccountId={setSelectedAccountId}
                accountHasChanges={!!accountHasChanges}
                saveAccount={handleSaveAccount}
                toggleCustomizedSettings={handleToggleAccountSettingsMode}
                accountRegionLabel={accountRegionLabel}
                gameSettingsTab={gameSettingsTab}
                setGameSettingsTab={setGameSettingsTab}
                snapshotSystemSettings={handleSnapshotSystemSettings}
                accountNicknameDraft={accountNicknameDraft}
                setAccountNicknameDraft={setAccountNicknameDraft}
                accountModArgsDraft={accountModArgsDraft}
                setAccountModArgsDraft={setAccountModArgsDraft}
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
                config={config}
                theme={theme}
                setTheme={setTheme}
                updateConfig={updateConfig}
                persistConfig={persistGlobalDraft}
                setFontScaleKey={setFontScaleKey}
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
                normalizedAudioSetupName={normalizedAudioSetupName}
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
                onClose={onClose}
                onInitializeAccount={onInitializeAccount}
              />
            )}

            {activeTab === "mod-processing" && config && (
              <ModProcessingPanel
                config={config}
                initializedAccounts={initializedTrackingAccounts}
                trackingTarget={trackingTarget}
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
                normalizedAudioSetupName={normalizedAudioSetupName}
                isAudioModUpgrade={isAudioModUpgrade}
                isAudioModFeatureManagement={isAudioModFeatureManagement}
                audioSetupNameError={audioSetupNameError}
                showAudioSetupNameError={showAudioSetupNameError}
                audioPrepareBlockedReason={audioPrepareBlockedReason}
                onTargetChange={handleAudioTargetChange}
                onPrepare={handlePrepareAudioMod}
                onRefresh={refreshAudioModState}
                onBackToRecognition={() => setActiveTab("automation")}
              />
            )}

            {/* Optional keyboard-only room creation and follower joining module. */}
            {activeTab === "room-automation" && (
              <RoomAutomationPanel
                accounts={accounts}
                language={config?.app_language}
                onDirtyChange={setRoomAutomationDirty}
                onOpenAudioModSettings={() => { handleOpenModProcessing("manage"); }}
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
                onRunSetup={() => {
                  onClose();
                  onReconfigure();
                }}
              />
            )}
    </SettingsShell>
  );
}
