// ── 数据模型 ──

export interface LegacyPathMigration {
  game_path: string;
  saved_games_path: string;
  battle_net_path: string;
}

export interface LaunchGroup {
  id: string;
  name: string;
  account_ids: string[];
  members?: LaunchGroupMember[];
}

export interface LaunchGroupMember {
  account_id: string;
  /** undefined/null = legacy group inherits the account default; empty string = explicitly no Mod. */
  mod_args?: string | null;
  position_preset_id?: string | null;
  position_configured?: boolean;
  /** false/undefined keeps legacy groups inheriting the account's current graphics settings. */
  graphics_configured?: boolean;
  resolution?: string | null;
  fps?: number | null;
}

export interface WindowPositionPreset {
  id: string;
  name: string;
  x: number;
  y: number;
}

export interface GlobalConfig {
  version: number;
  cn_battle_net_path: string;
  cn_game_path: string;
  cn_saved_games_path: string;
  global_game_path: string;
  global_saved_games_path: string;
  legacy_path_migration?: LegacyPathMigration | null;
  program_data_agent_path: string;
  app_data_roaming_bnet_path: string;
  accounts_dir: string;
  first_run_complete: boolean;
  browser_path: string;
  browser_type: string;
  enable_bongo_cat: boolean;
  bongo_cat_chatterbox: boolean;
  bongo_cat_scale: number;
  bongo_cat_skin: string;
  bongo_cat_unlocked_skins: string[];
  enable_overlay: boolean;
  enable_tz_overlay: boolean;
  enable_stats_overlay: boolean;
  theme: string;
  theme_overlay: string;
  auto_close_browser: boolean;
  enable_auto_update: boolean;
  first_launch: boolean;
  rune_audio_enabled: boolean;
  rune_audio_target_account: string;
  rune_audio_detection_threshold?: number;
  rune_audio_tracked_categories: string[];
  rune_audio_min_rune_number?: number;
  rune_audio_min_gem_level?: number;
  rune_audio_tracked_charm_codes?: string[];
  shortcut_bindings_json: string;
  overlay_opacity: number;
  main_opacity: number;
  font_scale: string;
  separate_game_taskbar_icons?: boolean;
  app_language?: string;
  agent_mode?: number;
  agent_delay_secs?: number;
  agent_threshold?: number;
  launch_groups: LaunchGroup[];
  favorite_launch_group_ids?: string[];
}

// ── 可选能力运行状态 ──

/**
 * Runtime state reported by the backend capability supervisor.
 *
 * This deliberately describes observed lifecycle state, not a value inferred
 * from global configuration. `requested_enabled` remains separate so a module
 * that was requested but failed to start is not presented as healthy.
 */
export type CapabilityRuntimeState =
  | "disabled"
  | "stopped"
  | "starting"
  | "running"
  | "degraded"
  | "failed";

export interface CapabilityStatus {
  /** Stable, non-localized, kebab-case module identifier. */
  id: string;
  /** Persisted user intent as resolved by the backend compatibility adapter. */
  requested_enabled: boolean;
  /** Actual lifecycle state observed by the backend supervisor. */
  state: CapabilityRuntimeState;
  /** Stable machine-readable reason; localized copy remains frontend-owned. */
  reason_code: string | null;
}

export interface CapabilityStatusSnapshot {
  /** Monotonic backend revision used to reject stale command responses. */
  revision: number;
  capabilities: CapabilityStatus[];
}

// ── 数据统计 ──

export type DropKind = "rune" | "item";

/// 单条通用掉落记录（兼容旧版符文字段由 Rust 迁移）。
export interface PersistedDropEntry {
  kind: DropKind;
  telemetry_id: number;
  item_code?: string | null;
  category: string;
  display_name: string;
  display_name_en?: string | null;
  rune_number?: number | null;
  screenshot_path?: string | null;  // 截图相对路径（仅 #24+ 符文）
}

export interface SceneRecord {
  id?: number;
  absolute_time: string;
  character_name: string;
  scene_name: string;
  tz?: boolean;
  timer_seconds: number;
  journey_id?: string | null;
  segment_index?: number | null;
  drops: PersistedDropEntry[];    // 每个元素 = 一次独立掉落
}

export interface MergeStrategy {
  id: number;
  name: string;
  scene_names: string[];
}

export interface DropObservation {
  id: number;
  observed_at: string;
  account_id: string;
  kind: DropKind;
  telemetry_id: number;
  item_code?: string | null;
  category: string;
  display_name: string;
  display_name_en: string;
  rune_number?: number | null;
  confidence: number;
  source: string;
  scene_record_id?: number | null;
}

