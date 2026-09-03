import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { showToast } from "../../components/ui/Toast";
import { invokeCommand, listenEvent } from "../../platform/tauri";
import { useGlobalConfig } from "../../store/globalConfig";
import type { AccountMeta, AudioModSetupState, GlobalConfig } from "../../store/types";
import { validateAudioModName } from "../../utils/audioModName";
import {
  AUTO_EXIT_ON_DEATH_FEATURE_ID,
  AUDIO_TELEMETRY_FEATURE_ID,
  audioModFeatureDefaultsForPurpose,
  audioModFeatureInvokeOptions,
  audioSetupDefaults,
  hasAudioTelemetry,
  hasSelectedAudioModFeature,
  IN_GAME_ROOM_TOOLS_FEATURE_ID,
  selectedAudioModFeatureAddsCapability,
  type AudioModProcessingMode,
  type AudioModPrepareProgress,
  type AudioModPrepareResult,
  type RuneAudioStatus,
} from "./audioModuleModel";
import type { ModProcessingPurpose } from "./panels/ModProcessingPanel";
import type { SettingsTabId } from "./settingsRegistry";

type AudioSetupMode = "original" | "existing";

interface AudioModuleControllerOptions {
  open: boolean;
  activeTab: SettingsTabId;
  config: GlobalConfig | null;
  initializedAccounts: AccountMeta[];
  trackingTargetId: string;
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
  persistConfig: (draft: GlobalConfig, quiet?: boolean) => Promise<unknown>;
  loadAccounts: () => Promise<void>;
  setActiveTab: Dispatch<SetStateAction<SettingsTabId>>;
}

