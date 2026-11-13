import { useId, type KeyboardEvent } from "react";
import { AlertTriangle, ArrowLeft, CheckCircle2, Layers3, PackageOpen, PackagePlus, RefreshCw } from "lucide-react";
import { ProgressBar } from "../../../components/ui/ProgressBar";
import { Button } from "../../../components/ui/Button";
import { RoomToolProcessingOptions } from "./RoomToolProcessingOptions";
import { ModProcessorStatus } from "./ModProcessorStatus";
import { ModPageHeader } from "./ModPageHeader";
import { ModFeatureChoice as FeatureChoice } from "./ModFeatureChoice";
import { capsuleBaseModLabel, capsuleFeatureLabels } from "../../modCapsules/model";
import { AUDIO_MOD_NAME_MAX_LENGTH } from "../../../utils/audioModName";
import { validateTrackingTarget } from "../../../utils/trackingTarget";
import { AUDIO_TELEMETRY_FEATURE_ID, IN_GAME_ROOM_TOOLS_FEATURE_ID, ESC_NEXT_GAME_FEATURE_ID, AUTO_EXIT_ON_DEATH_FEATURE_ID } from "../../mods/featureContract";
import type { ModWorkflowController } from "../../mods/workflow/useModWorkflow";

function navigateChoices(event: KeyboardEvent<HTMLDivElement>) {
  if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
  const choices = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button[role="radio"]:not(:disabled)'));
  const current = choices.indexOf(event.target as HTMLButtonElement);
  if (current < 0 || !choices.length) return;
  event.preventDefault();
  const index = event.key === "Home" ? 0 : event.key === "End" ? choices.length - 1
    : (current + (["ArrowLeft", "ArrowUp"].includes(event.key) ? -1 : 1) + choices.length) % choices.length;
  choices[index].focus();
  choices[index].click();
}

