import { useEffect, useRef, useState } from "react";
import { showToast } from "../../components/ui/Toast";
import { invokeCommand } from "../../platform/tauri";
import type { AccountMeta, GlobalConfig } from "../../store/types";
import { useModInspection } from "../mods/workflow/useModInspection";
import type { ModAppliedResult, ModProcessingRequest } from "../mods/workflow/types";
import { defaultExternalAudioTarget, installationTargetId, matchesRecognitionSource, type ExternalAudioTarget } from "../../utils/recognitionSource";
import type { ExternalAudioInstance, RuneAudioStatus } from "./audioModuleModel";
import { hasAudioTelemetry } from "../mods/featureContract";
import type { SettingsTabId } from "./settingsRegistry";

interface AudioModuleControllerOptions {
  open: boolean;
  activeTab: SettingsTabId;
  config: GlobalConfig | null;
  initializedAccounts: AccountMeta[];
  trackingTargetId: string;
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
  persistConfig: (draft: GlobalConfig, quiet?: boolean) => Promise<unknown>;
  requestProcessing: (request: ModProcessingRequest) => void;
  optionalFeaturesAvailable?: boolean;
}

/** Recognition runtime and preferences only. Mod recipes belong to the Mod workflow. */
export function useAudioModuleController(options: AudioModuleControllerOptions) {
  const { open, activeTab, config, initializedAccounts, trackingTargetId, optionalFeaturesAvailable = true } = options;
  const latest = useRef(options);
  latest.current = options;
  const active = open && activeTab === "automation" && optionalFeaturesAvailable;
  const external = config?.rune_audio_external_target;
  const target = initializedAccounts.find(account => account.id === trackingTargetId);
  const inspectionIdentity = JSON.stringify([external, config?.cn_game_path, config?.global_game_path, target?.mod_args, target?.is_running, target?.running_pid]);
  const inspection = useModInspection(active, external ? installationTargetId(external.edition) : trackingTargetId, inspectionIdentity,
    external ? { edition: external.edition, modName: external.mod_name } : undefined);
  const [audioStatus, setAudioStatus] = useState<RuneAudioStatus | null>(null);
  const [changing, setChanging] = useState(false);
  const [externalInstances, setExternalInstances] = useState<ExternalAudioInstance[]>([]);
  const [externalInstancesError, setExternalInstancesError] = useState<string | null>(null);
  const requestRevision = useRef(0);
  const hasReadyAudioMod = !!inspection.state?.ready && (external ? !!external.mod_name && inspection.state.account_id === "" : !!trackingTargetId && inspection.state.account_id === trackingTargetId);

  useEffect(() => {
    if (active && config && !config.rune_audio_external_target && !config.rune_audio_target_account && initializedAccounts.length === 0) {
      latest.current.updateConfig(next => { next.rune_audio_external_target = defaultExternalAudioTarget(next); });
    }
  }, [active, config?.rune_audio_external_target, config?.rune_audio_target_account, initializedAccounts.length]);

  useEffect(() => {
    if (!active) return;
    let disposed = false;
    let timer: number | undefined;
    const poll = async () => {
      const revision = requestRevision.current;
      try {
        const status = await invokeCommand<RuneAudioStatus>("get_rune_audio_status");
        if (!disposed && revision === requestRevision.current) setAudioStatus(status);
      } catch (error) { console.warn("读取音频遥测状态失败", error); }
      if (!disposed) timer = window.setTimeout(poll, 1000);
    };
    void poll();
    return () => { disposed = true; if (timer !== undefined) window.clearTimeout(timer); };
  }, [active]);

  useEffect(() => {
    if (!active || !external) { setExternalInstances([]); setExternalInstancesError(null); return; }
    let disposed = false;
    let timer: number | undefined;
    const poll = async () => {
      const revision = requestRevision.current;
      try {
        const instances = await invokeCommand<ExternalAudioInstance[]>("get_external_audio_instances");
        if (!disposed && revision === requestRevision.current) { setExternalInstances(instances); setExternalInstancesError(null); }
      } catch (error) { if (!disposed && revision === requestRevision.current) { setExternalInstances([]); setExternalInstancesError(String(error)); } }
      if (!disposed) timer = window.setTimeout(poll, 2500);
    };
    void poll();
    return () => { disposed = true; if (timer !== undefined) window.clearTimeout(timer); };
  }, [active, external?.edition, external?.mod_name]);

  const persistExternalState = async (target: ExternalAudioTarget, enabled: boolean) => {
    const current = latest.current.config;
    if (!current) throw new Error("识别设置尚未加载");
    const next = { ...current, rune_audio_external_target: target, rune_audio_enabled: enabled };
    latest.current.updateConfig(draft => { draft.rune_audio_external_target = target; draft.rune_audio_enabled = enabled; });
    if (!(await latest.current.persistConfig(next, true))) throw new Error("识别设置未能保存，请重试");
  };
  const handleExternalTargetChange = async (target: ExternalAudioTarget) => {
    ++requestRevision.current;
    setChanging(true);
    try {
      await persistExternalState(target, false);
      await invokeCommand("stop_rune_audio_monitor");
    } catch (error) { showToast("error", `无法切换游戏监听目标：${error}`); }
    finally { setChanging(false); }
  };
  const selectExternalInstance = async (instance: ExternalAudioInstance) => {
    const revision = ++requestRevision.current;
    setChanging(true);
    try {
      await invokeCommand("select_external_audio_instance", { identity: { pid: instance.pid, started_at: instance.started_at } });
      if (revision !== requestRevision.current) return;
      setExternalInstances(previous => previous.map(candidate => ({ ...candidate, selected: candidate.pid === instance.pid && candidate.started_at === instance.started_at })));
      const status = await invokeCommand<RuneAudioStatus>("get_rune_audio_status").catch(() => null);
      if (revision === requestRevision.current && status) setAudioStatus(status);
    }
    catch (error) { showToast("error", `无法连接游戏实例：${error}`); }
    finally { if (revision === requestRevision.current) setChanging(false); }
  };

  const persistAudioEnabledState = async (accountId: string, enabled: boolean) => {
    const current = latest.current.config;
    if (!current) throw new Error("识别设置尚未加载");
    const next = { ...current, rune_audio_target_account: accountId, rune_audio_external_target: null, rune_audio_enabled: enabled };
    latest.current.updateConfig(draft => { draft.rune_audio_target_account = accountId; draft.rune_audio_external_target = null; draft.rune_audio_enabled = enabled; });
    if (!(await latest.current.persistConfig(next, true))) throw new Error("识别设置未能保存，请重试");
  };

  const handleAudioTargetChange = async (accountId: string) => {
    const revision = ++requestRevision.current;
    const wasEnabled = !!latest.current.config?.rune_audio_enabled;
    setChanging(true);
    try {
      await persistAudioEnabledState(accountId, wasEnabled);
      if (revision !== requestRevision.current) return;
    } catch (error) {
      if (revision === requestRevision.current) showToast("error", `无法检查账号的识别 Mod：${error}`);
    } finally { if (revision === requestRevision.current) setChanging(false); }
  };

  const handleAudioToggle = async (enabled: boolean, preferredAccountId?: string) => {
    const revision = ++requestRevision.current;
    const external = !preferredAccountId ? latest.current.config?.rune_audio_external_target : null;
    if (external) {
      setChanging(true);
      try {
        if (!enabled) {
          await persistExternalState(external, false);
          await invokeCommand("stop_rune_audio_monitor");
          return;
        }
        const state = await inspection.inspect(installationTargetId(external.edition), true, { edition: external.edition, modName: external.mod_name });
        if (revision !== requestRevision.current) return;
        inspection.accept(state, installationTargetId(external.edition));
        if (state.ready && external.mod_name) await persistExternalState(external, true);
        else latest.current.requestProcessing({ origin: "recognition", edition: external.edition, installationOnly: true,
          ...(external.mod_name ? { source: { name: external.mod_name, processed: state.feature_groups.length > 0 || state.update_required } } : {}) });
      } catch (error) { if (revision === requestRevision.current) showToast("error", `无法开启声纹识别：${error}`); }
      finally { if (revision === requestRevision.current) setChanging(false); }
      return;
    }
    if (!enabled) {
      await persistAudioEnabledState(latest.current.trackingTargetId, false);
      await invokeCommand("stop_rune_audio_monitor").catch(() => undefined);
      return;
    }
    const accountId = preferredAccountId || latest.current.trackingTargetId;
    if (!accountId) { showToast("warning", latest.current.initializedAccounts.length ? "请先选择监听账号" : "请先初始化一个账号"); return; }
    setChanging(true);
    try {
      await persistAudioEnabledState(accountId, true);
      if (revision !== requestRevision.current) return;
      await invokeCommand("start_rune_audio_monitor").catch(() => undefined);
    } catch (error) {
      if (revision === requestRevision.current) showToast("error", `无法开启声纹识别：${error}`);
    } finally { if (revision === requestRevision.current) setChanging(false); }
  };

  const completeModProcessing = async ({ origin, accountId, state, installationEdition }: ModAppliedResult) => {
    if (installationEdition) {
      const current = latest.current.config;
      if (origin === "recognition" && current?.rune_audio_external_target?.edition === installationEdition) {
        const target = { edition: installationEdition, mod_name: state.current_mod_name ?? "" };
        await persistExternalState(target, hasAudioTelemetry(state.feature_groups) && state.ready);
        inspection.accept(state, installationTargetId(installationEdition));
      }
      return;
    }
    const current = latest.current.config;
    if (current?.rune_audio_external_target) return;
    inspection.accept(state);
    if (origin === "recognition") await persistAudioEnabledState(accountId, hasAudioTelemetry(state.feature_groups));
    else if (current?.rune_audio_target_account === accountId) {
      await persistAudioEnabledState(accountId, !!current.rune_audio_enabled && hasAudioTelemetry(state.feature_groups));
    }
  };

  const toggleAudioDiagnosticRecording = async () => {
    try {
      if (audioStatus?.diagnostic_recording) {
        const path = await invokeCommand<string | null>("stop_rune_audio_diagnostic_recording");
        setAudioStatus(previous => previous ? { ...previous, diagnostic_recording: false,
          diagnostic_recording_path: path ?? previous.diagnostic_recording_path } : previous);
        if (path) showToast("success", `诊断录音已保存：${path}`);
      } else {
        const path = await invokeCommand<string>("start_rune_audio_diagnostic_recording");
        setAudioStatus(previous => previous ? { ...previous, diagnostic_recording: true, diagnostic_recording_path: path } : previous);
        showToast("success", "诊断录音已开始，仅录制目标 D2R 进程的声音");
      }
    } catch (error) { showToast("error", `切换诊断录音失败: ${error}`); }
  };

  return {
    audioStatus, audioModState: inspection.state, audioModStateLoading: inspection.loading || changing,
    audioModError: inspection.error,
    hasInitializedAudioAccount: initializedAccounts.length > 0, hasAudioTarget: !!external || !!trackingTargetId,
    hasReadyAudioMod: !!hasReadyAudioMod, isAudioEnableRequested: !!config?.rune_audio_enabled,
    isAudioRecognitionActive: optionalFeaturesAvailable && !!config?.rune_audio_enabled
      && audioStatus?.running === true && (external
        ? !!config && matchesRecognitionSource(config, audioStatus) : audioStatus.account_id === trackingTargetId),
    refreshAudioModState: inspection.refresh, handleAudioTargetChange, handleAudioToggle,
    completeModProcessing, toggleAudioDiagnosticRecording, handleExternalTargetChange, externalInstances, externalInstancesError, selectExternalInstance,
  };
}
