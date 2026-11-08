import type { AudioModSetupState } from "../../../store/types";
import { validateAudioModName } from "../../../utils/audioModName";
import {
  AUDIO_TELEMETRY_FEATURE_ID, IN_GAME_ROOM_TOOLS_FEATURE_ID, ESC_NEXT_GAME_FEATURE_ID, AUTO_EXIT_ON_DEATH_FEATURE_ID,
  audioModFeatureDefaultsForPurpose, hasSelectedAudioModFeature, selectedAudioModFeatureAddsCapability,
} from "../featureContract";
import type { ModProcessingDraft, ModWorkflowOrigin } from "./types";

export function initialModFeatures(origin: ModWorkflowOrigin) {
  return audioModFeatureDefaultsForPurpose(origin === "room-automation" ? "room-tools" : origin === "library" ? "manage" : "recognition");
}

export function describeModDraft(draft: ModProcessingDraft, state: AudioModSetupState | null, en: boolean) {
  const selectedName = draft.recipe.kind === "augment" ? draft.recipe.modName : draft.recipe.source;
  const selected = state?.installed_mods.find(mod => mod.name.toLocaleLowerCase() === selectedName?.toLocaleLowerCase());
  const inherited = selected?.feature_groups ?? [];
  const augment = draft.recipe.kind === "augment";
  const includeAudioTelemetry = draft.features.includeAudioTelemetry || draft.origin === "recognition" || inherited.includes(AUDIO_TELEMETRY_FEATURE_ID) || (!!selected?.update_required && !inherited.length);
  const requestedSceneAudio = includeAudioTelemetry ? draft.features.includeSceneAudio : undefined;
  const sceneAudioChanged = requestedSceneAudio !== undefined
    && requestedSceneAudio !== (selected?.scene_audio_enabled ?? true);
  const selection = {
    includeSceneAudio: augment
      ? (sceneAudioChanged ? requestedSceneAudio : undefined)
      : includeAudioTelemetry ? requestedSceneAudio ?? selected?.scene_audio_enabled ?? true : undefined,
    roomToolsDefaultHell: draft.features.roomToolsDefaultHell ?? selected?.room_tools_default_hell ?? true,
    includeAudioTelemetry,
    includeRoomTools: draft.features.includeRoomTools || draft.origin === "room-automation" || inherited.includes(IN_GAME_ROOM_TOOLS_FEATURE_ID),
    includeSceneShortcuts: draft.features.includeSceneShortcuts || inherited.includes("scene_shortcuts"),
    includePartyLoot: draft.features.includePartyLoot || inherited.includes("party_loot"),
    includeHostileAutoRestart: draft.features.includeHostileAutoRestart || inherited.includes("hostile_auto_restart"),
    includeEscNextGame: draft.features.includeHostileAutoRestart || inherited.includes("hostile_auto_restart") || draft.features.includeEscNextGame || inherited.includes(ESC_NEXT_GAME_FEATURE_ID),
    includeAutoExitOnDeath: draft.features.includeAutoExitOnDeath || inherited.includes(AUTO_EXIT_ON_DEATH_FEATURE_ID),
  };
  const roomOptionsChanged = selection.includeRoomTools
    && selection.roomToolsDefaultHell !== (selected?.room_tools_default_hell ?? true);
  const updating = draft.recipe.kind === "augment" && (!!selected?.update_required || !!draft.recipe.rebuild || sceneAudioChanged);
  const originalName = draft.recipe.kind === "augment"
    ? draft.recipe.sourceOverride ?? selected?.source_mod_name : null;
  const original = originalName ? state?.installed_mods.find(mod => mod.name.toLowerCase() === originalName.toLowerCase()) : null;
  const originalUnavailable = updating && !!originalName && (!original || !original.source_eligible
    || original.requires_unpack || original.update_required || original.feature_groups.length > 0);
  const nameError = draft.recipe.kind === "create" ? validateAudioModName(draft.recipe.replaceSource ? draft.recipe.source ?? "" : draft.recipe.name, draft.recipe.replaceSource ? [] : state?.installed_mods.map(mod => mod.name) ?? []) : null;
  const targetReason = draft.installationOnly
    ? (!state || state.account_id !== "" ? (en ? "Inspect the game installation before processing" : "请先检查游戏目录的 Mod") : "")
    : !draft.accountId ? (en ? "Select an initialized account" : "请选择已初始化的账号")
      : !state || state.account_id !== draft.accountId ? (en ? "Inspect the target account before processing" : "请先检查目标账号的 Mod") : "";
  const blockedReason = draft.recipe.kind === "create" && draft.recipe.replaceSource && !draft.recipe.source
    ? (en ? "Same-name processing requires an existing source Mod" : "同名加工需要选择已有源 Mod")
    : targetReason ? targetReason
    : selected?.requires_unpack ? (en ? "Unpack this MPQ in the Mod library before processing" : "请先在 Mod 库点击解压，完成后再加工")
      : selectedName && (!selected || (!augment && !selected.source_eligible)) ? (en ? "The selected Mod is unavailable; rescan and choose again" : "所选 Mod 已不可用，请重新扫描后再选择")
        : draft.recipe.kind === "create" && draft.recipe.source === "" ? (en ? "Select a source Mod" : "请选择源 Mod")
        : augment && !selected?.feature_groups.length && !selected?.update_required ? (en ? "Select a processed Mod to augment" : "请选择一个已加工 Mod")
          : originalUnavailable ? (en ? "Original source Mod is unavailable or already processed. Select an unpacked, unprocessed source." : "原始源 Mod 不可用或已经加工，请重新选择已解压且未经加工的来源。")
          : !hasSelectedAudioModFeature(selection) ? (en ? "Select at least one Mod feature" : "请至少选择一个 Mod 功能")
            : augment && !updating && !roomOptionsChanged && !selectedAudioModFeatureAddsCapability(selection, inherited) ? (en ? "This Mod already contains every selected feature" : "当前 Mod 已包含所选功能，请选择一个尚未安装的功能")
              : nameError || "";
  return { selected, inherited, selection, augment, updating, roomOptionsChanged, nameError, blockedReason };
}
