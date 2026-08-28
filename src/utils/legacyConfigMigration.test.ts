import type { GlobalConfig } from "../store/types";
import { resolveLegacyPathMigration } from "./legacyConfigMigration";

function config(overrides: Partial<GlobalConfig> = {}): GlobalConfig {
  return {
    version: 6,
    cn_battle_net_path: "",
    cn_game_path: "",
    cn_saved_games_path: "",
    global_game_path: "",
    global_saved_games_path: "",
    legacy_path_migration: {
      game_path: "D:/Legacy/D2R",
      saved_games_path: "D:/Legacy/Saves",
      battle_net_path: "D:/Legacy/Battle.net.exe",
    },
    program_data_agent_path: "C:/ProgramData/Battle.net/Agent",
    app_data_roaming_bnet_path: "C:/Roaming/Battle.net",
    accounts_dir: "C:/D2RHub/accounts",
    first_run_complete: false,
    browser_path: "C:/Browser/msedge.exe",
    browser_type: "edge",
    enable_bongo_cat: false,
    bongo_cat_chatterbox: true,
    bongo_cat_scale: 1,
    bongo_cat_skin: "original",
    bongo_cat_unlocked_skins: ["original"],
    enable_overlay: true,
    enable_tz_overlay: false,
    enable_stats_overlay: true,
    theme: "onyx",
    theme_overlay: "light",
    auto_close_browser: false,
    enable_auto_update: false,
    first_launch: false,
    rune_audio_enabled: false,
    rune_audio_target_account: "",
    rune_audio_tracked_categories: ["runes"],
    shortcut_bindings_json: "{}",
    overlay_opacity: 81,
    main_opacity: 92,
    font_scale: "large",
    ...overrides,
  };
}

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

const original = config();
const cn = resolveLegacyPathMigration(original, "CN");
assert(cn.cn_game_path === "D:/Legacy/D2R", "CN confirmation maps the legacy game path");
assert(cn.cn_saved_games_path === "D:/Legacy/Saves", "CN confirmation maps the legacy save path");
assert(cn.cn_battle_net_path === "D:/Legacy/Battle.net.exe", "CN confirmation maps Battle.net");
assert(cn.legacy_path_migration === null, "CN confirmation clears the pending migration marker");
assert(cn.theme === original.theme && cn.overlay_opacity === original.overlay_opacity,
  "migration confirmation preserves unrelated appearance settings");

const global = resolveLegacyPathMigration(original, "Global");
assert(global.global_game_path === "D:/Legacy/D2R", "Global confirmation maps the legacy game path");
assert(global.global_saved_games_path === "D:/Legacy/Saves", "Global confirmation maps the legacy save path");
assert(global.cn_battle_net_path === "", "Global confirmation does not reuse deprecated Battle.net auth");

const current = config({ cn_game_path: "C:/Current/D2R-CN", global_game_path: "E:/Current/D2R" });
const kept = resolveLegacyPathMigration(current, "keep");
assert(kept.cn_game_path === current.cn_game_path && kept.global_game_path === current.global_game_path,
  "keeping current paths only clears the migration marker");
assert(original.legacy_path_migration !== null, "resolving a migration does not mutate the loaded config");
