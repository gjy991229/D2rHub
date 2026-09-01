import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  Package,
  Play,
  RotateCw,
} from "lucide-react";
import type { CSSProperties, Dispatch, SetStateAction } from "react";
import { Button } from "../../../components/ui/Button";
import { Toggle } from "../../../components/ui/Toggle";
import { showToast } from "../../../components/ui/Toast";
import { invokeCommand } from "../../../platform/tauri";
import type { AccountMeta, AudioModSetupState, GlobalConfig, ModCapsulePool } from "../../../store/types";
import {
  AUDIO_TELEMETRY_CAPSULE_FEATURE,
  capsuleSelectionForAccount,
  compatibleCapsulesForAccount,
} from "../../modCapsules/model";
import { AUDIO_MOD_NAME_MAX_LENGTH } from "../../../utils/audioModName";
import { validateTrackingTarget } from "../../../utils/trackingTarget";
import {
  AGGREGATE_ITEM_FILTERS,
  AUDIO_TELEMETRY_FEATURE_ID,
  CHARM_FILTERS,
  DEFAULT_TRACKING_CATEGORIES,
  GEM_LEVELS,
  IN_GAME_ROOM_TOOLS_FEATURE_ID,
  TRACKING_CATEGORIES,
  type AudioModPrepareProgress,
  type RuneAudioStatus,
} from "../audioModuleModel";