/** The form renders one explicit workflow draft; it does not own navigation or scanning. */
export function ModProcessingPanel({ workflow }: { workflow: ModWorkflowController }) {
  const blockedReasonId = useId();
  const { draft, inspection, analysis, actions, en: isEnglish, minimalMode } = workflow;
  if (!draft || !analysis) return null;
  const initializedAccounts = workflow.accounts;
  const trackingTarget = validateTrackingTarget(draft.accountId, initializedAccounts);
  const targetReady = !!draft.installationOnly || trackingTarget.valid;
  const inspectionState = inspection.state;
  const inspecting = inspection.loading;
  const preparing = workflow.busy;
  const prepareProgress = workflow.progress;
  const blockedReason = workflow.blockedReason;
  const operationKind = draft.recipe.kind;
  const targetModName = draft.recipe.kind === "augment" ? draft.recipe.modName : "";
  const sourceMode = draft.recipe.kind === "create" && draft.recipe.source !== null ? "existing" : "original";
  const sourceName = draft.recipe.kind === "create" ? draft.recipe.source ?? "" : "";
  const replaceSource = draft.recipe.kind === "create" && !!draft.recipe.replaceSource;
  const outputName = draft.recipe.kind === "create" ? (replaceSource ? sourceName : draft.recipe.name) : "";
  const isAugment = analysis.augment;
  const isAddingFeatures = analysis.augment && !analysis.updating && !analysis.roomOptionsChanged;
  const outputNameError = analysis.nameError;
  const showNameError = !!outputName && !!analysis.nameError && !workflow.prepared;
  const inheritedFeatureGroups = analysis.inherited;
  const audioRequired = draft.origin === "recognition";
  const roomToolsRequired = draft.origin === "room-automation";
  const audioInherited = inheritedFeatureGroups.includes(AUDIO_TELEMETRY_FEATURE_ID);
  const roomToolsInherited = inheritedFeatureGroups.includes(IN_GAME_ROOM_TOOLS_FEATURE_ID);
  const autoExitOnDeathInherited = inheritedFeatureGroups.includes(AUTO_EXIT_ON_DEATH_FEATURE_ID);
  const audioSelected = analysis.selection.includeAudioTelemetry;
  const roomToolsSelected = analysis.selection.includeRoomTools;
  const autoExitOnDeathSelected = analysis.selection.includeAutoExitOnDeath;
  const includeEscNextGame = analysis.selection.includeEscNextGame;
  const sourceMods = inspectionState?.installed_mods.filter(mod => mod.source_eligible) ?? [];
  const rebuildSources = sourceMods.filter(mod => !mod.feature_groups.length && !mod.update_required && !mod.requires_unpack);
  const readyCapsules = workflow.readyCapsules;
  const catalogLoading = workflow.catalog.loading;
  const catalogError = workflow.catalog.error;
  const scannedLabel = inspection.scannedAt ? new Date(inspection.scannedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : null;
  const backLabel = draft.origin === "recognition" ? (isEnglish ? "Back to recognition" : "返回识别设置")
    : draft.origin === "room-automation" ? (isEnglish ? "Back to room automation" : "返回自动跟房")
      : (isEnglish ? "Back to Mod library" : "返回 Mod 管理");
  const selectSourceMod = (source: string) => actions.changeRecipe({ kind: "create", source, name: outputName, replaceSource });
  const selectProcessedMod = (modName: string) => actions.changeRecipe({ kind: "augment", modName });
  const selectNewMod = () => actions.changeRecipe({ kind: "create", source: null, name: "" });
  const setOutputName = (name: string) => { if (draft.recipe.kind === "create") actions.changeRecipe({ ...draft.recipe, name }); };
  return (
    <div className="mod-workspace mod-processing-panel" data-processing-mode={operationKind}>
      <ModPageHeader title={isEnglish ? "Mod Processing" : "Mod 加工"}
        description={isEnglish
              ? "Add or update modules in an existing Mod, or process a new copy."
              : "为现有 Mod 增补或更新模块，也可以加工新副本。"}>
          <Button size="sm" variant="ghost" disabled={preparing} onClick={actions.back}><ArrowLeft size={13} aria-hidden="true" />{backLabel}</Button>
          <Button
            size="sm"
            variant="ghost"
            loading={inspecting}
            disabled={!targetReady || preparing}
            title={scannedLabel ? `${isEnglish ? "Last scan" : "上次扫描"} ${scannedLabel}` : (isEnglish ? "Rescan Mod directory" : "重新扫描 Mod 目录")}
            onClick={() => void actions.refresh()}
          >
            <RefreshCw size={13} />
            {isEnglish ? "Rescan" : "重新扫描"}
          </Button>
      </ModPageHeader>

      <div className="mod-processing-processor"><ModProcessorStatus edition={draft.edition} en={isEnglish}
        onReady={actions.setProcessorReady} /></div>
      {workflow.error && <p className="mod-catalog-error" role="alert">{workflow.error}</p>}
      {inspection.error && <p className="mod-catalog-error" role="alert">{inspection.error}</p>}
      <section className="mod-processing-section mod-processing-target">
        <div className="mod-processing-section-heading">
          <div>
            <h3>{draft.installationOnly ? (isEnglish ? "Game installation" : "游戏安装目录") : (isEnglish ? "Target account" : "加工目标")}</h3>
            <span className="micro-meta">{draft.edition === "CN" ? (isEnglish ? "China edition" : "国服") : (isEnglish ? "Global edition" : "国际服")}</span>
            <p>{draft.installationOnly ? (isEnglish ? "Copy the generated launch arguments to your game launcher." : "无需账号，完成后将生成的启动参数配置到游戏启动器。") : isEnglish ? "The selected account receives the generated launch arguments." : "加工完成后会自动写入这个账号的启动参数。"}</p>
          </div>
        </div>
        <div className="mod-section-body">
          {!draft.installationOnly && (
          <select
            className="settings-input mod-processing-account-select"
            aria-label={isEnglish ? "Target account" : "加工目标账号"}
            value={trackingTarget.valid ? trackingTarget.account.id : ""}
            disabled={initializedAccounts.length === 0 || preparing}
            onChange={(event) => void actions.chooseTarget(event.target.value)}
          >
            <option value="" disabled>{initializedAccounts.length ? (isEnglish ? "Select an account" : "请选择账号") : (isEnglish ? "No initialized accounts" : "暂无已初始化账号")}</option>
            {initializedAccounts.map((account) => (
              <option key={account.id} value={account.id}>{account.display_name || account.id}</option>
            ))}
          </select>
          )}
          <div className="mod-processing-rebuild">
            {draft.recipe.kind === "augment" && <label className="mod-processing-rebuild-toggle"><input type="checkbox" checked={!!draft.recipe.rebuild || !!analysis.selected?.update_required}
              disabled={preparing || !!analysis.selected?.update_required}
              onChange={event => { if (draft.recipe.kind === "augment") actions.changeRecipe({ ...draft.recipe, rebuild: event.target.checked }); }} />
              {isEnglish ? "Rebuild from original source and replace the same name" : "从原始源 Mod 重做并替换同名成品"}</label>}
            {draft.recipe.kind === "augment" && analysis.updating && <label className="mod-processing-rebuild-source">
              {isEnglish ? "Original source for rebuild" : "重做所用的原始源 Mod"}
              <select className="settings-input" disabled={preparing} value={draft.recipe.sourceOverride ?? analysis.selected?.source_mod_name ?? ""}
                onChange={event => { if (draft.recipe.kind === "augment") actions.changeRecipe({ ...draft.recipe, sourceOverride: event.target.value || undefined }); }}>
                <option value="">{isEnglish ? "Saved original / vanilla recipe" : "使用旧清单记录的来源／原版配方"}</option>
                {analysis.selected?.source_mod_name && !rebuildSources.some(mod => mod.name === analysis.selected?.source_mod_name) && <option value={analysis.selected.source_mod_name}>{analysis.selected.source_mod_name} — {isEnglish ? "missing" : "缺失，请重新指定"}</option>}
                {rebuildSources.map(mod => <option key={mod.name} value={mod.name}>{mod.name}</option>)}
              </select>
            </label>}
          </div>
          <div className="mod-capsule-pool-summary">
            <div>
              <Layers3 size={14} aria-hidden="true" />
              <span>
                <strong>{isEnglish ? "Processing options" : "选择加工方式"}</strong>
                <small>{catalogLoading
                  ? (isEnglish ? "Scanning installed Mods…" : "正在扫描已安装 Mod…")
                  : catalogError
                    ? (isEnglish ? "Processed Mods are temporarily unavailable" : "暂时无法读取已加工 Mod")
                    : isEnglish
                      ? `${readyCapsules.length} processed Mods can be augmented, or you can process a new Mod`
                      : `${readyCapsules.length} 个已加工 Mod 可继续增补，也可以加工新 Mod`}</small>
              </span>
            </div>
            {!catalogLoading && (
              <div className="mod-capsule-pool-list">
                <button
                  type="button"
                  className="mod-processing-new-capsule"
                  aria-pressed={operationKind === "create"}
                  title={isEnglish ? "Create a separate processed Mod" : "加工并生成一个新的独立 Mod"}
                  disabled={preparing}
                  onClick={selectNewMod}
                >
                  <PackagePlus size={14} aria-hidden="true" />
                  <b>{isEnglish ? "Process a new Mod" : "加工新 Mod"}</b>
                  <span>{isEnglish ? "Choose its source below" : "在下方选择来源"}</span>
                </button>
                {readyCapsules.map((capsule) => (
                  <button
                    type="button"
                    key={capsule.id}
                    aria-pressed={operationKind === "augment"
                      && targetModName.toLocaleLowerCase() === capsule.name.toLocaleLowerCase()}
                    title={`${capsule.edition} · ${capsuleFeatureLabels(capsule, isEnglish, minimalMode).join(isEnglish ? ", " : "、")}`}
                    disabled={preparing}
                    onClick={() => selectProcessedMod(capsule.name)}
                  >
                    <b>{capsule.name}</b>
                    <span className="mod-capsule-pool-features">
                      <em data-kind="base">{capsuleBaseModLabel(capsule, isEnglish)}</em>
                      {capsuleFeatureLabels(capsule, isEnglish, minimalMode).length
                        ? capsuleFeatureLabels(capsule, isEnglish, minimalMode).map((label) => <em data-kind="feature" key={label}>{label}</em>)
                        : !minimalMode && <em data-kind="pending">{isEnglish ? "Update required" : "待更新"}</em>}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      </section>

      {!targetReady ? (
        <div className="room-automation-state room-automation-state-block mod-processing-main-state" data-tone="danger" role="status">
          <AlertTriangle size={16} />
          <div>
            <strong>{isEnglish ? "Select an initialized account first" : "请先选择一个已初始化账号"}</strong>
            <p>{isEnglish ? "D2RHub needs the account edition and Mod directory before it can inspect available modules." : "D2RHub 需要先确定账号版本与 Mod 目录，才能读取可用模块。"}</p>
          </div>
        </div>
      ) : inspecting && !inspectionState ? (
        <div className="mod-processing-main-state space-y-2" aria-label="正在扫描 Mod">
          <div className="h-24 skeleton rounded-xl" />
          <div className="h-40 skeleton rounded-xl" />
        </div>
      ) : (
        <>
          {operationKind === "create" && (
            <section className="mod-processing-section mod-processing-source">
              <div className="mod-processing-section-heading">
                <div>
                  <h3>{isEnglish ? "Choose the source Mod" : "选择源 Mod"}</h3>
                  <p>{isEnglish ? "Choose a separate output name, or keep the source name and retain the original folder as a backup." : "可生成独立副本，也可沿用源 Mod 名称，原目录加后缀保留为备份。"}</p>
                </div>
              </div>
              <div className="mod-section-body">
                <div className="mod-processing-source-options" role="radiogroup" aria-label={isEnglish ? "Mod source" : "Mod 来源"} onKeyDown={navigateChoices}>
                  <button
                    type="button"
                    role="radio"
                    aria-checked={sourceMode === "original"}
                    tabIndex={sourceMode === "original" || sourceMods.length === 0 ? 0 : -1}
                    className="mod-source-choice"
                    disabled={preparing}
                    onClick={() => actions.changeRecipe({ kind: "create", source: null, name: outputName })}
                  >
                    <strong>{isEnglish ? "Original game" : "原版游戏"}</strong>
                    <span>{isEnglish ? "Start with D2RHub modules only" : "只生成本次所选模块"}</span>
                  </button>
                  <button
                    type="button"
                    role="radio"
                    aria-checked={sourceMode === "existing"}
                    tabIndex={sourceMode === "existing" ? 0 : -1}
                    className="mod-source-choice"
                    disabled={preparing || sourceMods.length === 0}
                    onClick={() => selectSourceMod("")}
                  >
                    <strong>{isEnglish ? "Existing Mod" : "已有 Mod"}</strong>
                    <span>{isEnglish ? "Keep every detected feature" : "继承并锁定已有功能"}</span>
                  </button>
                </div>
                {sourceMode === "existing" && (
                  <label className="mod-processing-source-select">
                    <span>{isEnglish ? "Source Mod" : "源 Mod"}</span>
                    <select
                      className="settings-input"
                      value={sourceName}
                      disabled={preparing}
                      onChange={(event) => selectSourceMod(event.target.value)}
                    >
                      <option value="" disabled>{isEnglish ? "Select a Mod" : "请选择 Mod"}</option>
                      {sourceMods.map((mod) => <option key={mod.name} value={mod.name}>{mod.name}</option>)}
                    </select>
                  </label>
                )}
              </div>
            </section>
          )}

          <section className="mod-processing-section mod-processing-capabilities">
            <div className="mod-processing-section-heading">
              <div>
                <h3>{isEnglish ? "Feature modules" : "功能模块"}</h3>
                <p>{draft.installationOnly ? (isEnglish ? "Build and validate in this game directory, then copy the launch arguments." : "在所选游戏目录加工并校验，完成后复制启动参数。") : operationKind === "augment"
                  ? (isEnglish
                    ? "Installed modules remain intact. Choose only the additional capabilities you need."
                    : "目标 Mod 已有模块会完整保留，只需选择这次要增补的功能。")
                  : (isEnglish
                    ? "Modules inherited from the source remain installed in the new result."
                    : "源 Mod 已有模块会完整保留到新结果中。")}</p>
              </div>
            </div>
            <div className="mod-processing-features">
              {!minimalMode && <div className="mod-feature-with-options"><FeatureChoice
                title={isEnglish ? "Audio recognition" : "声纹识别"}
                detail={isEnglish ? "Drops and accumulated combat time" : "掉落与累计战斗时间，包含主城和主界面声纹"}
                checked={audioSelected}
                locked={audioRequired || audioInherited}
                lockLabel={audioInherited ? (operationKind === "augment" ? (isEnglish ? "Already installed" : "目标 Mod 已有") : (isEnglish ? "Included in source" : "源 Mod 已有")) : (isEnglish ? "Required for this setup" : "本次目标 · 必选")}
                disabled={preparing}
                onChange={value => actions.changeFeatures({ includeAudioTelemetry: value })} />
              </div>}
              {!minimalMode && <div className="mod-feature-with-options"><FeatureChoice
                title={isEnglish ? "In-game room tools" : "局内房间工具"}
                detail={isEnglish ? "Create/join shortcuts and automation with hidden buttons" : "创建、加入与自动跟房，局内按钮始终隐藏"}
                checked={roomToolsSelected}
                locked={roomToolsRequired || roomToolsInherited}
                lockLabel={roomToolsInherited
                  ? operationKind === "augment"
                    ? (isEnglish ? "Already installed" : "目标 Mod 已有")
                    : (isEnglish ? "Included in source" : "源 Mod 已有")
                  : (isEnglish ? "Required for room automation" : "自动跟房必选")}
                disabled={preparing}
                onChange={value => actions.changeFeatures({ includeRoomTools: value })}
              />
              {roomToolsSelected && <RoomToolProcessingOptions en={isEnglish}
                enabled={analysis.selection.roomToolsDefaultHell} disabled={preparing}
                onChange={value => actions.changeFeatures({ roomToolsDefaultHell: value })} />}
              </div>}
              <FeatureChoice
                title={isEnglish ? "Hostile auto-restart" : "敌对自动重开"}
                detail={isEnglish ? "Requires game sound. Includes Double Esc; works independently of scene/drop tracking." : "需开启游戏声音。自动包含双击 Esc 下一局，可独立于场景、掉落识别使用。"}
                checked={!!analysis.selection.includeHostileAutoRestart}
                locked={inheritedFeatureGroups.includes("hostile_auto_restart")}
                lockLabel={isEnglish ? "Installed" : "已安装"}
                disabled={preparing}
                onChange={value => actions.changeFeatures({ includeHostileAutoRestart: value, ...(value ? { includeEscNextGame: true } : {}) })}
              />
              <FeatureChoice
                title={isEnglish ? "Player loot buttons" : "左侧玩家尸体拾取按钮"}
                detail={isEnglish ? "Show corpse-loot buttons over player portraits." : "在左侧玩家头像上显示尸体拾取按钮。"}
                checked={!!analysis.selection.includePartyLoot}
                locked={inheritedFeatureGroups.includes("party_loot")}
                lockLabel={isEnglish ? "Installed" : "已安装"}
                disabled={preparing}
                onChange={value => actions.changeFeatures({ includePartyLoot: value })}
              />
              <FeatureChoice
                title={isEnglish ? "Scene restart" : "场景重开"}
                detail={isEnglish ? "Restart or create a game at an A3, Travincal, or A4 waypoint." : "需在 A3、崔凡克或 A4 小站处重开或创房。"}
                checked={!!analysis.selection.includeSceneShortcuts}
                locked={inheritedFeatureGroups.includes("scene_shortcuts")}
                lockLabel={isEnglish ? "Installed" : "已安装"}
                disabled={preparing}
                onChange={value => actions.changeFeatures({ includeSceneShortcuts: value })}
              />
              <FeatureChoice
                title={isEnglish ? "Double Esc: next Hell game" : "双击 Esc 下一局地狱"}
                detail={isEnglish ? "Press Esc twice within 0.5 seconds" : "0.5 秒内双击 Esc 退出并进入下一局地狱"}
                checked={!!analysis.selection.includeHostileAutoRestart || includeEscNextGame || inheritedFeatureGroups.includes(ESC_NEXT_GAME_FEATURE_ID)}
                locked={!!analysis.selection.includeHostileAutoRestart || inheritedFeatureGroups.includes(ESC_NEXT_GAME_FEATURE_ID)}
                lockLabel={inheritedFeatureGroups.includes(ESC_NEXT_GAME_FEATURE_ID)
                  ? (isEnglish ? "Installed" : "已安装")
                  : (isEnglish ? "Required by hostile auto-restart" : "敌对自动重开必选")}
                disabled={preparing}
                onChange={value => actions.changeFeatures({ includeEscNextGame: value })}
              />
              <FeatureChoice
                title={isEnglish ? "Auto-exit after death" : "死亡后自动退房"}
                detail={isEnglish
                  ? "Leave the current game after death. This cannot prevent death, and you cannot recover your corpse in that game."
                  : "死亡后离开当前房间；不能避免死亡，离开后无法在本局捡回尸体。"}
                checked={autoExitOnDeathSelected}
                locked={autoExitOnDeathInherited}
                lockLabel={isEnglish ? "Installed" : "已安装"}
                disabled={preparing}
                onChange={value => actions.changeFeatures({ includeAutoExitOnDeath: value })}
              />
            </div>
          </section>

          <section className="mod-processing-section mod-processing-output">
            <div className="mod-processing-section-heading">
              <div>
                <h3>{isEnglish ? "Output" : "输出与应用"}</h3>
                <p>{draft.installationOnly
                  ? (isEnglish ? "Process and validate in this game directory, then copy the launch arguments." : "在所选游戏目录加工并校验，完成后复制启动参数。")
                  : analysis.updating
                  ? (isEnglish ? "Rebuild from the original source and saved modules, then replace the same name after verification and apply it to the account." : "从原始来源与已保存模块重新生成，校验后替换同名成品并应用到账号。")
                  : operationKind === "augment"
                  ? (isEnglish ? "The selected Mod is augmented in place after verification, then applied to the target account." : "校验成功后原位增补所选 Mod，再应用到目标账号。")
                  : (isEnglish ? "Name the generated Mod, then build and apply it in one step." : "确认输出方式，生成并校验后应用。")}</p>
              </div>
            </div>
            <div className="mod-section-body">
              {operationKind === "create" && sourceMode === "existing" && <>
                <div className="mod-processing-source-options" role="radiogroup" aria-label={isEnglish ? "Output method" : "输出方式"} onKeyDown={navigateChoices}>
                  {[false, true].map(replace => <button key={String(replace)} type="button" role="radio" aria-checked={replaceSource === replace} tabIndex={replaceSource === replace ? 0 : -1} className="mod-source-choice" disabled={preparing}
                    onClick={() => { if (draft.recipe.kind === "create") actions.changeRecipe({ ...draft.recipe, replaceSource: replace }); }}>
                    <strong>{replace ? (isEnglish ? "Keep source name" : "沿用源名称") : (isEnglish ? "Create a new copy" : "生成新副本")}</strong>
                    <span>{replace ? (isEnglish ? "Keep the original folder as a backup" : "原文件夹改名备份") : (isEnglish ? "Choose a new name; keep the source" : "自定义名称，保留源 Mod")}</span>
                  </button>)}
                </div>
                {replaceSource && <p className="mod-section-description">{isEnglish ? "Verify first, then replace. The original folder is retained with a _backup_ suffix. Close games using this Mod first." : "先生成并校验，再替换。原目录加 _backup_ 后缀保留；请先关闭使用此 Mod 的游戏。"}</p>}
              </>}
              {operationKind === "augment" || replaceSource ? (
                <div className="mod-processing-existing-output">
                  <CheckCircle2 size={15} />
                  <span>{replaceSource ? sourceName : targetModName}</span>
                </div>
              ) : (
                <label className="mod-processing-name" htmlFor="processed-mod-name">
                  <span>{isEnglish ? "New Mod name" : "新 Mod 名称"}</span>
                  <input
                    id="processed-mod-name"
                    className="settings-input"
                    value={outputName}
                    maxLength={AUDIO_MOD_NAME_MAX_LENGTH}
                    disabled={preparing || replaceSource}
                    autoCapitalize="off"
                    autoCorrect="off"
                    spellCheck={false}
                    aria-invalid={!!outputNameError}
                    placeholder="MyD2RHubMod"
                    onChange={(event) => setOutputName(event.target.value)}
                  />
                  {showNameError && <small>{outputNameError}</small>}
                </label>
              )}
              {preparing && prepareProgress && (
                <div className="mod-processing-progress" aria-live="polite">
                  <div><span>{prepareProgress.message}</span><strong>{Math.round(prepareProgress.percent)}%</strong></div>
                  <ProgressBar value={prepareProgress.percent} label={isEnglish ? "Mod processing progress" : "Mod 加工进度"} />
                </div>
              )}
              {!!blockedReason && !preparing && (
                <p id={blockedReasonId} className="mod-processing-blocked" role="status">
                  <AlertTriangle size={13} />
                  {blockedReason}
                </p>
              )}
              <div className="mod-processing-actions">
                {preparing && <Button variant="ghost" size="md"
                  disabled={workflow.preparationTask.currentTask?.state !== "running" || workflow.preparationTask.cancelling || workflow.preparationTask.currentTask?.cancel_requested}
                  onClick={() => void workflow.preparationTask.cancel()}>
                  {workflow.preparationTask.cancelling || workflow.preparationTask.currentTask?.cancel_requested
                    ? (isEnglish ? "Cancelling…" : "正在取消…") : (isEnglish ? "Cancel processing" : "取消加工")}
                </Button>}
                <Button
                  variant="primary"
                  size="md"
                  loading={preparing}
                  disabled={preparing || (!workflow.prepared && (!workflow.processorReady || !!blockedReason))}
                  aria-describedby={blockedReason && workflow.processorReady && !workflow.prepared ? blockedReasonId : undefined}
                  onClick={() => void actions.prepare()}
                >
                  <PackageOpen size={14} />
                  {preparing
                    ? (isEnglish ? "Processing…" : "正在加工…")
                    : workflow.prepared ? (isEnglish ? "Retry application" : "重试应用")
                    : workflow.processorReady === null ? (isEnglish ? "Checking processor…" : "正在读取加工器…")
                    : !workflow.processorReady ? (isEnglish ? "Repair Hub installation" : "请修复 Hub 安装")
                    : isAddingFeatures
                      ? (isEnglish ? "Add selected modules" : "增补所选模块")
                      : isAugment
                        ? (isEnglish ? "Verify and update" : "校验并更新")
                        : (isEnglish ? "Process and apply" : "开始加工并应用")}
                </Button>
              </div>
              {workflow.preparationTask.cancelError && <p className="mod-catalog-error" role="alert">{workflow.preparationTask.cancelError}</p>}
              {workflow.notice && <p className="text-xs text-text-muted" role="status">{workflow.notice}</p>}
            </div>
          </section>
        </>
      )}
    </div>
  );
}
