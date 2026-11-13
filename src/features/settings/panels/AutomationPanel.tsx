import {
  ChevronDown,
  RotateCw,
} from "lucide-react";
import { useState } from "react";
import { Button } from "../../../components/ui/Button";
import { RangeSlider } from "../../../components/ui/RangeSlider";
import { Toggle } from "../../../components/ui/Toggle";
import { showToast } from "../../../components/ui/Toast";
import { invokeCommand } from "../../../platform/tauri";
import type { AccountMeta, AudioModSetupState, GlobalConfig, ModCapsulePool } from "../../../store/types";
import { validateTrackingTarget } from "../../../utils/trackingTarget";
import {
  AGGREGATE_ITEM_FILTERS,
  CHARM_FILTERS,
  DEFAULT_TRACKING_CATEGORIES,
  GEM_LEVELS,
  TRACKING_CATEGORIES,
  type RuneAudioStatus,
  type ExternalAudioInstance,
} from "../audioModuleModel";

import { defaultExternalAudioTarget, type ExternalAudioTarget } from "../../../utils/recognitionSource";
import { ExternalRecognitionTarget } from "./ExternalRecognitionTarget";
import { HostileRestartSettings } from "./HostileRestartSettings";
import { VisualRecognitionSettings } from "./VisualRecognitionSettings";
import "./recognitionSettings.css";

type TrackingTarget = ReturnType<typeof validateTrackingTarget>;

interface AutomationPanelProps {
  config: GlobalConfig;
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
  persistConfig: (draft: GlobalConfig, quiet?: boolean) => Promise<unknown>;
  initializedTrackingAccounts: AccountMeta[];
  trackingTarget: TrackingTarget;
  audioStatus: RuneAudioStatus | null;
  audioModState: AudioModSetupState | null;
  audioModStateLoading: boolean;
  modCapsulePool?: ModCapsulePool | null;
  assigningCapsuleAccountId?: string | null;
  onAssignModCapsule?: (accountId: string, capsuleId: string) => Promise<unknown>;
  onPrepareModCapsule?: (accountId: string, capsuleId: string) => void;
  onOpenModProcessing?: () => void;
  audioPreparing: boolean;
  hasInitializedAudioAccount: boolean;
  hasAudioTarget: boolean;
  hasReadyAudioMod: boolean;
  isAudioEnableRequested: boolean;
  isAudioRecognitionActive: boolean;
  externalInstances?: ExternalAudioInstance[];
  externalInstancesError?: string | null;
  onExternalTargetChange?: (target: ExternalAudioTarget) => Promise<void>;
  onSelectExternalInstance?: (instance: ExternalAudioInstance) => Promise<void>;
  onAudioTargetChange: (accountId: string) => Promise<void>;
  onAudioToggle: (enabled: boolean) => Promise<boolean | void>;
  onToggleDiagnosticRecording: () => Promise<void>;
  onClose: () => void;
  onInitializeAccount: () => void;
}

