import {
  AlertTriangle,
  Check,
  CheckCircle2,
  Layers3,
  PackageOpen,
  RefreshCw,
} from "lucide-react";
import type { Dispatch, SetStateAction } from "react";
import { Button } from "../../../components/ui/Button";
import type { AccountMeta, AudioModSetupState, GlobalConfig, ModCapsulePool } from "../../../store/types";
import { AUDIO_MOD_NAME_MAX_LENGTH } from "../../../utils/audioModName";
import { validateTrackingTarget } from "../../../utils/trackingTarget";
import {
  AUDIO_TELEMETRY_FEATURE_ID,
  IN_GAME_ROOM_TOOLS_FEATURE_ID,
  type AudioModPrepareProgress,
} from "../audioModuleModel";

type TrackingTarget = ReturnType<typeof validateTrackingTarget>;
type AudioSetupMode = "original" | "existing";
export type ModProcessingPurpose = "recognition" | "room-tools" | "manage";

interface ModProcessingPanelProps {
  config: GlobalConfig;
  initializedAccounts: AccountMeta[];
  trackingTarget: TrackingTarget;
  audioModState: AudioModSetupState | null;
  audioModStateLoading: boolean;
  audioModScannedAt: number | null;
  modCapsulePool?: ModCapsulePool | null;
  modCapsulePoolLoading?: boolean;
  modCapsulePoolError?: string | null;
  purpose: ModProcessingPurpose;
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
  audioPrepareBlockedReason: string;
  onTargetChange: (accountId: string) => Promise<void>;
  onPrepare: () => Promise<void>;
  onRefresh: () => Promise<void>;
  onBackToRecognition: () => void;
}

