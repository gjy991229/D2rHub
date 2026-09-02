import type { AudioModFeatureGroup, AudioModSetupState } from "../../store/types";

export interface RuneAudioStatus {
  running: boolean;
  account_id: string | null;
  target_pid: number | null;
  last_error: string | null;
  captured_frames: number;
  audio_peak: number;
  decoded_packets: number;
  rune_events: number;
  item_events: number;
  scene_heartbeats: number;
  last_marker: string | null;
  last_confidence: number | null;
  last_detected_at: string | null;
  diagnostic_recording: boolean;
  diagnostic_recording_path: string | null;
}

export interface AudioModPrepareProgress {
  account_id: string;
  phase: string;
  percent: number;
  message: string;
}

export interface AudioModPrepareResult {
  account_id: string;
  mod_name: string;
  mod_directory: string;
  launch_arguments: string;
  source_mod_name: string | null;
  feature_groups: AudioModFeatureGroup[];
}

export interface AudioModFeatureSelection {
  includeAudioTelemetry: boolean;
  includeRoomTools: boolean;
  includeAutoExitOnDeath: boolean;
}

export type AudioModProcessingPurpose = "recognition" | "room-tools" | "manage";
export type AudioModProcessingMode = "create" | "augment";

export const AUDIO_TELEMETRY_FEATURE_ID = "audio_telemetry";
export const IN_GAME_ROOM_TOOLS_FEATURE_ID = "in_game_room_tools";
export const AUTO_EXIT_ON_DEATH_FEATURE_ID = "auto_exit_on_death";

export function audioModFeatureDefaultsForPurpose(
  purpose: AudioModProcessingPurpose,
): AudioModFeatureSelection {
  return {
    includeAudioTelemetry: purpose === "recognition",
    includeRoomTools: purpose === "recognition" || purpose === "room-tools",
    includeAutoExitOnDeath: false,
  };
}

export function audioModFeatureInvokeOptions(
  selection: AudioModFeatureSelection,
): AudioModFeatureSelection {
  return {
    includeAudioTelemetry: selection.includeAudioTelemetry,
    includeRoomTools: selection.includeRoomTools,
    includeAutoExitOnDeath: selection.includeAutoExitOnDeath,
  };
}

export function hasSelectedAudioModFeature(
  selection: AudioModFeatureSelection,
): boolean {
  return selection.includeAudioTelemetry
    || selection.includeRoomTools
    || selection.includeAutoExitOnDeath;
}

export function selectedAudioModFeatureAddsCapability(
  selection: AudioModFeatureSelection,
  installedGroups: readonly string[],
): boolean {
  return (
    (selection.includeAudioTelemetry && !installedGroups.includes(AUDIO_TELEMETRY_FEATURE_ID))
    || (selection.includeRoomTools && !installedGroups.includes(IN_GAME_ROOM_TOOLS_FEATURE_ID))
    || (selection.includeAutoExitOnDeath
      && !installedGroups.includes(AUTO_EXIT_ON_DEATH_FEATURE_ID))
  );
}

export function hasAudioTelemetry(
  groups: readonly string[] | readonly AudioModFeatureGroup[],
): boolean {
  return groups.some((group) => (
    typeof group === "string" ? group : group.id
  ) === AUDIO_TELEMETRY_FEATURE_ID);
}

export function audioSetupDefaults(state: AudioModSetupState): {
  mode: "original" | "existing";
  source: string;
  name: string;
} {
  const sources = state.installed_mods.filter((mod) => mod.source_eligible);
  const recordedSource = sources.find((mod) => (
    !!state.source_mod_name && mod.name.toLowerCase() === state.source_mod_name.toLowerCase()
  ));
  const currentSource = sources.find((mod) => (
    !!state.current_mod_name && mod.name.toLowerCase() === state.current_mod_name.toLowerCase()
  ));
  const source = (!state.update_required ? currentSource : undefined)
    ?? recordedSource
    ?? (state.update_required && state.build_mode === "augment" && sources.length === 1 ? sources[0] : undefined)
    ?? (!state.update_required ? sources[0] : undefined);
  const preserveExisting = state.update_required
    ? state.build_mode === "augment"
    : !!source;
  return {
    mode: preserveExisting ? "existing" : "original",
    source: source?.name ?? "",
    name: state.update_required ? state.current_mod_name ?? "" : "",
  };
}

export const TRACKING_CATEGORIES = [
  { id: "runes", label: "符文", detail: "#1–#33" },
  { id: "gems", label: "宝石与骷髅", detail: "35 种等级/颜色" },
  { id: "charms", label: "护身符", detail: "小型/大型/超大型；不区分词缀" },
  { id: "jewels", label: "珠宝", detail: "基础珠宝；不区分品质或词缀" },
  { id: "keys", label: "钥匙", detail: "恐惧/憎恨/毁灭" },
  { id: "organs", label: "器官", detail: "角/眼/脑" },
  { id: "essences", label: "精华与徽章", detail: "四种精华及赦免徽章" },
] as const;
export const DEFAULT_TRACKING_CATEGORIES = TRACKING_CATEGORIES.map(category => category.id);
export const GEM_LEVELS = ["碎裂", "裂开", "普通", "无瑕疵", "完美"] as const;
export const CHARM_FILTERS = [
  { code: "cm1", label: "小型护身符", detail: "Small Charm" },
  { code: "cm2", label: "大型护身符", detail: "Large Charm" },
  { code: "cm3", label: "超大型护身符", detail: "Grand Charm" },
] as const;
export const AGGREGATE_ITEM_FILTERS = [
  { id: "jewels", label: "珠宝", detail: "全部基础珠宝，不区分品质或词缀" },
  { id: "keys", label: "钥匙", detail: "恐惧、憎恨、毁灭三把钥匙" },
  { id: "organs", label: "器官", detail: "角、眼、脑作为一整项" },
  { id: "essences", label: "精华与徽章", detail: "四种精华及赦免徽章" },
] as const;
