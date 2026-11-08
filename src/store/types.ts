// ── 数据模型 ──

export interface LegacyPathMigration {
  game_path: string;
  saved_games_path: string;
  battle_net_path: string;
}

export interface SchemeEnvironment {
  schema_version: 2;
  settings: Pick<GlobalConfig, "rune_audio_target_account" | "keep_battle_net_account_id">;
  room: { primary_account_id: string; follower_account_ids: string[] };
  layout: WindowLayout | null;
}

export interface AccountLaunchSnapshot {
  game_settings: Record<string, unknown>;
  position: [number, number] | null;
  region: string | null;
  language: string | null;
  voicelanguage: string | null;
}

export interface LaunchGroup {
  revision?: number;
  environment?: SchemeEnvironment | null;
  id: string;
  name: string;
  account_ids: string[];
  members?: LaunchGroupMember[];
}

export interface LaunchGroupMember {
  snapshot?: AccountLaunchSnapshot | null;
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

/** Desktop geometry uses physical pixels; slots are relative to a work area. */
export interface LayoutRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface FrameInsets { left: number; top: number; right: number; bottom: number }
export interface WindowFrameMetrics {
  dpi: number;
  style: number;
  ex_style: number;
  visible: FrameInsets;
  invisible: FrameInsets;
}
export interface WindowFrameProfile { monitor_id: string; scale_factor: number; metrics: WindowFrameMetrics }

export interface LayoutMonitor {
  id: string;
  name: string;
  bounds: LayoutRect;
  work_area: LayoutRect;
  primary: boolean;
  scale_factor: number;
  frame?: WindowFrameMetrics | null;
}

export interface LayoutSlot extends LayoutRect {
  /** width/height are Settings.json game resolution, not native window size. */
  monitor_id: string;
}

export interface WindowLayout {
  id: string;
  name: string;
  monitors: LayoutMonitor[];
  /** 1-based launch order is the array index + 1. Overlap is allowed. */
  windows: LayoutSlot[];
}

export interface GlobalConfig {
  launch_session?: { id: string; scheme: LaunchGroup; started_at: string; members?: LaunchGroupMember[] } | null;
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
  feature_profile?: "normal" | "minimal";
  feature_profile_prompt_revision?: number;
  installed_optional_modules?: string[];
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
  launch_trim_memory?: boolean;
  /** Reopen this account's Battle.net client after its game launch batch completes. */
  keep_battle_net_account_id?: string | null;
  enable_auto_update: boolean;
  download_source?: "auto" | "gitee" | "github";
  first_launch: boolean;
  rune_audio_enabled: boolean;
  hostile_auto_restart_enabled?: boolean;
  hostile_auto_restart_all_windows?: boolean;
  rune_audio_target_account: string;
  /** null/absent retains account monitoring and its saved target. */
  rune_audio_external_target?: { edition: "CN" | "Global"; mod_name: string } | null;
  rune_audio_detection_threshold?: number;
  rune_audio_tracked_categories: string[];
  rune_audio_min_rune_number?: number;
  rune_audio_min_gem_level?: number;
  rune_audio_tracked_charm_codes?: string[];
  shortcut_bindings_json: string;
  launch_scheme_shortcuts?: { group_id: string; shortcut: string }[];
  /** Main-window toggle; retain the stored name for older configurations. */
  show_main_window_shortcut?: string;
  /** Legacy fallback, cleared when migrated to the single toggle shortcut. */
  hide_main_window_shortcut?: string;
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
  window_layout_enabled?: boolean;
  active_window_layout_id?: string | null;
  window_layouts?: WindowLayout[];
  window_frame_profiles?: WindowFrameProfile[];
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
  /** Sanitized backend failure detail for diagnostics and recovery guidance. */
  last_error?: string | null;
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
  source_id?: string | null;
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
  source_id?: string;
  source_name?: string;
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
  screenshot_path?: string | null;
}

export interface StatsData {
  records: SceneRecord[];
  observations: DropObservation[];
  strategies: MergeStrategy[];
}

/// 符文声纹识别事件。
export interface RuneAudioEvent {
  source_id?: string;
  source: string;
  account_id: string;
  timestamp: string;
  rune_number: number;
  rune_name: string;
  rune_name_en: string;
  confidence: number;
}

export interface ItemAudioEvent {
  source_id?: string;
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
  source_id?: string;
  revision: number;
  account_id: string;
  /** Legacy compatibility field; name-based OCR scenes leave this null. */
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
    snapshot?: AccountLaunchSnapshot | null;
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
  source_mod_name?: string | null;
  audio_ready: boolean;
  update_required: boolean;
  source_eligible: boolean;
  requires_unpack?: boolean;
  unpack_recovery_required?: boolean;
  feature_groups: string[];
  audio_reusable: boolean;
  auto_exit_on_death_enabled: boolean;
  room_tools_default_hell?: boolean;
  scene_audio_enabled?: boolean;
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
  lightweight_profile?: "main" | "filler" | "min" | null;
  issue?: string | null;
  id: string;
  edition: "CN" | "Global" | string;
  name: string;
  origin: "scanned" | "custom" | string;
  launch_arguments: string;
  default_launch_arguments: string | null;
  source_mod_name?: string | null;
  feature_groups: string[];
  auto_exit_on_death_enabled?: boolean;
  processed: boolean;
  source_eligible: boolean;
  requires_unpack?: boolean;
  unpack_recovery_required?: boolean;
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

export interface ModUnpackProgress {
  capsule_id: string;
  task_id: number;
  percent: number;
  message: string;
}
export interface ModUnpackResult {
  pool: ModCapsulePool;
  backup_path: string | null;
  escaped_name_count: number;
}

export interface ModCapsulePool {
  generation: number;
  scanned_at: string;
  capsules: ModCapsule[];
  accounts: ModCapsuleAccountSelection[];
}