export function AutomationPanel({
  config,
  updateConfig,
  persistConfig: persistGlobalDraft,
  initializedTrackingAccounts,
  trackingTarget,
  audioStatus,
  audioModState,
  audioModStateLoading,
  modCapsulePool = null,
  onOpenModProcessing = () => undefined,
  audioPreparing,
  isAudioEnableRequested, isAudioRecognitionActive,
  externalInstances = [], externalInstancesError = null, onExternalTargetChange, onSelectExternalInstance,
  onAudioTargetChange: handleAudioTargetChange,
  onAudioToggle: handleAudioToggle,
  onToggleDiagnosticRecording: toggleAudioDiagnosticRecording,
  onClose,
  onInitializeAccount,
}: AutomationPanelProps) {
  const external = config.rune_audio_external_target;
  const isEnglish = config.app_language === "en-US";
  const trackedCategories = config.rune_audio_tracked_categories ?? DEFAULT_TRACKING_CATEGORIES;
  const filterSummary = trackedCategories.length
    ? isEnglish
      ? `${trackedCategories.length} drop categories${trackedCategories.includes("runes") ? ` · Runes #${config.rune_audio_min_rune_number ?? 1}+` : ""}`
      : `记录 ${trackedCategories.length} 类掉落${trackedCategories.includes("runes") ? ` · 符文 #${config.rune_audio_min_rune_number ?? 1} 起` : ""}`
    : isEnglish ? "Drops off; scene tracking and run timing remain on." : "暂不记录掉落，场景与刷图计时不受影响";
  const [section, setSection] = useState("listening");
  const requestAudioToggle = (enabled: boolean) => { void handleAudioToggle(enabled); };
  return (
<div className="recognition-settings settings-guided-page">
  <header className="recognition-overview">
    <div className="flex items-center justify-between py-1">
      <div className="min-w-0 pr-4">
        <span className="text-sm font-bold text-text-secondary">{isEnglish ? "Audio recognition" : "音频声纹自动识别"}</span>
        <p className="text-xs text-text-muted">{external ? (isEnglish ? "Track drops and run times for the selected game." : "记录所选游戏的掉落与刷图用时。") : isEnglish ? "Track drops and run times for the selected account." : "记录所选账号的掉落与刷图用时。"}</p>
      </div>
      <Toggle
        checked={isAudioEnableRequested}
        disabled={audioPreparing || audioModStateLoading}
        ariaLabel={isEnglish ? "Enable audio recognition" : "启用音频声纹自动识别"}
        descriptionId="rune-audio-readiness"
        onChange={requestAudioToggle}
      />
    </div>
    <p id="rune-audio-readiness" className="runtime-state" role="status" data-state={!isAudioEnableRequested ? "off" : audioStatus?.last_error ? "attention" : isAudioRecognitionActive ? "running" : "waiting"}>{isAudioEnableRequested
      ? audioStatus?.last_error ? (isEnglish ? "Enabled · Action needed" : "已启用，需要处理") : isAudioRecognitionActive ? (isEnglish ? "Listening" : "正在监听") : (isEnglish ? "Enabled · Waiting for game" : "已启用，等待游戏运行")
      : (isEnglish ? "Recognition is off" : "识别未启用")}</p>
    {isAudioEnableRequested && audioStatus?.last_error && <p role="status" className="text-xs text-warning break-words">{audioStatus.last_error}</p>}
    {config.hostile_auto_restart_enabled && audioStatus?.hostile_error && <p role="status" className="text-xs text-warning break-words">{isEnglish ? "Hostile auto-restart unavailable: " : "敌对自动重开不可用："}{audioStatus.hostile_error}</p>}
  </header>
  <nav className="settings-section-tabs" aria-label={isEnglish ? "Recognition sections" : "识别设置分页"}>
    {[["listening", isEnglish ? "Listening settings" : "监听设置"], ["filters", isEnglish ? "Recording" : "记录内容"], ["advanced", isEnglish ? "Diagnostics & advanced" : "诊断与高级"]].map(([id, label]) =>
      <button type="button" key={id} aria-current={section === id ? "page" : undefined} onClick={() => setSection(id)}>{label}</button>)}
  </nav>
  <section className="settings-section-body recognition-control-panel" hidden={section !== "listening"} aria-label={isEnglish ? "Listening settings" : "监听设置"}>


    <div className="recognition-target-section">
      <div className="recognition-target-heading">
        <div>
          <label htmlFor="rune-audio-target-account" className="text-sm font-semibold text-text-secondary">
            {isEnglish ? "Listening target" : "监听目标"}
          </label>
          <p className="text-2xs text-text-muted">{isEnglish ? "Choose an account or specify a game process" : "选择账号，或指定要监听的游戏进程"}</p>
        </div>
        <select
          id="rune-audio-target-account"
          value={external ? "external" : trackingTarget.valid ? trackingTarget.account.id : ""}
          disabled={audioPreparing || audioModStateLoading}
          aria-describedby="rune-audio-target-help"
          onChange={e => e.target.value === "external"
            ? void onExternalTargetChange?.(defaultExternalAudioTarget(config)) : void handleAudioTargetChange(e.target.value)}
          className="settings-input recognition-target-select"
        >
          <option value="" disabled>
            {initializedTrackingAccounts.length === 0 ? "暂无可用账号" : "请选择账号"}
          </option>
          <option value="external">{isEnglish ? "Specify game process (no account needed)" : "指定游戏进程（无需账号）"}</option>
          {initializedTrackingAccounts.map(account => (
            <option key={account.id} value={account.id}>{account.display_name || account.id}</option>
          ))}
        </select>
      </div>
      <p id="rune-audio-target-help" aria-live="polite" className="text-2xs text-text-secondary">
        {external ? (isEnglish ? "Choose a process below, including games started through Battle.net or a shortcut." : "在下方指定游戏进程，也支持通过战网或快捷方式启动的游戏。") : initializedTrackingAccounts.length === 0
          ? (isEnglish ? "Initialize an account to start tracking it." : "先初始化一个游戏账号，再选择监听目标。")
          : trackingTarget.valid
            ? `只识别“${trackingTarget.account.display_name || trackingTarget.account.id}”对应的游戏声音。`
            : (isEnglish ? "Select an account to listen to." : "请选择要监听的账号。")}
      </p>

    </div>

    {external && <ExternalRecognitionTarget config={config} state={audioModState} status={audioStatus} pool={modCapsulePool}
      busy={audioPreparing || audioModStateLoading} instances={externalInstances} instancesError={externalInstancesError}
      onChange={onExternalTargetChange ?? (async () => {})} onSelectInstance={onSelectExternalInstance ?? (async () => {})} onPrepare={onOpenModProcessing} />}

    {!external && !initializedTrackingAccounts.length && <Button size="sm" onClick={() => { onClose(); onInitializeAccount?.(); }}>初始化账号</Button>}
    {!external && initializedTrackingAccounts.length > 0 && !trackingTarget.valid && <Button size="sm" onClick={() => void handleAudioTargetChange(initializedTrackingAccounts[0].id)}>{isEnglish ? "Select first account" : "选择首个账号"}</Button>}
    {!external && <p className="feature-help">{isEnglish ? "Recognition support is checked against the actual launch arguments before starting." : "启动前会根据本次实际使用的 Mod 检查识别支持。"}</p>}

    <div className="settings-next-links">
      <button type="button" onClick={() => setSection("filters")}><span>{isEnglish ? "Drops to record" : "记录哪些掉落"}</span><small>{filterSummary}</small><ChevronDown size={14} aria-hidden="true" /></button>
      <Button size="sm" variant="ghost" onClick={async () => {
        try { await invokeCommand("open_stats_page"); }
        catch (e) { showToast("error", `${isEnglish ? "Could not open statistics: " : "打开统计界面失败: "}${e}`); }
      }}>{isEnglish ? "Open statistics" : "查看统计图表"}</Button>
    </div>
  </section>
  <section className="settings-section-body" hidden={section !== "advanced"} aria-label={isEnglish ? "Diagnostics & advanced" : "诊断与高级"}>
    <HostileRestartSettings config={config} persist={persistGlobalDraft} error={audioStatus?.hostile_error ?? audioStatus?.last_error} />
    {config.rune_audio_enabled && (trackingTarget.valid || external) && (
      <details className="settings-diagnostics group border-t border-border-default/50 pt-3">
        <summary className="flex cursor-pointer list-none items-center justify-between text-xs font-medium text-text-secondary">
          {isEnglish ? "Recognition diagnostics" : "识别诊断"}
          <ChevronDown size={14} className="transition-transform duration-200 group-open:rotate-180" />
        </summary>
        <div className="mt-3 space-y-3">
    <div className="recognition-monitor">
      <div className="flex items-center justify-between text-xs">
        <span className={audioStatus?.running ? "text-success" : "text-text-secondary"}>
          {audioStatus?.running ? `正在捕获 · PID ${audioStatus.target_pid}` : "监控未运行"}
        </span>
        <span className="text-text-muted">数据包 {audioStatus?.decoded_packets ?? 0}</span>
      </div>
      <div className="grid grid-cols-4 gap-2 text-center text-2xs">
        <div className="rounded bg-surface-hover px-2 py-1.5">
          <span className="block text-text-muted">音频峰值</span>
          <span className="font-mono text-text-primary">
            {audioStatus ? audioStatus.audio_peak.toFixed(4) : "0.0000"}
          </span>
        </div>
        <div className="rounded bg-surface-hover px-2 py-1.5">
          <span className="block text-text-muted">符文</span>
          <span className="font-mono text-text-primary">{audioStatus?.rune_events ?? 0}</span>
        </div>
        <div className="rounded bg-surface-hover px-2 py-1.5">
          <span className="block text-text-muted">物品</span>
          <span className="font-mono text-text-primary">{audioStatus?.item_events ?? 0}</span>
        </div>
        <div className="rounded bg-surface-hover px-2 py-1.5">
          <span className="block text-text-muted">地点信号</span>
          <span className="font-mono text-text-primary">{audioStatus?.scene_ocr_events ?? 0}</span>
        </div>
      </div>
      {audioStatus?.last_marker && (
        <p className="text-2xs text-success">
          最近识别：{audioStatus.last_marker} · {((audioStatus.last_confidence ?? 0) * 100).toFixed(1)}%
        </p>
      )}
      {audioStatus?.last_error && (
        <p className="text-2xs text-danger break-all">{audioStatus.last_error}</p>
      )}
    </div>


          <div className="recognition-threshold">
            <div>
              <span className="text-xs font-semibold text-text-secondary">识别阈值</span>
              <p className="text-2xs text-text-muted">默认 0.56；没有误识别时无需调整</p>
            </div>
            <input
              type="number"
              aria-label="识别阈值"
              min={0.4}
              max={0.95}
              step={0.01}
              value={config.rune_audio_detection_threshold ?? 0.56}
              onChange={event => updateConfig(c => {
                c.rune_audio_detection_threshold = Number(event.target.value);
              })}
              className="settings-input"
            />
          </div>

          <Button
            variant="secondary"
            size="md"
            onClick={async () => {
              try {
                await persistGlobalDraft(config, true);
                await invokeCommand("restart_rune_audio_monitor");
                showToast("success", "声纹监控已用新配置重启");
              } catch (e) {
                showToast("error", "重启声纹监控失败: " + e);
              }
            }}
            className="w-full"
          >
            <RotateCw size={13} className="shrink-0" />
            应用并重启识别
          </Button>

          <div className="border-t border-border-default/50 pt-3">
            <Button
              variant={audioStatus?.diagnostic_recording ? "danger" : "secondary"}
              size="md"
              disabled={!audioStatus?.running}
              onClick={toggleAudioDiagnosticRecording}
              className="w-full"
            >
              {audioStatus?.diagnostic_recording ? "停止并保存诊断录音" : "开始诊断录音"}
            </Button>
            <p className="mt-1 text-center text-2xs text-text-muted break-all">
              {audioStatus?.diagnostic_recording
                ? "正在录制目标游戏的声音并保存识别事件"
                : audioStatus?.diagnostic_recording_path
                  ? `最近保存：${audioStatus.diagnostic_recording_path}`
                  : "仅录制目标游戏，不录制麦克风或其他应用"}
            </p>
          </div>
        </div>
      </details>
    )}
    <details className="settings-help-disclosure"><summary>{isEnglish ? "How recognition works" : "识别说明"}<ChevronDown size={14} aria-hidden="true" /></summary><div className="recognition-notes">
      <p>{isEnglish ? "Only the selected game process is captured. No game memory is read and no code is injected." : "只捕获所选游戏进程的声音，不读取游戏内存，也不会向游戏注入代码。"}</p>
      <p>{isEnglish ? "Audio identifies base item codes; unique, set and affixes sharing a code cannot be distinguished by audio alone." : "声纹按基础物品代码识别；同一代码的暗金、套装或词缀无法仅凭音频区分。"}</p>
    </div></details>
  </section>
  <section className="settings-section-body recognition-side" hidden={section !== "filters"} aria-label={isEnglish ? "Recording" : "记录内容"}>

    <VisualRecognitionSettings mode="audio" language={config.app_language} />
    <label className="feature-setting-row">
      <span>{isEnglish ? "Timer display" : "计时显示格式"}</span>
      <select className="settings-input" value={config.combat_time_format || "seconds"}
        onChange={event => updateConfig(next => { next.combat_time_format = event.target.value; })}>
        <option value="seconds">{isEnglish ? "Seconds · 75.4" : "秒 · 75.4"}</option>
        <option value="minutes">{isEnglish ? "Minutes and seconds · 01:15" : "分与秒 · 01:15"}</option>
        <option value="hours">{isEnglish ? "Hours, minutes and seconds · 0:01:15" : "时、分与秒 · 0:01:15"}</option>
      </select>
    </label>
    <details open className="spatial-panel group overflow-hidden recognition-filters">
      <summary className="flex cursor-pointer list-none items-center justify-between gap-3 p-4">
        <span>
          <span className="block text-sm font-bold text-text-primary">{isEnglish ? "Drops to record" : "记录哪些掉落"}</span>
          <span className="mt-0.5 block text-xs text-text-muted">{filterSummary}</span>
        </span>
        <ChevronDown size={15} className="shrink-0 text-text-muted transition-transform duration-200 group-open:rotate-180" />
      </summary>

      <div className="border-t border-border-default/50 px-4 pb-4 pt-3">
        <div className="space-y-4">
          {([
            {
              id: "runes",
              label: "符文",
              value: config.rune_audio_min_rune_number ?? 1,
              max: 33,
              valueLabel: `#${config.rune_audio_min_rune_number ?? 1}–#33`,
              detail: "最低编号（含）；滑到 #20 时只记录 #20–#33",
              onChange: (value: number) => updateConfig(next => { next.rune_audio_min_rune_number = value; }),
            },
            {
              id: "gems",
              label: "宝石与骷髅",
              value: config.rune_audio_min_gem_level ?? 1,
              max: 5,
              valueLabel: `${GEM_LEVELS[(config.rune_audio_min_gem_level ?? 1) - 1]}及以上`,
              detail: "五档品质：碎裂、裂开、普通、无瑕疵、完美",
              onChange: (value: number) => updateConfig(next => { next.rune_audio_min_gem_level = value; }),
            },
          ] as const).map(filter => {
            const enabled = (config.rune_audio_tracked_categories ?? DEFAULT_TRACKING_CATEGORIES).includes(filter.id);
            return (
              <div key={filter.id} className={enabled ? "" : "opacity-55"}>
                <div className="flex items-center justify-between gap-3">
                  <label className="flex cursor-pointer items-center gap-2 text-xs font-semibold text-text-secondary">
                    <input
                      type="checkbox"
                      checked={enabled}
                      onChange={event => updateConfig(next => {
                        const current = new Set(next.rune_audio_tracked_categories ?? DEFAULT_TRACKING_CATEGORIES);
                        if (event.target.checked) current.add(filter.id);
                        else current.delete(filter.id);
                        next.rune_audio_tracked_categories = TRACKING_CATEGORIES
                          .map(item => item.id)
                          .filter(id => current.has(id));
                      })}
                      className="accent-[var(--accent)]"
                    />
                    {filter.label}
                  </label>
                  <span className="rounded-md bg-surface-hover px-2 py-0.5 font-mono text-2xs font-semibold text-text-primary">
                    {filter.valueLabel}
                  </span>
                </div>
                <RangeSlider
                  min={1}
                  max={filter.max}
                  step={1}
                  value={filter.value}
                  disabled={!enabled}
                  aria-label={`${filter.label}最低记录等级`}
                  onChange={event => filter.onChange(Number(event.target.value))}
                  className="mt-2 w-full"
                />
                <div className="mt-1 flex items-center justify-between text-2xs text-text-muted">
                  <span>{filter.detail}</span>
                  <span className="ml-3 shrink-0">1 — {filter.max}</span>
                </div>
              </div>
            );
          })}

          <div className="border-t border-border-default/50 pt-3">
            <p className="mb-2 text-2xs font-semibold text-text-muted">护身符 · 分别选择</p>
            <div className="recognition-charm-grid">
              {CHARM_FILTERS.map(item => {
                const categories = config.rune_audio_tracked_categories ?? DEFAULT_TRACKING_CATEGORIES;
                const codes = config.rune_audio_tracked_charm_codes ?? CHARM_FILTERS.map(filter => filter.code);
                const selected = categories.includes("charms") && codes.includes(item.code);
                return (
                  <label
                    key={item.code}
                    className={`cursor-pointer rounded-lg border px-2 py-2 transition-colors ${selected
                      ? "border-accent/40 bg-accent/5"
                      : "border-border-default bg-surface-hover"}`}
                  >
                    <span className="flex items-start gap-1.5">
                      <input
                        type="checkbox"
                        checked={selected}
                        onChange={event => updateConfig(next => {
                          const currentCategories = new Set(next.rune_audio_tracked_categories ?? DEFAULT_TRACKING_CATEGORIES);
                          const charmCodes = new Set(
                            currentCategories.has("charms")
                              ? next.rune_audio_tracked_charm_codes ?? CHARM_FILTERS.map(filter => filter.code)
                              : [],
                          );
                          if (event.target.checked) charmCodes.add(item.code);
                          else charmCodes.delete(item.code);
                          next.rune_audio_tracked_charm_codes = CHARM_FILTERS
                            .map(filter => filter.code)
                            .filter(code => charmCodes.has(code));
                          if (next.rune_audio_tracked_charm_codes.length > 0) currentCategories.add("charms");
                          else currentCategories.delete("charms");
                          next.rune_audio_tracked_categories = TRACKING_CATEGORIES
                            .map(filter => filter.id)
                            .filter(id => currentCategories.has(id));
                        })}
                        className="mt-0.5 accent-[var(--accent)]"
                      />
                      <span className="min-w-0">
                        <span className="block text-2xs font-semibold leading-tight text-text-secondary">{item.label}</span>
                        <span className="mt-1 block text-2xs leading-relaxed text-text-muted">{item.detail}</span>
                      </span>
                    </span>
                  </label>
                );
              })}
            </div>
          </div>

          <div className="border-t border-border-default/50 pt-3">
            <p className="mb-2 text-2xs font-semibold text-text-muted">其他物品 · 按整项选择</p>
            <div className="grid grid-cols-2 gap-x-3 gap-y-1">
              {AGGREGATE_ITEM_FILTERS.map(item => {
                const selected = (config.rune_audio_tracked_categories ?? DEFAULT_TRACKING_CATEGORIES).includes(item.id);
                return (
                  <label key={item.id} className="flex cursor-pointer items-start gap-2 border-b border-border-default/40 py-2 last:border-b-0">
                    <input
                      type="checkbox"
                      checked={selected}
                      onChange={event => updateConfig(next => {
                        const current = new Set(next.rune_audio_tracked_categories ?? DEFAULT_TRACKING_CATEGORIES);
                        if (event.target.checked) current.add(item.id);
                        else current.delete(item.id);
                        next.rune_audio_tracked_categories = TRACKING_CATEGORIES
                          .map(filter => filter.id)
                          .filter(id => current.has(id));
                      })}
                      className="mt-0.5 accent-[var(--accent)]"
                    />
                    <span>
                      <span className="block text-xs font-semibold text-text-secondary">{item.label}</span>
                      <span className="block text-2xs leading-relaxed text-text-muted">{item.detail}</span>
                    </span>
                  </label>
                );
              })}
            </div>
          </div>

          <p className="border-t border-border-default/50 pt-3 text-2xs leading-relaxed text-text-muted">
            所有掉落仅在已确认的野外或地下城场景中记录；主城、主界面和尚未识别地点时一律忽略。修改后应用过滤规则，识别会重新开始。
          </p>
        </div>
      </div>
    </details>

    <Button size="sm" variant="secondary" onClick={async () => {
      try { await persistGlobalDraft(config, true); await invokeCommand("restart_rune_audio_monitor"); showToast("success", isEnglish ? "Drop filters applied" : "掉落过滤规则已应用"); }
      catch (e) { showToast("error", `${isEnglish ? "Could not apply filters: " : "应用过滤规则失败: "}${e}`); }
    }}>{isEnglish ? "Apply drop filters" : "应用掉落过滤"}</Button>
  </section>
</div>
  );
}