export interface StatsData {
  records: SceneRecord[];
  observations: DropObservation[];
  strategies: MergeStrategy[];
}

/// 符文声纹识别事件。
export interface RuneAudioEvent {
  source: string;
  account_id: string;
  timestamp: string;
  rune_number: number;
  rune_name: string;
  rune_name_en: string;
  confidence: number;
}

export interface ItemAudioEvent {
  source: string;
  account_id: string;
  timestamp: string;
  item_id: number;
  item_code: string;
  category: string;
  item_name: string;
  item_name_en: string;
  confidence: number;
}

export interface TrackingDropSnapshot {
  observation_id: number;
  kind: DropKind;
  telemetry_id: number;
  code?: string | null;
  category: string;
  name: string;
  name_en: string;
  rune_number?: number | null;
}

export interface TrackingSnapshot {
  revision: number;
  account_id: string;
  current_area_id: number | null;
  current_scene: string;
  current_scene_en: string;
  tz?: boolean;
  location_kind: "town" | "wilderness" | "frontend" | null;
  is_town: boolean;
  is_frontend: boolean;
  is_timing: boolean;
  timer_started_at_ms: number | null;
  current_run_key: string | null;
  current_run_name: string | null;
  current_run_name_en: string | null;
  current_run_drops: TrackingDropSnapshot[];
  previous_run_drops?: TrackingDropSnapshot[];
  session_drops?: TrackingDropSnapshot[];
  session_runs: Record<string, number>;
}

export interface AccountMeta {
  id: string;
  display_name: string;
  mod_args: string;
  mod_list?: string[];
  created_at: string;
  last_launched_at: string | null;
  last_reset_at: string | null;
  initialized: boolean;
  order: number;
  is_running: boolean;
  running_pid?: number | null;
  window_x?: number | null;
  window_y?: number | null;
  position_presets?: WindowPositionPreset[];
  active_position_id?: string | null;
  auth_mode?: string | null;
  region?: string | null;
  language?: string | null;
  voicelanguage?: string | null;
  has_customized_settings?: boolean;
}

export interface LaunchProgress {
  account_id: string;
  step: string;
  status: string; // "pending" | "running" | "ok" | "error"
  message: string;
}

export interface LaunchResult {
  account_id: string;
  success: boolean;
  d2r_pid: number | null;
  error: string | null;
  mutex_killed: boolean;
}

export interface LaunchAccountEntry {
  account_id: string;
  overrides: {
    mod_args: string;
    position_preset_id: string | null;
    resolution?: string | null;
    fps?: number | null;
  };
}

export interface AudioModRuntimeWarning {
  account_id: string;
  account_name: string;
  target_pid: number;
  reason_code: string;
  message: string;
}

export interface InstalledAudioMod {
  name: string;
  audio_ready: boolean;
  update_required: boolean;
  source_eligible: boolean;
  feature_groups: string[];
  audio_reusable: boolean;
  auto_exit_on_death_enabled: boolean;
}

export interface AudioModFeatureGroup {
  id: string;
  recipe_version: number;
  fingerprint: string;
  reused_from_source: boolean;
}

export interface AudioModSetupState {
  account_id: string;
  account_name: string;
  current_mod_name: string | null;
  launch_arguments: string;
  has_txt: boolean;
  ready: boolean;
  update_required: boolean;
  recipe_version: number | null;
  required_recipe_version: number;
  build_mode: "minimal" | "augment" | null;
  source_mod_name: string | null;
  feature_groups: string[];
  auto_exit_on_death_enabled: boolean;
  reason_code: string;
  message: string;
  installed_mods: InstalledAudioMod[];
  running_pid: number | null;
  session_verified: boolean;
  active_session_ready: boolean | null;
  active_session_update_required: boolean | null;
  restart_required: boolean;
}

export interface ModCapsule {
  id: string;
  edition: "CN" | "Global" | string;
  name: string;
  origin: "scanned" | "custom" | string;
  launch_arguments: string;
  default_launch_arguments: string | null;
  feature_groups: string[];
  auto_exit_on_death_enabled?: boolean;
  processed: boolean;
  source_eligible: boolean;
  update_required: boolean;
  ready: boolean;
  deletable: boolean;
  assigned_account_ids: string[];
}

export interface ModCapsuleAccountSelection {
  account_id: string;
  account_name: string;
  edition: "CN" | "Global" | string | null;
  selected_capsule_id: string | null;
  legacy_mod_arguments: string;
  issue: string | null;
}

export interface ModCapsulePool {
  generation: number;
  scanned_at: string;
  capsules: ModCapsule[];
  accounts: ModCapsuleAccountSelection[];
}
