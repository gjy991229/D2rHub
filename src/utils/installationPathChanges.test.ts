import type { GlobalConfig } from "../store/types";
import {
  hasValidEditionPathPairs,
  installationPathEditsAreInvalid,
  installationPathsChanged,
} from "./installationPathChanges";

function config(overrides: Partial<GlobalConfig> = {}): GlobalConfig {
  return {
    version: 5,
    cn_battle_net_path: "",
    cn_game_path: "C:/D2R-CN",
    cn_saved_games_path: "C:/Saves-CN",
    global_game_path: "",
    global_saved_games_path: "",
    program_data_agent_path: "C:/ProgramData/Battle.net/Agent",
    app_data_roaming_bnet_path: "C:/Users/Test/AppData/Roaming/Battle.net",
    accounts_dir: "C:/D2RHub/accounts",
    first_run_complete: true,
    browser_path: "C:/Browser/browser.exe",
    browser_type: "edge",
    enable_overlay: true,
    enable_tz_overlay: true,
    enable_stats_overlay: true,
    theme: "light",
    theme_overlay: "light",
    auto_close_browser: true,
    enable_auto_update: true,
    first_launch: false,
    ocr_enabled: false,
    ocr_target_account: "",
    ocr_ch_b_profiles_json: "",
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
  const unavailableInstall = config({
    cn_game_path: "Z:/offline/D2R-CN",
    cn_saved_games_path: "Z:/offline/Saves-CN",
  });
  const appearanceEdit = config({
    ...unavailableInstall,
    theme: "onyx",
    overlay_opacity: 78,
  });
  assert(
    !installationPathsChanged(unavailableInstall, appearanceEdit),
    "appearance-only edits do not request installation-path validation",
  );
  assert(
    !installationPathEditsAreInvalid(unavailableInstall, appearanceEdit),
    "appearance-only edits remain saveable when an existing install is offline",
  );

  const auxiliaryPathEdit = config({
    ...unavailableInstall,
    browser_path: "D:/Browser/msedge.exe",
    program_data_agent_path: "D:/Battle.net/Agent",
    app_data_roaming_bnet_path: "D:/Roaming/Battle.net",
  });
  assert(
    !installationPathsChanged(unavailableInstall, auxiliaryPathEdit),
    "browser and Battle.net support paths do not count as game-installation edits",
  );
  assert(
    !installationPathEditsAreInvalid(unavailableInstall, auxiliaryPathEdit),
    "support-path edits remain saveable when an existing game install is offline",
  );

  const partialPathEdit = config({ cn_game_path: "D:/D2R-CN", cn_saved_games_path: "" });
  assert(
    installationPathsChanged(unavailableInstall, partialPathEdit),
    "editing a game path is detected as an installation change",
  );
  assert(
    !installationPathEditsAreInvalid(unavailableInstall, partialPathEdit),
    "a valid game path remains launchable without an optional save path",
  );

  const noGamePath = config({ cn_game_path: "", cn_saved_games_path: "D:/Saves-CN" });
  assert(
    installationPathEditsAreInvalid(unavailableInstall, noGamePath),
    "save paths alone cannot satisfy the minimum launch configuration",
  );

  const completeCnWithPartialGlobal = config({
    global_game_path: "D:/D2R-Global",
    global_saved_games_path: "",
  });
  assert(
    hasValidEditionPathPairs(completeCnWithPartialGlobal),
    "a configured CN game keeps global settings valid when Global has no save path",
  );
  assert(
    !installationPathEditsAreInvalid(unavailableInstall, completeCnWithPartialGlobal),
    "an optional Global save path does not block saving a configured CN edition",
  );

  const completeGlobalWithPartialCn = config({
    cn_game_path: "D:/D2R-CN",
    cn_saved_games_path: "",
    global_game_path: "D:/D2R-Global",
    global_saved_games_path: "D:/Saves-Global",
  });
  assert(
    hasValidEditionPathPairs(completeGlobalWithPartialCn),
    "a configured Global game keeps global settings valid when CN has no save path",
  );
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
