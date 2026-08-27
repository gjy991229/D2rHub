import type { GlobalConfig } from "../store/types";
import {
  firstConfiguredRegion,
  hasConfiguredPathsForRegion,
} from "./regionPaths";

function config(overrides: Partial<GlobalConfig>): GlobalConfig {
  return {
    version: 4,
    cn_battle_net_path: "",
    cn_game_path: "",
    cn_saved_games_path: "",
    global_battle_net_path: "",
    global_game_path: "",
    global_saved_games_path: "",
    program_data_agent_path: "",
    app_data_roaming_bnet_path: "",
    accounts_dir: "",
    first_run_complete: true,
    browser_path: "",
    browser_type: "",
    enable_overlay: true,
    theme: "light",
    theme_overlay: "light",
    auto_close_browser: true,
    enable_auto_update: true,
    first_launch: false,
    rune_audio_enabled: false,
    rune_audio_target_account: "",
    rune_audio_detection_threshold: 0.58,
    rune_audio_tracked_categories: ["runes"],
    shortcut_bindings_json: "{}",
    overlay_opacity: 95,
    main_opacity: 95,
    font_scale: "default",
    enable_bongo_cat: true,
    bongo_cat_chatterbox: true,
    bongo_cat_scale: 1,
    bongo_cat_skin: "original",
    bongo_cat_unlocked_skins: ["original"],
    ...overrides,
  };
}

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

export function runTests() {
  const cnOnly = config({ cn_battle_net_path: "C:/Battle.net-CN/Battle.net.exe", cn_game_path: "C:/D2R-CN", cn_saved_games_path: "C:/Saves-CN" });
  assert(hasConfiguredPathsForRegion(cnOnly, "CN", "bnet"), "CN Battle.net accounts are enabled by a complete CN path group");
  assert(!hasConfiguredPathsForRegion(cnOnly, "NA", "bnet"), "global accounts stay disabled without a global path group");
  assert(firstConfiguredRegion(cnOnly, "bnet") === "CN", "new Battle.net accounts default to CN when only CN is configured");

  const globalOnly = config({
    global_battle_net_path: "D:/Battle.net-Global/Battle.net.exe",
    global_game_path: "D:/D2R-Global",
    global_saved_games_path: "D:/Saves-Global",
  });
  assert(!hasConfiguredPathsForRegion(globalOnly, "CN", "bnet"), "CN accounts stay disabled without a CN path group");
  assert(hasConfiguredPathsForRegion(globalOnly, "KR", "bnet"), "KR uses the configured global path group");
  assert(hasConfiguredPathsForRegion(globalOnly, "NA", "bnet"), "NA uses the configured global path group");
  assert(hasConfiguredPathsForRegion(globalOnly, "EU", "bnet"), "EU uses the configured global path group");
  assert(firstConfiguredRegion(globalOnly, "bnet") === "KR", "new Battle.net accounts default to a global region when only global is configured");
  assert(!hasConfiguredPathsForRegion(globalOnly, "unknown", "bnet"), "unknown regions never fall through to the global installation");
  assert(!hasConfiguredPathsForRegion(globalOnly, undefined, "bnet"), "missing regions never silently default to an installation");

  const partial = config({ global_game_path: "D:/D2R-Global" });
  assert(!hasConfiguredPathsForRegion(partial, "EU", "token"), "a game path without a save path never enables Token account creation");

  const tokenOnly = config({
    global_game_path: "D:/D2R-Global",
    global_saved_games_path: "D:/Saves-Global",
  });
  assert(hasConfiguredPathsForRegion(tokenOnly, "NA", "token"), "Token accounts do not require Battle.net.exe");
  assert(!hasConfiguredPathsForRegion(tokenOnly, "NA", "bnet"), "Battle.net accounts still require Battle.net.exe");
  assert(firstConfiguredRegion(tokenOnly, "token") === "KR", "Token account defaults use game and save path availability");
}

const g = globalThis as any;
if (typeof g.process !== "undefined" && typeof g.process.argv !== "undefined") {
  try {
    runTests();
  } catch (error) {
    console.error(error);
    g.process.exit(1);
  }
}