type TrackingTarget = ReturnType<typeof validateTrackingTarget>;
type AudioSetupMode = "original" | "existing";

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
  audioSetupOpen: boolean;
  showModProcessing?: boolean;
  onOpenModProcessing?: () => void;
  onOpenAudioSetup: () => void;
  onCloseAudioSetup: () => void;
  audioSetupMode: AudioSetupMode;
  setAudioSetupMode: Dispatch<SetStateAction<AudioSetupMode>>;
  audioSetupSource: string;
  setAudioSetupSource: Dispatch<SetStateAction<string>>;
  audioSetupName: string;
  setAudioSetupName: Dispatch<SetStateAction<string>>;
  includeAudioTelemetry: boolean;
  setIncludeAudioTelemetry: Dispatch<SetStateAction<boolean>>;
  includeRoomTools: boolean;
  setIncludeRoomTools: Dispatch<SetStateAction<boolean>>;
  audioPreparing: boolean;
  audioPrepareProgress: AudioModPrepareProgress | null;
  isAudioModUpgrade: boolean;
  isAudioModFeatureManagement: boolean;
  audioSetupNameError: string | null;
  showAudioSetupNameError: boolean;
  hasInitializedAudioAccount: boolean;
  hasAudioTarget: boolean;
  hasReadyAudioMod: boolean;
  isAudioEnableRequested: boolean;
  isAudioRecognitionActive: boolean;
  audioPrepareBlockedReason: string;
  onAudioTargetChange: (accountId: string) => Promise<void>;
  onAudioToggle: (enabled: boolean) => Promise<void>;
  onPrepareAudioMod: () => Promise<void>;
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
  assigningCapsuleAccountId = null,
  onAssignModCapsule,
  audioSetupOpen,
  showModProcessing = false,
  onOpenModProcessing = () => undefined,
  onOpenAudioSetup,
  onCloseAudioSetup,
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
  onAudioTargetChange: handleAudioTargetChange,
  onAudioToggle: handleAudioToggle,
  onPrepareAudioMod: handlePrepareAudioMod,
  onToggleDiagnosticRecording: toggleAudioDiagnosticRecording,
  onClose,
  onInitializeAccount,
}: AutomationPanelProps) {
  const isEnglish = config.app_language === "en-US";
  const installedAudioTelemetry = !!audioModState?.feature_groups.includes(AUDIO_TELEMETRY_FEATURE_ID);
  const installedRoomTools = !!audioModState?.feature_groups.includes(IN_GAME_ROOM_TOOLS_FEATURE_ID);
  const trackingAccountId = trackingTarget.valid ? trackingTarget.account.id : "";
  const audioCapsules = compatibleCapsulesForAccount(modCapsulePool, trackingAccountId)
    .filter((capsule) => capsule.feature_groups.includes(AUDIO_TELEMETRY_CAPSULE_FEATURE));
  const audioCapsuleSelection = capsuleSelectionForAccount(modCapsulePool, trackingAccountId);
  const featureCopy = isEnglish
    ? {
        title: "Mod features",
        description: isAudioModFeatureManagement
          ? "Add capabilities to the current Mod. Installed features are preserved and cannot be removed here."
          : "Choose the capabilities to package into this Mod. You can enable either feature or both.",
        audioTitle: "Audio recognition",
        audioDetail: "Recognize scenes, drops, and Terror Zones",
        roomTitle: "In-game room tools",
        roomDetail: "Quickly recreate, create, and join rooms",
        installed: "Installed · kept",
        manage: "Manage features",
        cancel: "Cancel",
        manageTitle: "Manage Mod features",
      }
    : {
        title: "Mod 功能",
        description: isAudioModFeatureManagement
          ? "为当前 Mod 增补能力；已经安装的功能会保留，无法在这里移除。"
          : "选择要打包进这个 Mod 的能力；可以只选一项，也可以同时启用。",
        audioTitle: "声纹识别",
        audioDetail: "场景、掉落与恐怖区域识别",
        roomTitle: "局内房间工具",
        roomDetail: "快速重开、创建与加入房间",
        installed: "已安装 · 保留",
        manage: "管理功能",
        cancel: "取消",
        manageTitle: "管理 Mod 功能",
      };
  return (
<div className="settings-content-grid recognition-layout">
  <div className="spatial-panel recognition-control-panel">
    <div className="flex items-center justify-between py-1">
      <div className="min-w-0 pr-4">
        <span className="text-sm font-bold text-text-secondary">音频声纹自动识别</span>
        <p className="text-2xs text-text-muted">按 D2R 进程捕获 Mod 音频；自动识别所选掉落、场景切换并完成刷图计时统计</p>
      </div>
      <Toggle
        checked={isAudioEnableRequested}
        disabled={audioPreparing || audioModStateLoading}
        ariaLabel="启用音频声纹自动识别"
        descriptionId="rune-audio-readiness"
        onChange={handleAudioToggle}
      />
    </div>

    <div
      id="rune-audio-readiness"
      className="recognition-readiness"
      data-state={isAudioRecognitionActive ? "running" : hasReadyAudioMod ? "ready" : "attention"}
      role="status"
      aria-live="polite"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2.5">
          {isAudioRecognitionActive || hasReadyAudioMod
            ? <CheckCircle2 size={16} className={`mt-0.5 shrink-0 ${isAudioRecognitionActive ? "text-success" : "text-accent"}`} />
            : <AlertTriangle size={16} className="mt-0.5 shrink-0 text-warning" />}
          <div className="min-w-0">
            <p className="text-xs font-semibold text-text-primary">
              {isAudioRecognitionActive
                ? "声纹识别已开启"
                : !hasInitializedAudioAccount
                  ? isAudioEnableRequested ? "开启尚未完成：初始化账号" : "先初始化一个游戏账号"
                  : !hasAudioTarget
                    ? isAudioEnableRequested ? "开启尚未完成：选择监听账号" : "第 2 步：选择监听账号"
                    : audioModStateLoading
                      ? "正在检查识别 Mod"
                      : !hasReadyAudioMod
                        ? isAudioEnableRequested ? "开启尚未完成：准备识别 Mod" : "还差一步：准备识别 Mod"
                        : "准备完成，可以开启识别"}
            </p>
            <p className="mt-0.5 text-2xs leading-relaxed text-text-secondary">
              {isAudioRecognitionActive
                ? audioModState?.restart_required
                  ? "配置已完成；请重启该账号的游戏，让新的 Mod 启动参数生效。"
                  : "D2RHub 会锁定所选账号的 D2R 进程，不会录制其他应用声音。"
                : !hasInitializedAudioAccount
                  ? "声纹需要绑定一个可启动的账号。点击右侧按钮完成初始化，再回来选择账号和准备 Mod。"
                  : !hasAudioTarget
                    ? "声音按 D2R 进程隔离捕获；先明确要统计哪个账号。"
                    : !hasReadyAudioMod
                      ? "D2R 需要播放极短的识别音频。D2RHub 会保留你的原 Mod，并自动生成启动参数。"
                      : "所有前置项均已完成。点击开启后，目标游戏运行时会自动开始识别。"}
            </p>
          </div>
        </div>
        {!isAudioRecognitionActive && (
          <Button
            variant={hasReadyAudioMod ? "primary" : "secondary"}
            size="sm"
            className="shrink-0"
            disabled={audioPreparing || audioModStateLoading}
            onClick={() => {
              if (!hasInitializedAudioAccount) {
                onClose();
                onInitializeAccount();
                return;
              }
              if (!hasAudioTarget) {
                const firstAccount = initializedTrackingAccounts[0];
                if (firstAccount) void handleAudioTargetChange(firstAccount.id);
                return;
              }
              void handleAudioToggle(true);
            }}
          >
            {!hasInitializedAudioAccount
              ? "初始化账号"
              : !hasAudioTarget
                ? "选择首个账号"
                : !hasReadyAudioMod
                  ? "开始准备"
                  : "立即开启"}
          </Button>
        )}
      </div>
      <ol className="recognition-checklist" aria-label="声纹识别启用步骤">
        {[
          { label: "初始化账号", complete: hasInitializedAudioAccount },
          { label: "选择监听账号", complete: hasAudioTarget },
          { label: "准备识别 Mod", complete: hasReadyAudioMod },
        ].map((step, index) => (
          <li
            key={step.label}
            className="recognition-checklist-item"
            data-complete={step.complete ? "true" : undefined}
          >
            <span
              className="recognition-checklist-marker"
              aria-hidden="true"
            >
              {step.complete ? "✓" : index + 1}
            </span>
            <span className="truncate">{step.label}</span>
          </li>
        ))}
      </ol>
    </div>

    <div className="recognition-target-section">
      <div className="flex items-center justify-between gap-4">
        <div>
          <label htmlFor="rune-audio-target-account" className="text-sm font-semibold text-text-secondary">
            识别目标账号
          </label>
          <p className="text-2xs text-text-muted">选择一个已初始化账号，声音只从其 D2R PID 捕获</p>
        </div>
        <select
          id="rune-audio-target-account"
          value={trackingTarget.valid ? trackingTarget.account.id : ""}
          disabled={initializedTrackingAccounts.length === 0}
          aria-describedby="rune-audio-target-help"
          onChange={e => void handleAudioTargetChange(e.target.value)}
          className="h-8 min-w-36 px-2.5 rounded-lg bg-surface-hover border border-border-default text-text-primary text-xs disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <option value="" disabled>
            {initializedTrackingAccounts.length === 0 ? "暂无可用账号" : "请选择账号"}
          </option>
          {initializedTrackingAccounts.map(account => (
            <option key={account.id} value={account.id}>{account.display_name || account.id}</option>
          ))}
        </select>
      </div>
      <p id="rune-audio-target-help" aria-live="polite" className="text-2xs text-text-secondary">
        {initializedTrackingAccounts.length === 0
          ? "上方“初始化账号”会直接打开账号向导；完成后回到这里继续。"
          : trackingTarget.valid
            ? `只识别“${trackingTarget.account.display_name || trackingTarget.account.id}”对应的游戏声音。`
            : "必须先选择目标账号；也可点击上方“选择首个账号”快速继续。"}
      </p>
      {trackingTarget.valid && (
        <div className="recognition-capsule-selector">
          <span className="recognition-capsule-copy">
            {audioModStateLoading
              ? "正在检查当前 Mod…"
              : audioModState?.ready
                ? `${audioModState.current_mod_name} · ${audioModState.feature_groups.length} 个功能模块`
                : "当前账号还没有可用的 D2RHub 加工 Mod"}
          </span>
          {audioCapsules.length > 0 && (
            <select
              className="settings-input recognition-capsule-select"
              aria-label="从公共胶囊池选择声纹 Mod"
              value={audioCapsuleSelection?.selected_capsule_id ?? ""}
              disabled={assigningCapsuleAccountId === trackingAccountId || !onAssignModCapsule}
              onChange={(event) => {
                const capsule = audioCapsules.find((candidate) => candidate.id === event.target.value);
                if (capsule) void onAssignModCapsule?.(trackingAccountId, capsule.id);
              }}
            >
              <option value="">公共胶囊池</option>
              {audioCapsules.map((capsule) => <option value={capsule.id} key={capsule.id}>{capsule.name}</option>)}
            </select>
          )}
          <Button size="sm" variant="secondary" className="shrink-0" onClick={onOpenModProcessing}>
            <Package size={12} />
            {audioModState?.ready ? "管理 Mod" : "前往加工"}
          </Button>
        </div>
      )}
    </div>

    {showModProcessing && trackingTarget.valid && (
      <div className="border-t border-border-default/50 pt-3">
        {audioModStateLoading && !audioModState ? (
          <div className="h-16 rounded-xl bg-surface-hover skeleton" aria-label="正在检查识别 Mod" />
        ) : audioModState?.ready && !audioSetupOpen ? (
          <div className={`flex items-start justify-between gap-3 rounded-xl px-3 py-2.5 ${
            audioModState.update_required ? "bg-warning/10" : "bg-success/10"
          }`}>
            <div className="flex min-w-0 items-start gap-2.5">
              {audioModState.update_required
                ? <AlertTriangle size={16} className="mt-0.5 shrink-0 text-warning" />
                : <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-success" />}
              <div className="min-w-0">
                <p className="text-xs font-semibold text-text-primary">
                  {audioModState.update_required ? "识别 Mod 可以更新" : "识别 Mod 已就绪"}
                </p>
                <p className="mt-0.5 text-2xs text-text-secondary">
                  {audioModState.update_required
                    ? audioModState.message
                    : `${audioModState.current_mod_name} · 启动参数已自动配置`}
                </p>
                {audioModState.restart_required && (
                  <p className="mt-1 text-2xs text-warning">当前游戏仍是旧配置，重启该账号后生效</p>
                )}
              </div>
            </div>
            <button
              type="button"
              onClick={onOpenAudioSetup}
              className="shrink-0 text-2xs font-medium text-text-secondary hover:text-text-primary"
            >
              {audioModState.update_required
                ? isEnglish ? "Update" : "更新"
                : featureCopy.manage}
            </button>
          </div>
        ) : audioSetupOpen ? (
          <div className="audio-mod-setup rounded-xl bg-surface-hover p-3">
            <div className="flex items-start gap-2.5">
              <Package size={16} className="mt-0.5 shrink-0 text-accent" />
              <div>
                <p className="text-xs font-semibold text-text-primary">
                  {isAudioModFeatureManagement
                    ? featureCopy.manageTitle
                    : audioModState?.update_required
                      ? isEnglish ? "Update recognition Mod" : "更新识别 Mod"
                      : isEnglish ? "Prepare D2RHub Mod" : "准备 D2RHub Mod"}
                </p>
                <p className="mt-0.5 text-2xs leading-relaxed text-text-secondary">
                  {isAudioModFeatureManagement
                    ? isEnglish
                      ? `D2RHub will rebuild and verify “${audioModState?.current_mod_name}” before replacing it in place. Its existing content and launch arguments stay intact.`
                      : `D2RHub 会先重建并校验“${audioModState?.current_mod_name}”，再原位替换；已有内容和启动参数保持不变。`
                    : audioModState?.update_required
                    ? `关闭该账号游戏后，D2RHub 会先完整生成并校验新版，再原位替换“${audioModState.current_mod_name}”；名称和启动参数不变。`
                    : "选择准备方式并命名新 Mod，其他内容由 D2RHub 自动完成。"}
                </p>
              </div>
            </div>

            {!isAudioModFeatureManagement && (
              <>
                <div className="mt-3 flex gap-2" role="radiogroup" aria-label="识别 Mod 类型">
                  <button
                    type="button"
                    role="radio"
                    aria-checked={audioSetupMode === "original"}
                    disabled={audioPreparing}
                    onClick={() => setAudioSetupMode("original")}
                    className={`audio-mod-choice flex-1 ${audioSetupMode === "original" ? "is-selected" : ""}`}
                  >
                    <span className="block text-xs font-semibold">我玩原版</span>
                    <span className="mt-0.5 block text-2xs text-text-muted">直接生成所选功能</span>
                  </button>
                  <button
                    type="button"
                    role="radio"
                    aria-checked={audioSetupMode === "existing"}
                    disabled={audioPreparing || !audioModState?.installed_mods.some((mod) => mod.source_eligible)}
                    onClick={() => setAudioSetupMode("existing")}
                    className={`audio-mod-choice flex-1 ${audioSetupMode === "existing" ? "is-selected" : ""}`}
                  >
                    <span className="block text-xs font-semibold">我使用 Mod</span>
                    <span className="mt-0.5 block text-2xs text-text-muted">保留原 Mod 功能</span>
                  </button>
                </div>

                {audioSetupMode === "existing" && (
                  <label className="mt-2 block">
                    <span className="sr-only">选择现有 Mod</span>
                    <select
                      value={audioSetupSource}
                      disabled={audioPreparing}
                      onChange={event => setAudioSetupSource(event.target.value)}
                      className="h-8 w-full rounded-lg border border-border-default bg-surface-card px-2.5 text-xs text-text-primary"
                    >
                      <option value="" disabled>请选择未经加工的原始 Mod</option>
                      {!audioModState?.installed_mods.some((mod) => mod.source_eligible) && (
                        <option value="">未找到可用的原始 Mod</option>
                      )}
                      {audioModState?.installed_mods
                        .filter((mod) => mod.source_eligible)
                        .map((mod) => <option key={mod.name} value={mod.name}>{mod.name}</option>)}
                    </select>
                    {audioModState?.update_required && audioModState.build_mode === "augment" && !audioSetupSource && (
                      <span className="mt-1 block text-2xs text-warning">
                        旧版基于其他 Mod 生成，但未能自动确定原始 Mod；请选择当时那个未经加工的 Mod，或明确改选“我玩原版”。
                      </span>
                    )}
                  </label>
                )}
              </>
            )}

            <fieldset className="mt-3 border-t border-border-default/50 pt-3">
              <legend className="sr-only">{featureCopy.title}</legend>
              <div className="mb-2">
                <p className="text-xs font-semibold text-text-primary">{featureCopy.title}</p>
                <p className="mt-0.5 max-w-[70ch] text-2xs leading-relaxed text-text-muted">
                  {featureCopy.description}
                </p>
              </div>
              <div className="grid grid-cols-2 gap-2 max-[620px]:grid-cols-1">
                <label
                  className={`audio-mod-choice flex min-h-[62px] cursor-pointer items-start gap-2.5 ${
                    includeAudioTelemetry || installedAudioTelemetry ? "is-selected" : ""
                  } ${installedAudioTelemetry ? "cursor-default" : ""}`}
                >
                  <input
                    type="checkbox"
                    checked={includeAudioTelemetry || installedAudioTelemetry}
                    disabled={audioPreparing || installedAudioTelemetry}
                    onChange={event => setIncludeAudioTelemetry(event.target.checked)}
                    className="mt-0.5 shrink-0 accent-[var(--accent)]"
                  />
                  <span className="min-w-0">
                    <span className="flex flex-wrap items-center gap-1.5 text-xs font-semibold text-text-primary">
                      {featureCopy.audioTitle}
                      {installedAudioTelemetry && (
                        <span className="rounded-md bg-success/10 px-1.5 py-0.5 text-[9px] font-semibold text-success">
                          {featureCopy.installed}
                        </span>
                      )}
                    </span>
                    <span className="mt-0.5 block text-2xs leading-relaxed text-text-muted">
                      {featureCopy.audioDetail}
                    </span>
                  </span>
                </label>
                <label
                  className={`audio-mod-choice flex min-h-[62px] cursor-pointer items-start gap-2.5 ${
                    includeRoomTools || installedRoomTools ? "is-selected" : ""
                  } ${installedRoomTools ? "cursor-default" : ""}`}
                >
                  <input
                    type="checkbox"
                    checked={includeRoomTools || installedRoomTools}
                    disabled={audioPreparing || installedRoomTools}
                    onChange={event => setIncludeRoomTools(event.target.checked)}
                    className="mt-0.5 shrink-0 accent-[var(--accent)]"
                  />
                  <span className="min-w-0">
                    <span className="flex flex-wrap items-center gap-1.5 text-xs font-semibold text-text-primary">
                      {featureCopy.roomTitle}
                      {installedRoomTools && (
                        <span className="rounded-md bg-success/10 px-1.5 py-0.5 text-[9px] font-semibold text-success">
                          {featureCopy.installed}
                        </span>
                      )}
                    </span>
                    <span className="mt-0.5 block text-2xs leading-relaxed text-text-muted">
                      {featureCopy.roomDetail}
                    </span>
                  </span>
                </label>
              </div>
            </fieldset>

            {isAudioModUpgrade ? (
              <div className="mt-3 rounded-lg border border-border-default bg-surface-card px-2.5 py-2">
                <span className="block text-2xs font-semibold text-text-muted">
                  {isEnglish ? "Update in place · Mod name unchanged" : "原位更新，Mod 名称保持不变"}
                </span>
                <span className="mt-0.5 block truncate font-mono text-xs font-semibold text-text-primary">
                  {audioModState?.current_mod_name}
                </span>
              </div>
            ) : (
              <>
                <label className="mt-3 block" htmlFor="audio-mod-name">
                  <span className="flex items-center justify-between gap-3 text-2xs font-semibold text-text-secondary">
                    <span>新 Mod 名称</span>
                    <span className="font-normal text-text-muted">必填</span>
                  </span>
                  <input
                    id="audio-mod-name"
                    type="text"
                    value={audioSetupName}
                    maxLength={AUDIO_MOD_NAME_MAX_LENGTH}
                    disabled={audioPreparing}
                    autoFocus
                    autoCapitalize="off"
                    autoCorrect="off"
                    spellCheck={false}
                    aria-invalid={!!audioSetupNameError}
                    aria-describedby="audio-mod-name-help"
                    placeholder="例如：MyAudioMod"
                    onChange={event => setAudioSetupName(event.target.value)}
                    className={`mt-1 h-8 w-full rounded-lg border bg-surface-card px-2.5 text-xs text-text-primary outline-none transition-colors placeholder:text-text-muted focus:border-accent disabled:cursor-not-allowed disabled:opacity-60 ${
                      showAudioSetupNameError ? "border-danger" : "border-border-default"
                    }`}
                  />
                </label>
                <p
                  id="audio-mod-name-help"
                  className={`mt-1 text-2xs leading-relaxed ${
                    audioSetupNameError ? "font-medium text-warning" : "text-text-muted"
                  }`}
                >
                  {audioSetupNameError
                    ? audioSetupNameError
                    : "仅可使用英文字母、数字、短横线和下划线。"}
                </p>
              </>
            )}
            {audioPreparing && audioPrepareProgress && (
              <div className="mt-3" aria-live="polite">
                <div className="mb-1.5 flex items-center justify-between gap-3 text-2xs">
                  <span className="truncate text-text-secondary">{audioPrepareProgress.message}</span>
                  <span className="shrink-0 font-mono text-text-muted">{Math.round(audioPrepareProgress.percent)}%</span>
                </div>
                <div className="h-1.5 overflow-hidden rounded-full bg-surface-active">
                  <div
                    className="h-full rounded-full bg-accent transition-[width] duration-200 ease-out"
                    style={{ width: `${Math.max(2, audioPrepareProgress.percent)}%` }}
                  />
                </div>
              </div>
            )}

            {!!audioPrepareBlockedReason && !audioPreparing && (
              <div
                id="audio-prepare-blocked-reason"
                className="mt-3 flex items-start gap-2 rounded-lg border border-warning/25 bg-warning/10 px-2.5 py-2 text-2xs leading-relaxed text-text-secondary"
                role="status"
              >
                <AlertTriangle size={13} className="mt-0.5 shrink-0 text-warning" />
                <span>
                  <strong className="text-warning">
                    {isEnglish ? "Cannot continue: " : "还不能开始："}
                  </strong>
                  {audioPrepareBlockedReason}{isEnglish ? "." : "。"}
                </span>
              </div>
            )}

            <div className="mt-3 flex gap-2">
              <Button
                variant="secondary"
                size="md"
                disabled={audioPreparing}
                onClick={onCloseAudioSetup}
                className="shrink-0"
              >
                {featureCopy.cancel}
              </Button>
              <Button
                variant="primary"
                size="md"
                loading={audioPreparing}
                disabled={!!audioPrepareBlockedReason}
                aria-describedby={audioPrepareBlockedReason ? "audio-prepare-blocked-reason" : undefined}
                onClick={handlePrepareAudioMod}
                className="min-w-0 flex-1"
              >
                {audioPreparing
                  ? isEnglish ? "Preparing — keep D2RHub open" : "正在准备，请勿关闭软件"
                  : audioPrepareBlockedReason
                    ? isEnglish
                      ? "Complete the choices above"
                      : !isAudioModUpgrade && audioSetupNameError === "请输入新 Mod 名称"
                        ? "填写 Mod 名称后即可准备"
                        : "完成上方配置后即可准备"
                    : isAudioModFeatureManagement
                      ? isEnglish ? "Add selected features" : "增补所选功能"
                      : audioModState?.update_required
                        ? isEnglish ? "Update safely in place" : "同名更新并替换旧版"
                        : isEnglish ? "Prepare and apply" : "一键准备并应用"}
              </Button>
            </div>
            <p className="mt-2 text-center text-2xs leading-relaxed text-text-muted">
              {isAudioModFeatureManagement
                ? isEnglish
                  ? "The current verified Mod is used as the source; existing and future feature groups are carried forward."
                  : "以当前已校验 Mod 为来源重建；已有功能与未来版本功能都会一并保留。"
                : isAudioModUpgrade
                ? "不会拿旧版识别 Mod 再加工；会从原版或所选原始 Mod 重建，校验成功后才替换旧目录。"
                : "不修改源 Mod；账号参数固定配置为 -mod 名称 -txt -assettestmode 1。"}
            </p>
          </div>
        ) : (
          <button
            type="button"
            onClick={onOpenAudioSetup}
            className="flex w-full items-center justify-between gap-3 rounded-xl bg-surface-hover px-3 py-2.5 text-left hover:bg-surface-active"
          >
            <span className="flex min-w-0 items-start gap-2.5">
              <AlertTriangle size={15} className="mt-0.5 shrink-0 text-warning" />
              <span>
                <span className="block text-xs font-semibold text-text-primary">需要先准备识别 Mod</span>
                <span className="mt-0.5 block text-2xs text-text-secondary">{audioModState?.message ?? "点击开始，约一分钟完成"}</span>
              </span>
            </span>
            <span className="shrink-0 text-2xs font-medium text-accent">开始</span>
          </button>
        )}
      </div>
    )}

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
          <span className="font-mono text-text-primary">{audioStatus?.scene_heartbeats ?? 0}</span>
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

    {config.rune_audio_enabled && trackingTarget.valid && (
      <details className="group border-t border-border-default/50 pt-3">
        <summary className="flex cursor-pointer list-none items-center justify-between text-xs font-medium text-text-secondary">
          诊断工具
          <ChevronDown size={14} className="transition-transform duration-200 group-open:rotate-180" />
        </summary>
        <div className="mt-3 space-y-3">
          <div className="flex items-center justify-between gap-3">
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
              className="h-8 w-24 rounded-lg border border-border-default bg-surface-hover px-2.5 text-xs text-text-primary"
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
  </div>

  <div className="recognition-side">
    <details className="spatial-panel group overflow-hidden recognition-filters" open>
      <summary className="flex cursor-pointer list-none items-center justify-between gap-3 p-4">
        <span>
          <span className="block text-sm font-bold text-text-primary">识别过滤器</span>
          <span className="mt-0.5 block text-2xs text-text-muted">选择哪些已识别掉落写入统计；点击可收起</span>
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
                <input
                  type="range"
                  min={1}
                  max={filter.max}
                  step={1}
                  value={filter.value}
                  disabled={!enabled}
                  aria-label={`${filter.label}最低记录等级`}
                  onChange={event => filter.onChange(Number(event.target.value))}
                  className="tracking-filter-range mt-2 w-full"
                  style={{
                    "--range-progress": `${((filter.value - 1) / Math.max(1, filter.max - 1)) * 100}%`,
                  } as CSSProperties}
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
            <div className="grid grid-cols-3 gap-1.5">
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
                        <span className="mt-0.5 block truncate text-[9px] text-text-muted">{item.detail}</span>
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
            所有掉落仅在已确认的野外或地下城场景中记录；主城、主界面和尚未识别地点时一律忽略。修改后点击左侧“应用并重启识别”。
          </p>
        </div>
      </div>
    </details>

    <div className="spatial-panel recognition-notes">
      <div>
        <span className="text-xs font-bold text-text-primary block mb-1">识别说明</span>
        <p className="text-2xs text-text-muted">
          D2RHub 只捕获所选账号的游戏声音，不读取游戏内存，也不会向游戏注入代码。
        </p>
      </div>
      <p className="text-2xs text-text-secondary">
        过滤器只决定 D2RHub 是否将已接收事件写入统计；Mod 始终包含完整识别声纹。
      </p>
      <p className="text-2xs text-warning">
        声纹按基础物品代码识别；同一代码的暗金、套装或词缀无法仅凭音频区分。
      </p>
      <div className="border-t border-border-default/50 pt-3">
        <Button
          size="sm"
          onClick={async () => {
            try {
              await invokeCommand("open_stats_page");
            } catch (e) {
              showToast("error", `打开统计界面失败: ${e}`);
            }
          }}
        >
          <Play size={10} className="text-success fill-success" />
          打开掉落统计图表
        </Button>
      </div>
    </div>
  </div>
</div>
  );
}