export function useAudioModuleController({
  open,
  activeTab,
  config,
  initializedAccounts,
  trackingTargetId,
  updateConfig,
  persistConfig,
  loadAccounts,
  setActiveTab,
}: AudioModuleControllerOptions) {
  const [audioStatus, setAudioStatus] = useState<RuneAudioStatus | null>(null);
  const [audioModState, setAudioModState] = useState<AudioModSetupState | null>(null);
  const [audioModStateLoading, setAudioModStateLoading] = useState(false);
  const [audioSetupOpen, setAudioSetupOpen] = useState(false);
  const [audioSetupPurpose, setAudioSetupPurpose] = useState<ModProcessingPurpose>("manage");
  const [modProcessingTargetId, setModProcessingTargetId] = useState(trackingTargetId);
  const [audioSetupMode, setAudioSetupMode] = useState<AudioSetupMode>("original");
  const [audioSetupSource, setAudioSetupSource] = useState("");
  const [audioSetupName, setAudioSetupName] = useState("");
  const [audioProcessingMode, setAudioProcessingMode] = useState<AudioModProcessingMode>("create");
  const [audioProcessingTarget, setAudioProcessingTarget] = useState("");
  const [includeAudioTelemetry, setIncludeAudioTelemetry] = useState(false);
  const [includeRoomTools, setIncludeRoomTools] = useState(false);
  const [includeAutoExitOnDeath, setIncludeAutoExitOnDeath] = useState(false);
  const [audioPreparing, setAudioPreparing] = useState(false);
  const [audioPrepareProgress, setAudioPrepareProgress] = useState<AudioModPrepareProgress | null>(null);
  const [audioModScannedAt, setAudioModScannedAt] = useState<number | null>(null);
  const [autoPrepareRequest, setAutoPrepareRequest] = useState(0);
  const cacheRef = useRef(new Map<string, { state: AudioModSetupState; scannedAt: number }>());
  const setupAccountId = activeTab === "mod-processing"
    ? modProcessingTargetId || trackingTargetId
    : trackingTargetId;

  const normalizedAudioSetupName = audioSetupName.trim();
  const selectedProcessingTarget = audioModState?.installed_mods.find((mod) => (
    mod.name.toLocaleLowerCase() === audioProcessingTarget.toLocaleLowerCase()
  ));
  const hasSelectedProcessedMod = !!selectedProcessingTarget && (
    selectedProcessingTarget.update_required || selectedProcessingTarget.feature_groups.length > 0
  );
  const isAudioModUpgrade = audioProcessingMode === "augment" && hasSelectedProcessedMod;
  const isAudioModFeatureManagement = isAudioModUpgrade
    && selectedProcessingTarget?.update_required === false;
  const installedAudioModNames = audioModState?.installed_mods.map((mod) => mod.name) ?? [];
  const audioSetupNameError = audioProcessingMode === "augment"
    ? ""
    : validateAudioModName(audioSetupName, installedAudioModNames);
  const showAudioSetupNameError = audioProcessingMode === "create"
    && audioSetupName.length > 0
    && !!audioSetupNameError;
  const hasInitializedAudioAccount = initializedAccounts.length > 0;
  const hasAudioTarget = !!trackingTargetId;
  const hasReadyAudioMod = hasAudioTarget && !!audioModState?.ready;
  const isAudioEnableRequested = !!config?.rune_audio_enabled;
  const isAudioRecognitionActive = isAudioEnableRequested && hasReadyAudioMod;
  const installedAudioFeatureGroups = selectedProcessingTarget?.feature_groups ?? [];
  const selectedAudioSource = audioSetupMode === "existing"
    ? audioModState?.installed_mods.find((mod) => mod.name === audioSetupSource)
    : undefined;
  const selectedAudioSourceFeatureGroups = selectedAudioSource?.feature_groups ?? [];
  const inheritedAudioFeatureGroups = isAudioModUpgrade
    ? installedAudioFeatureGroups
    : selectedAudioSourceFeatureGroups;
  const audioFeatureSelection = {
    includeAudioTelemetry: includeAudioTelemetry
      || audioSetupPurpose === "recognition"
      || inheritedAudioFeatureGroups.includes(AUDIO_TELEMETRY_FEATURE_ID),
    includeRoomTools: includeRoomTools
      || audioSetupPurpose === "room-tools"
      || inheritedAudioFeatureGroups.includes(IN_GAME_ROOM_TOOLS_FEATURE_ID),
    includeAutoExitOnDeath: includeAutoExitOnDeath
      || inheritedAudioFeatureGroups.includes(AUTO_EXIT_ON_DEATH_FEATURE_ID),
  };
  const audioPrepareBlockedReason = !hasSelectedAudioModFeature(audioFeatureSelection)
    ? config?.app_language === "en-US" ? "Select at least one Mod feature" : "请至少选择一个 Mod 功能"
    : isAudioModFeatureManagement
        && !selectedAudioModFeatureAddsCapability(
          audioFeatureSelection,
          installedAudioFeatureGroups,
        )
      ? config?.app_language === "en-US"
        ? "The current Mod already contains every selected feature"
        : "当前 Mod 已包含所选功能，请选择一个尚未安装的功能"
      : audioProcessingMode === "augment" && !hasSelectedProcessedMod
        ? config?.app_language === "en-US"
          ? "The selected processed Mod is no longer available; rescan and choose again"
          : "所选已加工 Mod 已不可用，请重新扫描后再选择"
      : audioProcessingMode === "create" && audioSetupNameError
        ? audioSetupNameError
        : audioProcessingMode === "create" && audioSetupMode === "existing" && !audioSetupSource
          ? config?.app_language === "en-US"
            ? "Select the original Mod whose features should be preserved"
            : "请选择一个要保留功能的原始 Mod"
          : "";

  const applySetupDefaults = (next: AudioModSetupState) => {
    const defaults = audioSetupDefaults(next);
    const currentInstalled = next.installed_mods.find((mod) => (
      !!next.current_mod_name && mod.name.toLocaleLowerCase() === next.current_mod_name.toLocaleLowerCase()
    ));
    const currentIsProcessed = !!currentInstalled && (
      currentInstalled.update_required || currentInstalled.feature_groups.length > 0
    );
    setAudioSetupSource(defaults.source);
    setAudioSetupMode(defaults.mode);
    setAudioSetupName(defaults.name);
    setAudioProcessingMode(currentIsProcessed ? "augment" : "create");
    setAudioProcessingTarget(currentIsProcessed ? currentInstalled.name : "");
  };

  const applyFeatureDefaults = (purpose: ModProcessingPurpose) => {
    const defaults = audioModFeatureDefaultsForPurpose(purpose);
    setIncludeAudioTelemetry(defaults.includeAudioTelemetry);
    setIncludeRoomTools(defaults.includeRoomTools);
    setIncludeAutoExitOnDeath(defaults.includeAutoExitOnDeath);
  };

  const cacheState = (accountId: string, next: AudioModSetupState) => {
    const scannedAt = Date.now();
    cacheRef.current.set(accountId, { state: next, scannedAt });
    setAudioModState(next);
    setAudioModScannedAt(scannedAt);
    return next;
  };

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
    if (!open || (activeTab !== "automation" && activeTab !== "mod-processing") || !setupAccountId) return;
    let cancelled = false;
    const cached = cacheRef.current.get(setupAccountId);
    if (cached) {
      setAudioModState(cached.state);
      setAudioModScannedAt(cached.scannedAt);
      setAudioModStateLoading(false);
      if (audioModState?.account_id === setupAccountId) return;
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
    void invokeCommand<AudioModSetupState>("get_audio_mod_setup_state", { accountId: setupAccountId })
      .then((next) => {
        if (cancelled) return;
        cacheState(setupAccountId, next);
        applySetupDefaults(next);
        if (!next.ready && config?.rune_audio_enabled) {
          applyFeatureDefaults("recognition");
          setAudioSetupPurpose("recognition");
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
    return () => { cancelled = true; };
  }, [open, activeTab, setupAccountId]);

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

  const persistAudioEnabledState = async (accountId: string, enabled: boolean) => {
    const current = useGlobalConfig.getState().config;
    if (!current) return;
    const next = { ...current, rune_audio_target_account: accountId, rune_audio_enabled: enabled };
    useGlobalConfig.setState({ config: next });
    await persistConfig(next, true);
  };

  const refreshAudioModState = async () => {
    const accountId = setupAccountId || initializedAccounts[0]?.id;
    if (!accountId) return;
    setAudioModStateLoading(true);
    try {
      const next = await invokeCommand<AudioModSetupState>("get_audio_mod_setup_state", { accountId });
      cacheState(accountId, next);
      applySetupDefaults(next);
    } catch (error) {
      showToast("error", `重新扫描 Mod 失败: ${error}`);
    } finally {
      setAudioModStateLoading(false);
    }
  };

  const handleAudioTargetChange = async (accountId: string) => {
    setModProcessingTargetId(accountId);
    const wasEnabled = !!useGlobalConfig.getState().config?.rune_audio_enabled;
    updateConfig((next) => {
      next.rune_audio_target_account = accountId;
      next.rune_audio_enabled = false;
    });
    setAudioSetupOpen(false);
    setAudioSetupName("");
    setAudioModStateLoading(true);
    try {
      const next = await invokeCommand<AudioModSetupState>("get_audio_mod_setup_state", { accountId });
      cacheState(accountId, next);
      applySetupDefaults(next);
      if (wasEnabled && next.ready) {
        await persistAudioEnabledState(accountId, true);
      } else if (wasEnabled) {
        applyFeatureDefaults("recognition");
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

  const handleModProcessingTargetChange = async (accountId: string) => {
    setModProcessingTargetId(accountId);
    setAudioSetupOpen(true);
    setAudioSetupName("");
    setAudioModStateLoading(true);
    try {
      cacheRef.current.delete(accountId);
      const next = await invokeCommand<AudioModSetupState>("get_audio_mod_setup_state", { accountId });
      cacheState(accountId, next);
      applySetupDefaults(next);
    } catch (error) {
      showToast("error", `无法检查账号的加工 Mod：${error}`);
    } finally {
      setAudioModStateLoading(false);
    }
  };

  const handleAudioToggle = async (enabled: boolean, preferredAccountId?: string) => {
    if (!enabled) {
      if (trackingTargetId) await persistAudioEnabledState(trackingTargetId, false);
      else updateConfig((next) => { next.rune_audio_enabled = false; });
      setAudioSetupOpen(false);
      await invokeCommand("stop_rune_audio_monitor").catch(() => undefined);
      return;
    }
    const accountId = preferredAccountId || trackingTargetId || initializedAccounts[0]?.id;
    if (!accountId) {
      showToast("warning", "请先初始化一个账号");
      return;
    }
    setAudioModStateLoading(true);
    try {
      const cached = cacheRef.current.get(accountId);
      const next = cached?.state
        ?? await invokeCommand<AudioModSetupState>("get_audio_mod_setup_state", { accountId });
      if (!cached) cacheState(accountId, next);
      if (next.ready) {
        await persistAudioEnabledState(accountId, true);
        setAudioSetupOpen(false);
        if (next.update_required) showToast("warning", "旧版识别 Mod 仍可使用；建议更新以获得即时恐怖区域识别");
        if (next.running_pid && next.active_session_ready === true) {
          await invokeCommand("start_rune_audio_monitor").catch(() => undefined);
        } else if (next.restart_required) {
          showToast("warning", "设置已生效，请重启该账号的游戏后开始识别");
        }
        return;
      }
      applySetupDefaults(next);
      updateConfig((current) => {
        current.rune_audio_target_account = accountId;
        current.rune_audio_enabled = false;
      });
      applyFeatureDefaults("recognition");
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
    if (audioModState) applySetupDefaults(audioModState);
    applyFeatureDefaults(purpose);
    setAudioSetupPurpose(purpose);
    setAudioSetupOpen(true);
  };

  const handleOpenModProcessing = (purpose: ModProcessingPurpose = "manage", accountId?: string) => {
    setModProcessingTargetId(accountId || trackingTargetId || initializedAccounts[0]?.id || "");
    handleOpenAudioSetup(purpose);
    setActiveTab("mod-processing");
  };

  const handlePrepareSelectedMod = (
    accountId: string,
    purpose: Exclude<ModProcessingPurpose, "manage">,
    autoStart = false,
    sourceModName?: string,
    processingMode: AudioModProcessingMode = "create",
  ) => {
    cacheRef.current.delete(accountId);
    setModProcessingTargetId(accountId);
    setAudioSetupPurpose(purpose);
    applyFeatureDefaults(purpose);
    setAudioProcessingMode(processingMode);
    setAudioProcessingTarget(processingMode === "augment" ? sourceModName ?? "" : "");
    if (sourceModName) {
      setAudioSetupMode("existing");
      setAudioSetupSource(sourceModName);
    }
    setAudioSetupOpen(true);
    if (autoStart) setAutoPrepareRequest((request) => request + 1);
    setActiveTab("mod-processing");
  };

  const handlePrepareAudioMod = async () => {
    const accountId = setupAccountId || initializedAccounts[0]?.id;
    if (!accountId) return;
    if (audioPrepareBlockedReason) {
      showToast("warning", audioPrepareBlockedReason);
      return;
    }
    const featureOptions = audioModFeatureInvokeOptions(audioFeatureSelection);
    setAudioPreparing(true);
    setAudioPrepareProgress({ account_id: accountId, phase: "starting", percent: 1, message: "正在开始准备…" });
    try {
      if (isAudioModUpgrade) {
        const currentAudioConfig = useGlobalConfig.getState().config;
        const wasEnabled = !!currentAudioConfig?.rune_audio_enabled;
        const targetIsConfigured = audioModState?.current_mod_name?.toLocaleLowerCase()
          === audioProcessingTarget.toLocaleLowerCase();
        const recordedSourceModName = selectedProcessingTarget?.source_mod_name
          ?? (targetIsConfigured ? audioModState?.source_mod_name : null);
        const selectedSourceModName = audioSetupMode === "existing"
          ? audioSetupSource.trim() || null
          : null;
        const upgraded = await invokeCommand<AudioModSetupState>("upgrade_audio_mod", {
          accountId,
          modName: audioProcessingTarget,
          sourceModName: recordedSourceModName ?? selectedSourceModName,
          ...featureOptions,
        });
        const targetWasAlreadyApplied = audioModState?.current_mod_name?.toLocaleLowerCase()
          === audioProcessingTarget.toLocaleLowerCase();
        const next = targetWasAlreadyApplied
          ? upgraded
          : await invokeCommand<AudioModSetupState>("apply_audio_mod_to_account", {
              accountId,
              modName: audioProcessingTarget,
            });
        cacheState(accountId, next);
        if (!targetWasAlreadyApplied) await loadAccounts();
        if (audioSetupPurpose === "recognition" || currentAudioConfig?.rune_audio_target_account === accountId) {
          await persistAudioEnabledState(accountId, wasEnabled && hasAudioTelemetry(next.feature_groups));
        }
        setAudioSetupOpen(false);
        if (audioSetupPurpose === "room-tools") setActiveTab("room-automation");
        showToast(
          "success",
          config?.app_language === "en-US"
            ? `Mod “${audioProcessingTarget}” was augmented and applied to the account`
            : `Mod“${audioProcessingTarget}”已完成增补并应用到账号`,
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
      cacheState(accountId, next);
      await loadAccounts();
      const preparedAudioTelemetry = hasAudioTelemetry(result.feature_groups)
        && hasAudioTelemetry(next.feature_groups);
      const currentAudioConfig = useGlobalConfig.getState().config;
      if (audioSetupPurpose === "recognition") {
        await persistAudioEnabledState(accountId, preparedAudioTelemetry);
      } else if (currentAudioConfig?.rune_audio_target_account === accountId) {
        await persistAudioEnabledState(accountId, !!currentAudioConfig.rune_audio_enabled && preparedAudioTelemetry);
      }
      setAudioSetupOpen(false);
      setAudioSetupName("");
      if (audioSetupPurpose === "room-tools") setActiveTab("room-automation");
      showToast(
        next.restart_required ? "warning" : "success",
        config?.app_language === "en-US"
          ? next.restart_required
            ? "The selected Mod features are ready. Restart this game session once to enable them"
            : "The selected Mod features are ready for the next game session"
          : next.restart_required
            ? "所选 Mod 功能已准备完成。当前游戏需重启一次后生效"
            : "所选 Mod 功能已准备完成，下次启动游戏时生效",
      );
    } catch (error) {
      showToast("error", `准备识别 Mod 失败：${error}`);
    } finally {
      setAudioPreparing(false);
    }
  };

  const toggleAudioDiagnosticRecording = async () => {
    try {
      if (audioStatus?.diagnostic_recording) {
        const path = await invokeCommand<string | null>("stop_rune_audio_diagnostic_recording");
        setAudioStatus((previous) => previous ? {
          ...previous,
          diagnostic_recording: false,
          diagnostic_recording_path: path ?? previous.diagnostic_recording_path,
        } : previous);
        if (path) showToast("success", `诊断录音已保存：${path}`);
      } else {
        const path = await invokeCommand<string>("start_rune_audio_diagnostic_recording");
        setAudioStatus((previous) => previous ? {
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

  return {
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
    consumeAutoPrepareRequest: () => setAutoPrepareRequest(0),
    refreshAudioModState,
    handleAudioTargetChange,
    handleModProcessingTargetChange,
    handleAudioToggle,
    handleOpenAudioSetup,
    handleOpenModProcessing,
    handlePrepareSelectedMod,
    handlePrepareAudioMod,
    toggleAudioDiagnosticRecording,
  };
}
