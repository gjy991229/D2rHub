import type { InstalledAudioMod } from "../../../store/types";
import { validateAudioModName } from "../../../utils/audioModName";
import { JCY_INVENTORY_FEATURE_ID, hasSelectedAudioModFeature, selectedAudioModFeatureAddsCapability, type AudioModFeatureSelection } from "../featureContract";

export type BatchMode = "create" | "replace-source" | "augment" | "rebuild";
export interface BatchRow {
  source: string;
  name: string;
  sourceModName: string | null;
  mode: BatchMode;
  features: AudioModFeatureSelection;
  state: "blocked" | "pending" | "running" | "success" | "failed" | "skipped";
  message: string;
}

export function batchPlan(mods: InstalledAudioMod[], selected: string[], mode: BatchMode,
  features: AudioModFeatureSelection, names: Record<string, string>, en: boolean): BatchRow[] {
  const occupied = mods.map(mod => mod.name);
  const outputs = new Set<string>();
  return selected.map(source => {
    const mod = mods.find(mod => mod.name === source);
    const effectiveMode = mode === "augment" && mod?.update_required ? "rebuild" : mode;
    const name = mode === "create" ? (names[source] ?? `${source}-Hub`).trim() : source;
    const groups = mod?.feature_groups ?? [];
    const roomToolsDefaultHell = mode === "rebuild" || !features.includeRoomTools
      ? mod?.room_tools_default_hell ?? true : features.roomToolsDefaultHell ?? true;
    const selection = mode === "rebuild" ? {
      roomToolsDefaultHell,
      includeAudioTelemetry: groups.includes("audio_telemetry") || (!groups.length && !!mod?.update_required),
      includeRoomTools: groups.includes("in_game_room_tools"),
      includeJcyInventory: groups.includes(JCY_INVENTORY_FEATURE_ID),
      includeSceneShortcuts: groups.includes("scene_shortcuts"),
      includePartyLoot: groups.includes("party_loot"),
      includeHostileAutoRestart: groups.includes("hostile_auto_restart"),
      includeEscNextGame: groups.includes("esc_next_game"),
      includeAutoExitOnDeath: groups.includes("auto_exit_on_death"),
    } : {
      roomToolsDefaultHell,
      includeAudioTelemetry: features.includeAudioTelemetry || groups.includes("audio_telemetry") || (!!mod?.update_required && !groups.length),
      includeRoomTools: features.includeRoomTools || groups.includes("in_game_room_tools"),
      includeJcyInventory: features.includeJcyInventory || groups.includes(JCY_INVENTORY_FEATURE_ID),
      includeSceneShortcuts: features.includeSceneShortcuts || groups.includes("scene_shortcuts"),
      includePartyLoot: features.includePartyLoot || groups.includes("party_loot"),
      includeHostileAutoRestart: features.includeHostileAutoRestart || groups.includes("hostile_auto_restart"),
      includeEscNextGame: features.includeEscNextGame || groups.includes("esc_next_game"),
      includeAutoExitOnDeath: features.includeAutoExitOnDeath || groups.includes("auto_exit_on_death"),
    };
    let message = !mod ? (en ? "Source no longer exists" : "来源已不存在")
      : mod.requires_unpack ? (en ? "Unpack this MPQ first" : "请先解压 MPQ")
      : effectiveMode === "rebuild" && groups.some(id => ![JCY_INVENTORY_FEATURE_ID, "party_loot", "scene_shortcuts", "audio_telemetry", "in_game_room_tools", "esc_next_game", "auto_exit_on_death"].includes(id))
        ? (en ? "Unknown module cannot be rebuilt by this Hub" : "包含当前 Hub 无法重做的未知模块")
      : (mode === "create" || mode === "replace-source") && !mod.source_eligible ? (en ? "Source is unavailable" : "该 Mod 不能作为加工来源")
      : mode !== "create" && mode !== "replace-source" && !groups.length && !mod.update_required ? (en ? "Not a processed Mod" : "不是已加工 Mod")
      : !hasSelectedAudioModFeature(selection) ? (en ? "Choose at least one module" : "请至少选择一个模块")
      : mode === "augment" && !mod.update_required && !(selection.includeRoomTools && roomToolsDefaultHell !== (mod.room_tools_default_hell ?? true))
        && !selectedAudioModFeatureAddsCapability(features, groups)
        ? (en ? "Selected modules already installed" : "已包含所选模块，无需增补")
      : "";
    if (!message && (mode === "create" || mode === "replace-source")) message = validateAudioModName(name, mode === "replace-source" ? [] : occupied) ?? "";
    if (!message && outputs.has(name.toLowerCase())) message = en ? "Duplicate output name" : "本批次输出名称重复";
    outputs.add(name.toLowerCase());
    if (!message && (mode === "rebuild" || (mode === "augment" && mod?.update_required)) && mod?.source_mod_name
      && !mods.some(item => item.name.toLowerCase() === mod.source_mod_name!.toLowerCase() && item.source_eligible && !item.requires_unpack && !item.feature_groups.length && !item.update_required)) {
      message = en ? "Original source Mod missing or packed" : "原始源 Mod 缺失或尚未解压";
    }
    return { source, name, sourceModName: mode === "create" || mode === "replace-source" ? source : mod?.source_mod_name ?? null,
      mode: effectiveMode, features: selection, state: message ? "blocked" : "pending",
      message: message || (effectiveMode !== mode ? (en ? "Protocol changed; rebuild from original source" : "协议不一致，将从原始源 Mod 重做") : "") };
  });
}
