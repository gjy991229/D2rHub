import type { GlobalConfig } from "./types";

// Keep this array JSON-compatible: the Rust contract test reads the same
// declaration and compares it with the serialized GlobalConfig field set.
export const GLOBAL_CONFIG_FIELDS = [
  "version",
  "cn_battle_net_path",
  "cn_game_path",
  "cn_saved_games_path",
  "global_game_path",
  "global_saved_games_path",
  "legacy_path_migration",
  "program_data_agent_path",
  "app_data_roaming_bnet_path",
  "accounts_dir",
  "first_run_complete",
  "installed_optional_modules",
  "browser_path",
  "browser_type",
  "enable_bongo_cat",
  "bongo_cat_chatterbox",
  "bongo_cat_scale",
  "bongo_cat_skin",
  "bongo_cat_unlocked_skins",
  "enable_overlay",
  "enable_tz_overlay",
  "enable_stats_overlay",
  "theme",
  "theme_overlay",
  "auto_close_browser",
  "enable_auto_update",
  "first_launch",
  "rune_audio_enabled",
  "rune_audio_target_account",
  "rune_audio_detection_threshold",
  "rune_audio_tracked_categories",
  "rune_audio_min_rune_number",
  "rune_audio_min_gem_level",
  "rune_audio_tracked_charm_codes",
  "shortcut_bindings_json",
  "overlay_opacity",
  "main_opacity",
  "font_scale",
  "separate_game_taskbar_icons",
  "app_language",
  "agent_mode",
  "agent_delay_secs",
  "agent_threshold",
  "launch_groups",
  "favorite_launch_group_ids"
] as const satisfies readonly (keyof GlobalConfig)[];

type MissingGlobalConfigField = Exclude<keyof GlobalConfig, typeof GLOBAL_CONFIG_FIELDS[number]>;

// A newly added TypeScript field must be declared above. Extra names are
// rejected by `satisfies`; the Rust test checks the other side of the contract.
export const GLOBAL_CONFIG_FIELDS_COVER_TYPESCRIPT:
  [MissingGlobalConfigField] extends [never] ? true : never = true;