export function ModProcessingPanel({
  config,
  initializedAccounts,
  trackingTarget,
  audioModState,
  audioModStateLoading,
  audioModScannedAt,
  modCapsulePool = null,
  modCapsulePoolLoading = false,
  modCapsulePoolError = null,
  purpose,
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
  audioPrepareBlockedReason,
  onTargetChange,
  onPrepare,
  onRefresh,
  onBackToRecognition,
}: ModProcessingPanelProps) {
  const isEnglish = config.app_language === "en-US";
  const selectedSource = audioModState?.installed_mods.find((mod) => mod.name === audioSetupSource);
  const inheritedFeatureGroups = isAudioModUpgrade
    ? audioModState?.feature_groups ?? []
    : audioSetupMode === "existing"
      ? selectedSource?.feature_groups ?? []
      : [];
  const audioRequired = purpose === "recognition";
  const roomToolsRequired = purpose === "room-tools";
  const audioInherited = inheritedFeatureGroups.includes(AUDIO_TELEMETRY_FEATURE_ID);
  const roomToolsInherited = inheritedFeatureGroups.includes(IN_GAME_ROOM_TOOLS_FEATURE_ID);
  const audioSelected = audioRequired || audioInherited || includeAudioTelemetry;
  const roomToolsSelected = roomToolsRequired || roomToolsInherited || includeRoomTools;
  const sourceMods = audioModState?.installed_mods.filter((mod) => mod.source_eligible) ?? [];
  const scannedLabel = audioModScannedAt
    ? new Date(audioModScannedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })
    : null;
  const readyCapsules = modCapsulePool?.capsules.filter((capsule) => capsule.ready) ?? [];

  return (
    <div className="mod-processing-panel">
      <header className="mod-processing-header">
        <div>
          <h2>{isEnglish ? "Mod Processing" : "Mod 加工"}</h2>
          <p>
            {isEnglish
              ? "Choose a source Mod, preserve everything it already provides, then add the capabilities you need."
              : "选择源 Mod，完整保留它已有的功能，再增补这次需要的 D2RHub 模块。"}
          </p>
        </div>
        <Button
          size="sm"
          variant="ghost"
          loading={audioModStateLoading}
          disabled={!trackingTarget.valid || audioPreparing}
          title={scannedLabel ? `上次扫描 ${scannedLabel}` : "重新扫描 Mod 目录"}
          onClick={() => void onRefresh()}
        >
          <RefreshCw size={13} />
          {isEnglish ? "Rescan" : "重新扫描"}
        </Button>
      </header>

      <section className="spatial-panel mod-processing-section mod-processing-target">
        <div className="mod-processing-section-heading">
          <div>
            <h3>{isEnglish ? "Target account" : "加工目标"}</h3>
            <p>{isEnglish ? "The selected account receives the generated launch arguments." : "加工完成后会自动写入这个账号的启动参数。"}</p>
          </div>
        </div>
        <select
          className="settings-input mod-processing-account-select"
          value={trackingTarget.valid ? trackingTarget.account.id : ""}
          disabled={initializedAccounts.length === 0 || audioPreparing}
          onChange={(event) => void onTargetChange(event.target.value)}
        >
          <option value="" disabled>{initializedAccounts.length ? "请选择账号" : "暂无已初始化账号"}</option>
          {initializedAccounts.map((account) => (
            <option key={account.id} value={account.id}>{account.display_name || account.id}</option>
          ))}
        </select>
        <div className="mod-capsule-pool-summary">
          <div>
            <Layers3 size={14} aria-hidden="true" />
            <span>
              <strong>{isEnglish ? "Shared processed-Mod pool" : "公共加工 Mod 胶囊池"}</strong>
              <small>{modCapsulePoolLoading
                ? (isEnglish ? "Scanning installed credentials…" : "正在扫描已安装凭证…")
                : modCapsulePoolError
                  ? (isEnglish ? "Pool unavailable" : "胶囊池暂不可用")
                  : isEnglish
                    ? `${readyCapsules.length} verified capsules; old account arguments are matched automatically`
                    : `${readyCapsules.length} 个已验证胶囊；旧账号启动参数会自动匹配`}</small>
            </span>
          </div>
          {!modCapsulePoolLoading && readyCapsules.length > 0 && (
            <div className="mod-capsule-pool-list">
              {readyCapsules.map((capsule) => (
                <span key={capsule.id} title={`${capsule.edition} · ${capsule.feature_groups.join(", ")}`}>
                  <b>{capsule.name}</b>
                  <em>{capsule.assigned_account_ids.length}</em>
                </span>
              ))}
            </div>
          )}
        </div>
      </section>

      {!trackingTarget.valid ? (
        <div className="room-automation-state room-automation-state-block mod-processing-main-state" data-tone="danger" role="status">
          <AlertTriangle size={16} />
          <div>
            <strong>{isEnglish ? "Select an initialized account first" : "请先选择一个已初始化账号"}</strong>
            <p>{isEnglish ? "D2RHub needs the account edition and Mod directory before it can inspect available modules." : "D2RHub 需要先确定账号版本与 Mod 目录，才能读取可用模块。"}</p>
          </div>
        </div>
      ) : audioModStateLoading && !audioModState ? (
        <div className="mod-processing-main-state space-y-2" aria-label="正在扫描 Mod">
          <div className="h-24 skeleton rounded-xl" />
          <div className="h-40 skeleton rounded-xl" />
        </div>
      ) : (
        <>
          {!isAudioModFeatureManagement && (
            <section className="spatial-panel mod-processing-section mod-processing-source">
              <div className="mod-processing-section-heading">
                <div>
                  <h3>{isEnglish ? "Choose the source" : "选择源内容"}</h3>
                  <p>{isEnglish ? "An existing Mod stays unchanged; D2RHub builds a separate verified result." : "原 Mod 不会被修改；D2RHub 会生成并校验一个独立结果。"}</p>
                </div>
              </div>
              <div className="mod-processing-source-options" role="radiogroup" aria-label="Mod 来源">
                <button
                  type="button"
                  role="radio"
                  aria-checked={audioSetupMode === "original"}
                  className={`audio-mod-choice ${audioSetupMode === "original" ? "is-selected" : ""}`}
                  disabled={audioPreparing}
                  onClick={() => setAudioSetupMode("original")}
                >
                  <strong>{isEnglish ? "Original game" : "原版游戏"}</strong>
                  <span>{isEnglish ? "Start with D2RHub modules only" : "只生成本次所选模块"}</span>
                </button>
                <button
                  type="button"
                  role="radio"
                  aria-checked={audioSetupMode === "existing"}
                  className={`audio-mod-choice ${audioSetupMode === "existing" ? "is-selected" : ""}`}
                  disabled={audioPreparing || sourceMods.length === 0}
                  onClick={() => setAudioSetupMode("existing")}
                >
                  <strong>{isEnglish ? "Existing Mod" : "已有 Mod"}</strong>
                  <span>{isEnglish ? "Keep every detected feature" : "继承并锁定已有功能"}</span>
                </button>
              </div>
              {audioSetupMode === "existing" && (
                <label className="mod-processing-source-select">
                  <span>{isEnglish ? "Source Mod" : "源 Mod"}</span>
                  <select
                    className="settings-input"
                    value={audioSetupSource}
                    disabled={audioPreparing}
                    onChange={(event) => setAudioSetupSource(event.target.value)}
                  >
                    <option value="" disabled>{isEnglish ? "Select a Mod" : "请选择 Mod"}</option>
                    {sourceMods.map((mod) => <option key={mod.name} value={mod.name}>{mod.name}</option>)}
                  </select>
                </label>
              )}
            </section>
          )}

          <section className="spatial-panel mod-processing-section mod-processing-capabilities">
            <div className="mod-processing-section-heading">
              <div>
                <h3>{isEnglish ? "Feature modules" : "功能模块"}</h3>
                <p>{isEnglish ? "Inherited modules are locked. Recognition is also locked when this flow was opened to enable it." : "源 Mod 已有模块会锁定保留；为开启识别进入此页时，声纹识别同样为必选。"}</p>
              </div>
            </div>
            <div className="mod-processing-features">
              <FeatureChoice
                title={isEnglish ? "Audio recognition" : "声纹识别"}
                detail={isEnglish ? "Scenes, drops, Terror Zones, and run statistics" : "场景、掉落、恐怖区域与刷图统计"}
                checked={audioSelected}
                locked={audioRequired || audioInherited}
                lockLabel={audioInherited ? (isEnglish ? "Included in source" : "源 Mod 已有") : (isEnglish ? "Required for this setup" : "本次目标 · 必选")}
                disabled={audioPreparing}
                onChange={setIncludeAudioTelemetry}
              />
              <FeatureChoice
                title={isEnglish ? "In-game room tools" : "局内房间工具"}
                detail={isEnglish ? "Create, recreate, and join rooms from the automation workflow" : "为自动跟房提供创建、重开与加入房间能力"}
                checked={roomToolsSelected}
                locked={roomToolsRequired || roomToolsInherited}
                lockLabel={roomToolsInherited
                  ? (isEnglish ? "Included in source" : "源 Mod 已有")
                  : (isEnglish ? "Required for room automation" : "自动跟房必选")}
                disabled={audioPreparing}
                onChange={setIncludeRoomTools}
              />
            </div>
          </section>

          <section className="spatial-panel mod-processing-section mod-processing-output">
            <div className="mod-processing-section-heading">
              <div>
                <h3>{isEnglish ? "Output" : "输出与应用"}</h3>
                <p>{isAudioModUpgrade ? (isEnglish ? "The current Mod is safely replaced in place after verification." : "校验成功后原位更新，名称和账号启动参数保持不变。") : (isEnglish ? "Name the generated Mod, then build and apply it in one step." : "为加工结果命名，然后一次完成生成、校验与应用。")}</p>
              </div>
            </div>
            {isAudioModUpgrade ? (
              <div className="mod-processing-existing-output">
                <CheckCircle2 size={15} />
                <span>{audioModState?.current_mod_name}</span>
              </div>
            ) : (
              <label className="mod-processing-name" htmlFor="processed-mod-name">
                <span>{isEnglish ? "New Mod name" : "新 Mod 名称"}</span>
                <input
                  id="processed-mod-name"
                  className="settings-input"
                  value={audioSetupName}
                  maxLength={AUDIO_MOD_NAME_MAX_LENGTH}
                  disabled={audioPreparing}
                  autoCapitalize="off"
                  autoCorrect="off"
                  spellCheck={false}
                  aria-invalid={!!audioSetupNameError}
                  placeholder="MyD2RHubMod"
                  onChange={(event) => setAudioSetupName(event.target.value)}
                />
                {showAudioSetupNameError && <small>{audioSetupNameError}</small>}
              </label>
            )}
            {audioPreparing && audioPrepareProgress && (
              <div className="mod-processing-progress" aria-live="polite">
                <div><span>{audioPrepareProgress.message}</span><strong>{Math.round(audioPrepareProgress.percent)}%</strong></div>
                <div><span style={{ width: `${Math.max(2, audioPrepareProgress.percent)}%` }} /></div>
              </div>
            )}
            {!!audioPrepareBlockedReason && !audioPreparing && (
              <p className="mod-processing-blocked" role="status">
                <AlertTriangle size={13} />
                {audioPrepareBlockedReason}
              </p>
            )}
            <div className="mod-processing-actions">
              {purpose === "recognition" && (
                <Button variant="ghost" size="md" disabled={audioPreparing} onClick={onBackToRecognition}>
                  {isEnglish ? "Back" : "返回识别设置"}
                </Button>
              )}
              <Button
                variant="primary"
                size="md"
                loading={audioPreparing}
                disabled={!!audioPrepareBlockedReason}
                onClick={() => void onPrepare()}
              >
                <PackageOpen size={14} />
                {audioPreparing
                  ? (isEnglish ? "Processing…" : "正在加工…")
                  : isAudioModFeatureManagement
                    ? (isEnglish ? "Add selected modules" : "增补所选模块")
                    : isAudioModUpgrade
                      ? (isEnglish ? "Verify and update" : "校验并更新")
                      : (isEnglish ? "Process and apply" : "开始加工并应用")}
              </Button>
            </div>
          </section>
        </>
      )}
    </div>
  );
}

function FeatureChoice({
  title,
  detail,
  checked,
  locked,
  lockLabel,
  disabled,
  onChange,
}: {
  title: string;
  detail: string;
  checked: boolean;
  locked: boolean;
  lockLabel: string;
  disabled: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="audio-mod-choice mod-processing-feature" data-locked={locked ? "true" : undefined}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled || locked}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span>
        <strong>{title}</strong>
        <small>{detail}</small>
      </span>
      {locked && <em><Check size={11} />{lockLabel}</em>}
    </label>
  );
}
