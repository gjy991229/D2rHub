import type { GlobalConfig } from "../store/types";
import {
  INTERNATIONAL_ACCOUNT_REGIONS,
  accountRegionLabel,
  firstConfiguredRegion,
  hasConfiguredPathsForRegion,
  isInternationalRegion,
  requiresTokenMigration,
} from "./regionPaths";

function config(overrides: Partial<GlobalConfig>): GlobalConfig {
  return {
    version: 6,
    cn_battle_net_path: "",
    cn_game_path: "",
    cn_saved_games_path: "",
    global_game_path: "",
    global_saved_games_path: "",
    program_data_agent_path: "",
    app_data_roaming_bnet_path: "",
    accounts_dir: "",
    first_run_complete: true,
    browser_path: "",
    browser_type: "",
    enable_overlay: true,
    enable_tz_overlay: true,
    enable_stats_overlay: true,
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
  assert(
    JSON.stringify(INTERNATIONAL_ACCOUNT_REGIONS) === JSON.stringify(["KR", "NA", "EU"]),
    "the inline international switcher exposes only Asia, Americas, and Europe",
  );
  assert(accountRegionLabel("KR") === "亚服", "international region labels use the dashboard vocabulary");
  assert(accountRegionLabel("Global") === "亚服", "legacy Global accounts preserve their existing Asia launch behavior");
  assert(accountRegionLabel("Asia") === "亚服", "legacy Asia aliases normalize to KR");
  assert(accountRegionLabel("US") === "美服", "legacy US aliases normalize to NA");
  assert(accountRegionLabel("Americas") === "美服", "legacy Americas aliases normalize to NA");
  assert(accountRegionLabel("Europe") === "欧服", "legacy Europe aliases normalize to EU");
  assert(isInternationalRegion("US"), "legacy US accounts expose the international region switcher");
  const cnOnly = config({ cn_battle_net_path: "C:/Battle.net-CN/Battle.net.exe", cn_game_path: "C:/D2R-CN", cn_saved_games_path: "C:/Saves-CN" });
  assert(hasConfiguredPathsForRegion(cnOnly, "CN", "bnet"), "CN Battle.net accounts are enabled by a complete CN path group");
  assert(!hasConfiguredPathsForRegion(cnOnly, "NA", "bnet"), "global accounts stay disabled without a global path group");
  assert(firstConfiguredRegion(cnOnly, "bnet") === "CN", "new Battle.net accounts default to CN when only CN is configured");

  const globalOnly = config({
    global_game_path: "D:/D2R-Global",
    global_saved_games_path: "D:/Saves-Global",
  });
  assert(!hasConfiguredPathsForRegion(globalOnly, "CN", "bnet"), "CN accounts stay disabled without a CN path group");
  assert(!hasConfiguredPathsForRegion(globalOnly, "KR", "bnet"), "KR never enables Battle.net authentication");
  assert(!hasConfiguredPathsForRegion(globalOnly, "NA", "bnet"), "NA never enables Battle.net authentication");
  assert(!hasConfiguredPathsForRegion(globalOnly, "EU", "bnet"), "EU never enables Battle.net authentication");
  assert(firstConfiguredRegion(globalOnly, "bnet") === null, "Battle.net account defaults never select a global region");
  assert(!hasConfiguredPathsForRegion(globalOnly, "unknown", "bnet"), "unknown regions never fall through to the global installation");
  assert(!hasConfiguredPathsForRegion(globalOnly, undefined, "bnet"), "missing regions never silently default to an installation");

  const partial = config({ global_game_path: "D:/D2R-Global" });
  assert(hasConfiguredPathsForRegion(partial, "EU", "token"), "a game path without a save path still enables Token account creation");

  const tokenOnly = config({
    global_game_path: "D:/D2R-Global",
    global_saved_games_path: "D:/Saves-Global",
  });
  assert(hasConfiguredPathsForRegion(tokenOnly, "NA", "token"), "Token accounts do not require Battle.net.exe");
  assert(!hasConfiguredPathsForRegion(tokenOnly, "NA", "bnet"), "Battle.net accounts still require Battle.net.exe");
  assert(firstConfiguredRegion(tokenOnly, "token") === "KR", "Token account defaults use game path availability");
  assert(requiresTokenMigration("bnet", "EU"), "legacy global Battle.net accounts require Token migration");
  assert(requiresTokenMigration("bnet", undefined, globalOnly), "region-less legacy accounts infer global migration when only global paths are complete");
  assert(!requiresTokenMigration("bnet", undefined, cnOnly), "region-less legacy accounts remain CN-compatible when only CN paths are complete");
  assert(!requiresTokenMigration("bnet", undefined, config({
    cn_game_path: "C:/D2R-CN",
    cn_saved_games_path: "C:/Saves-CN",
    global_game_path: "D:/D2R-Global",
    global_saved_games_path: "D:/Saves-Global",
  })), "region-less legacy accounts remain ambiguous when both editions are complete");
  assert(!requiresTokenMigration("bnet", "CN"), "CN Battle.net compatibility accounts remain supported");
  assert(!requiresTokenMigration("token", "NA"), "global Token accounts remain launchable");
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
