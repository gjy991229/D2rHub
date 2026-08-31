import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { mockConvertFileSrc, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { AccountMeta, GlobalConfig } from "./store/types";
import "./styles/globals.css";
import "./styles/visualAudit.css";

type Surface =
  | "main"
  | "setup"
  | "settings"
  | "account-init"
  | "about"
  | "overlay"
  | "stats-overlay"
  | "bongo";

type SettingsMap = Record<string, unknown>;

const params = new URLSearchParams(window.location.search);
const surface = ((params.get("surface") as Surface | null) || "main") as Surface;
const requestedTheme = params.get("theme") === "dark" ? "onyx" : "light";
const auditFrame = params.get("frame");
const settingsState = params.get("settingsState");
const settingsTab = params.get("settingsTab");
const audioModState = params.get("audioModState");
const seedSampleDrops = params.get("drops") === "sample";
const seedManyAccounts = params.get("accounts") === "many";
const currentWindowLabel =
  surface === "overlay"
    ? "overlay"
    : surface === "stats-overlay"
      ? "stats-overlay"
      : surface === "bongo"
        ? "bongo-cat"
        : "main";

document.documentElement.setAttribute("data-theme", requestedTheme);
if (auditFrame) {
  document.documentElement.setAttribute("data-visual-audit-frame", auditFrame);
}
if (surface === "overlay" && auditFrame?.startsWith("mini-")) {
  localStorage.setItem("d2rhub-information-overlay-mode", "mini");
  localStorage.setItem(
    "d2rhub-information-overlay-mini-size",
    JSON.stringify({ width: 260, height: 28 }),
  );
}

mockWindows(currentWindowLabel, "main", "overlay", "stats-overlay", "bongo-cat");
mockConvertFileSrc("windows");

type TauriInternals = Record<string, unknown> & {
  metadata?: {
    currentWindow: { label: string };
    currentWebview: { windowLabel: string; label: string };
  };
};

const tauriInternalsState: TauriInternals = {};

function applyWindowMetadata(target: TauriInternals) {
  target.metadata = {
    currentWindow: { label: currentWindowLabel },
    currentWebview: { windowLabel: currentWindowLabel, label: currentWindowLabel },
  };
}

function installWindowMetadata() {
  const tauriWindow = window as Window & { __TAURI_INTERNALS__?: TauriInternals };
  if (tauriWindow.__TAURI_INTERNALS__) {
    Object.assign(tauriInternalsState, tauriWindow.__TAURI_INTERNALS__);
  }
  applyWindowMetadata(tauriInternalsState);

  try {
    Object.defineProperty(tauriWindow, "__TAURI_INTERNALS__", {
      configurable: true,
      get() {
        applyWindowMetadata(tauriInternalsState);
        return tauriInternalsState;
      },
      set(next) {
        if (next && typeof next === "object") {
          Object.assign(tauriInternalsState, next);
        }
        applyWindowMetadata(tauriInternalsState);
      },
    });
  } catch {
    tauriWindow.__TAURI_INTERNALS__ = tauriInternalsState;
  }
}

installWindowMetadata();

const accounts: AccountMeta[] = [
  {
    id: "sorc-01",
    display_name: "Ladder Sorc",
    mod_args: "-mod highres -txt",
    mod_list: ["-mod highres -txt", "-direct -txt"],
    created_at: "2026-06-15T10:00:00Z",
    last_launched_at: "2026-06-30T20:14:00Z",
    last_reset_at: null,
    initialized: true,
    order: 1,
    is_running: true,
    running_pid: 28420,
    window_x: 12,
    window_y: 32,
    position_presets: [
      { id: "top-left", name: "左上", x: 12, y: 32 },
      { id: "right", name: "右侧", x: 980, y: 34 },
    ],
    active_position_id: "top-left",
    auth_mode: "token",
    region: "KR",
    language: "zhCN",
    voicelanguage: "enUS",
    has_customized_settings: true,
  },
  {
    id: "barb-02",
    display_name: "Trav Barb",
    mod_args: "",
    mod_list: [],
    created_at: "2026-06-12T10:00:00Z",
    last_launched_at: "2026-06-30T19:40:00Z",
    last_reset_at: "2026-06-28T12:00:00Z",
    initialized: true,
    order: 2,
    is_running: true,
    running_pid: 28421,
    window_x: 980,
    window_y: 34,
    position_presets: [{ id: "right", name: "右侧", x: 980, y: 34 }],
    active_position_id: "right",
    auth_mode: "token",
    region: "NA",
    language: "enUS",
    voicelanguage: "enUS",
    has_customized_settings: false,
  },
  {
    id: "pala-03",
    display_name: "Smiter",
    mod_args: "-w",
    mod_list: ["-w"],
    created_at: "2026-06-11T08:20:00Z",
    last_launched_at: null,
    last_reset_at: null,
    initialized: true,
    order: 3,
    is_running: true,
    running_pid: 28422,
    window_x: null,
    window_y: null,
    position_presets: [],
    active_position_id: null,
    auth_mode: "browser",
    region: "EU",
    language: "zhTW",
    voicelanguage: "enUS",
    has_customized_settings: true,
  },
  {
    id: "new-04",
    display_name: "Need Init",
    mod_args: "",
    mod_list: [],
    created_at: "2026-06-26T08:20:00Z",
    last_launched_at: null,
    last_reset_at: null,
    initialized: false,
    order: 4,
    is_running: false,
    running_pid: null,
    window_x: null,
    window_y: null,
    position_presets: [],
    active_position_id: null,
    auth_mode: null,
    region: null,
    language: null,
    voicelanguage: null,
    has_customized_settings: false,
  },
];

if (seedManyAccounts) {
  for (let index = 5; index <= 20; index += 1) {
    accounts.push({
      ...accounts[0],
      id: `bench-${String(index).padStart(2, "0")}`,
      display_name: `Bench ${String(index).padStart(2, "0")}`,
      order: index,
      is_running: false,
      running_pid: null,
      last_launched_at: index % 2 === 0 ? "2026-06-29T18:20:00Z" : null,
      active_position_id: null,
      position_presets: [],
    });
  }
}

const baseConfig: GlobalConfig = {
  version: 9,
  cn_battle_net_path: "C:\\Program Files (x86)\\Battle.net CN\\Battle.net.exe",
  cn_game_path: "D:\\Games\\Diablo II Resurrected CN",
  cn_saved_games_path: "C:\\Users\\Player\\Saved Games\\Diablo II Resurrected (CN)",
  global_game_path: "D:\\Games\\Diablo II Resurrected",
  global_saved_games_path: "C:\\Users\\Player\\Saved Games\\Diablo II Resurrected",
  program_data_agent_path: "C:\\ProgramData\\Battle.net\\Agent",
  app_data_roaming_bnet_path: "C:\\Users\\Player\\AppData\\Roaming\\Battle.net",
  accounts_dir: "D:\\D2RHub\\accounts",
  first_run_complete: surface !== "setup",
  browser_path: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  browser_type: "chrome",
  enable_bongo_cat: true,
  bongo_cat_chatterbox: true,
  bongo_cat_scale: 1,
  bongo_cat_skin: "original",
  bongo_cat_unlocked_skins: ["original"],
  enable_overlay: true,
  enable_tz_overlay: true,
  enable_stats_overlay: true,
  theme: requestedTheme,
  theme_overlay: requestedTheme,
  auto_close_browser: true,
  enable_auto_update: true,
  first_launch: false,
  rune_audio_enabled: true,
  rune_audio_target_account: "sorc-01",
  rune_audio_detection_threshold: 0.58,
  rune_audio_tracked_categories: ["runes", "gems", "charms", "jewels", "keys", "organs", "essences"],
  rune_audio_min_rune_number: 20,
  rune_audio_min_gem_level: 4,
  rune_audio_tracked_charm_codes: ["cm1", "cm3"],
  shortcut_bindings_json: JSON.stringify({ "1": "Ctrl+Alt+1", "2": "Ctrl+Alt+2" }),
  overlay_opacity: 94,
  main_opacity: 96,
  font_scale: "default",
  app_language: "zhCN",
  agent_mode: 1,
  agent_delay_secs: 4,
  agent_threshold: 2,
  launch_groups: [
    {
      id: "farm-core",
      name: "日常 Farm",
      account_ids: ["sorc-01", "barb-02"],
      members: [
        {
          account_id: "sorc-01",
          mod_args: "-direct -txt",
          position_preset_id: "right",
          position_configured: true,
          graphics_configured: true,
          resolution: "2560x1440",
          fps: 144,
        },
        {
          account_id: "barb-02",
          mod_args: "",
          position_preset_id: "right",
          position_configured: true,
          graphics_configured: true,
          resolution: "1920x1080",
          fps: 60,
        },
      ],
    },
    { id: "uber-team", name: "火炬队", account_ids: ["sorc-01", "pala-03"] },
  ],
  favorite_launch_group_ids: ["farm-core"],
};

let persistedGlobalConfig: GlobalConfig = { ...baseConfig };

const accountSettings: SettingsMap = {
  "Window Mode": 0,
  "Screen Resolution (Windowed)": "1280x720",
  "Resolution Scale": 100,
  "Texture Quality": 2,
  "Shadow Quality": 2,
  "Anti-Aliasing": 1,
  "VFX Lighting Quality": 2,
  "Sound": 1,
  "Music Volume": 65,
  "Master Volume": 70,
  "Automap Size": 0,
  "Automap Opacity": 70,
  "Show Item Names": 1,
  "Quick Cast Skills": 1,
};

let foregroundWindowTitle = "D2R - Ladder Sorc";

function installIpcMock() {
  mockIPC((cmd, payload) => {
    if (cmd.startsWith("plugin:window|")) {
      if (cmd.endsWith("|outer_size") || cmd.endsWith("|inner_size")) {
        return currentWindowLabel === "overlay"
          ? auditFrame?.startsWith("mini-")
            ? { width: 260, height: 28 }
              : { width: 280, height: 250 }
          : currentWindowLabel === "stats-overlay"
            ? { width: 280, height: 300 }
            : currentWindowLabel === "bongo-cat"
            ? { width: 240, height: 400 }
            : { width: 1120, height: 720 };
      }
      if (cmd.endsWith("|outer_position") || cmd.endsWith("|inner_position")) {
        return { x: 40, y: 40 };
      }
      if (cmd.endsWith("|scale_factor")) return 1;
      if (cmd.endsWith("|is_visible")) return true;
      if (cmd.endsWith("|is_focused")) return true;
      if (cmd.endsWith("|current_monitor") || cmd.endsWith("|primary_monitor")) {
        return {
          name: "Visual Audit",
          size: { width: 1920, height: 1080 },
          position: { x: 0, y: 0 },
          workArea: {
            size: { width: 1920, height: 1040 },
            position: { x: 0, y: 0 },
          },
          scaleFactor: 1,
        };
      }
      if (cmd.endsWith("|available_monitors")) return [];
      return null;
    }

    if (cmd.startsWith("plugin:webview|") || cmd.startsWith("plugin:webviewwindow|")) {
      const payloadRecord =
        payload && typeof payload === "object" && !Array.isArray(payload)
          ? (payload as Record<string, unknown>)
          : {};
      if (cmd.endsWith("|get_by_label")) return { label: String(payloadRecord.label || "main") };
      return null;
    }

    switch (cmd) {
      case "get_global_config":
        return { ...persistedGlobalConfig, first_run_complete: surface !== "setup" };
      case "save_global_config": {
        persistedGlobalConfig = (payload as { config?: GlobalConfig } | undefined)?.config
          ?? persistedGlobalConfig;
        return persistedGlobalConfig;
      }
      case "patch_global_config": {
        const patch = (payload as { patch?: Partial<GlobalConfig> } | undefined)?.patch ?? {};
        persistedGlobalConfig = { ...persistedGlobalConfig, ...patch };
        return persistedGlobalConfig;
      }
      case "list_accounts":
        return accounts;
      case "refresh_account_running_state":
        return accounts.filter((account) => account.is_running).map((account) => account.id);
      case "get_d2r_window_titles":
        return ["D2R - Ladder Sorc", "D2R - Trav Barb"];
      case "get_foreground_window_title":
        return foregroundWindowTitle;
      case "bring_window_by_title_to_front":
        foregroundWindowTitle = String(
          (payload as Record<string, unknown> | undefined)?.windowTitle || foregroundWindowTitle,
        );
        return true;
      case "get_account_settings":
        if (settingsState === "missing") {
          throw new Error("系统 Settings.json 不存在");
        }
        return accountSettings;
      case "snapshot_system_settings_to_account":
        return accountSettings;
      case "get_scene_stats":
        return { avg_time: 73.4, total_runs: 218 };
      case "get_audio_mod_setup_state":
        if (audioModState === "legacy") {
          return {
            account_id: "sorc-01",
            account_name: "Ladder Sorc",
            current_mod_name: "jcy-tz",
            launch_arguments: "-mod jcy-tz -txt -assettestmode 1",
            has_txt: true,
            ready: true,
            update_required: true,
            recipe_version: null,
            required_recipe_version: 2,
            build_mode: "augment",
            source_mod_name: null,
            reason_code: "update_available",
            message: "旧版识别 Mod 仍可使用；更新后可获得即时恐怖区域识别",
            installed_mods: [
              { name: "jcy", audio_ready: false, update_required: false, source_eligible: true },
              { name: "jcy-tz", audio_ready: true, update_required: true, source_eligible: false },
            ],
            running_pid: 28420,
            session_verified: true,
            active_session_ready: true,
            active_session_update_required: true,
            restart_required: false,
          };
        }
        return {
          account_id: "sorc-01",
          account_name: "Ladder Sorc",
          current_mod_name: null,
          launch_arguments: "-w",
          has_txt: false,
          ready: false,
          update_required: false,
          recipe_version: null,
          required_recipe_version: 2,
          build_mode: null,
          source_mod_name: null,
          reason_code: "missing_mod",
          message: "当前账号还没有使用识别 Mod",
          installed_mods: [
            { name: "ReMoDDeD", audio_ready: false, update_required: false, source_eligible: true },
            { name: "VanillaPlus", audio_ready: false, update_required: false, source_eligible: true },
          ],
          running_pid: null,
          session_verified: false,
          active_session_ready: null,
          active_session_update_required: null,
          restart_required: false,
        };
      case "get_terror_zone_snapshot":
        return {
          current: {
            start_time: 1782916800,
            end_time: 1782920400,
            display_time: "20:00-21:00",
            location_name: "Chaos Sanctuary",
            location_detail: "Act IV · Chaos Sanctuary",
            tier_exp: "A",
            tier_loot: "S",
            immunities: [
              { code: "f", label: "火", color: "#ef4444" },
              { code: "l", label: "电", color: "#facc15" },
            ],
          },
          next: {
            start_time: 1782920400,
            end_time: 1782924000,
            display_time: "21:00-22:00",
            location_name: "Worldstone Keep",
            location_detail: "Act V · Worldstone Keep",
            tier_exp: "S",
            tier_loot: "A",
            immunities: [
              { code: "c", label: "冰", color: "#38bdf8" },
              { code: "p", label: "毒", color: "#86efac" },
            ],
          },
        };
      case "load_overlay_geometry":
        return { x: 60, y: 60, width: 240, height: 320 };
      case "detect_saved_games_path":
        return baseConfig.cn_saved_games_path;
      case "detect_global_saved_games_path":
        return baseConfig.global_saved_games_path;
      case "detect_program_data_agent_path":
        return baseConfig.program_data_agent_path;
      case "detect_app_data_roaming_bnet_path":
        return baseConfig.app_data_roaming_bnet_path;
      case "detect_browser_path":
        return [baseConfig.browser_path, baseConfig.browser_type];
      case "detect_browser_path_by_type":
        return baseConfig.browser_path;
      case "check_path_exists":
        return true;
      case "check_saved_games_settings":
        return false;
      case "get_app_version":
        return "0.7.2";
      case "check_cloud_version":
        return { has_update: false, version: "0.7.2", download_url: "" };
      case "create_account":
        return "audit-created-account";
      case "launch_accounts":
      case "launch_battle_net_only":
        return [];
      default:
        return null;
    }
  }, { shouldMockEvents: true });
}

installIpcMock();
installWindowMetadata();

async function primeStores() {
  const [{ useGlobalConfig }, { useAccounts }, { useTheme }, { useStats }] = await Promise.all([
    import("./store/globalConfig"),
    import("./store/accounts"),
    import("./store/theme"),
    import("./store/stats"),
  ]);

  useGlobalConfig.setState({
    config: { ...baseConfig, first_run_complete: surface !== "setup" },
    initialLoading: false,
    saving: false,
    error: null,
  });
  useAccounts.setState({ accounts, loading: false, error: null });
  useTheme.setState({ theme: requestedTheme });
  if (surface === "stats-overlay" && seedSampleDrops) {
    useStats.setState({
      currentScene: "混沌魔殿",
      currentRunName: "混沌魔殿",
      currentRunKey: "area:108",
      isTiming: true,
      timerStart: Date.now() - 73_400,
      elapsedMs: 73_400,
      dbAvgTime: 81.2,
      dbTotalRuns: 218,
      sessionRuns: { "混沌魔殿": 6 },
      currentDrops: [
        { kind: "rune", telemetryId: 15, itemCode: "r15", category: "runes", name: "海尔", nameEn: "Hel", runeNumber: 15, screenshotPath: null },
        { kind: "rune", telemetryId: 15, itemCode: "r15", category: "runes", name: "海尔", nameEn: "Hel", runeNumber: 15, screenshotPath: null },
        { kind: "rune", telemetryId: 30, itemCode: "r30", category: "runes", name: "贝", nameEn: "Ber", runeNumber: 30, screenshotPath: null },
        { kind: "item", telemetryId: 40, itemCode: "pk1", category: "keys", name: "恐惧之钥", nameEn: "Key of Terror", runeNumber: null, screenshotPath: null },
        { kind: "item", telemetryId: 40, itemCode: "pk1", category: "keys", name: "恐惧之钥", nameEn: "Key of Terror", runeNumber: null, screenshotPath: null },
        { kind: "item", telemetryId: 36, itemCode: "cm1", category: "charms", name: "小型护身符", nameEn: "Small Charm", runeNumber: null, screenshotPath: null },
      ],
    });
  }
  document.documentElement.setAttribute("data-theme", requestedTheme);
  return useStats;
}

function AuditRuntime() {
  const [content, setContent] = useState<React.ReactNode>(
    <div className="visual-audit-loading">Loading visual audit surface...</div>,
  );

  useEffect(() => {
    let cancelled = false;

    async function loadSurface() {
      try {
        const statsStore = await primeStores();

        if (surface === "overlay" || surface === "stats-overlay") {
          const { Overlay } = await import("./pages/Overlay");
          if (!cancelled) {
            setContent(<Overlay />);
            if (surface === "stats-overlay" && seedSampleDrops) {
              const sampleDrops = statsStore.getState().currentDrops;
              window.requestAnimationFrame(() => {
                statsStore.setState({ currentDrops: [] });
                window.setTimeout(() => statsStore.setState({ currentDrops: sampleDrops }), 80);
              });
            }
          }
          return;
        }

        if (surface === "bongo") {
          const { BongoCatWindow } = await import("./pages/BongoCatWindow");
          if (!cancelled) setContent(<BongoCatWindow />);
          return;
        }

        if (surface === "settings") {
          const [{ AppShell }, { SettingsCenter }, { ToastContainer }] = await Promise.all([
            import("./components/layout/AppShell"),
            import("./components/config/SettingsCenter"),
            import("./components/ui/Toast"),
          ]);
          if (!cancelled) {
            setContent(
              <AppShell>
                <div className="flex-1" />
                <SettingsCenter
                  open
                  onClose={() => {}}
                  onReconfigure={() => {}}
                  onInitializeAccount={() => {}}
                  initialTab={settingsTab}
                />
                <ToastContainer />
              </AppShell>,
            );
          }
          return;
        }

        if (surface === "account-init") {
          const [{ AppShell }, { AccountInitDialog }, { ToastContainer }] = await Promise.all([
            import("./components/layout/AppShell"),
            import("./components/accounts/AccountInitDialog"),
            import("./components/ui/Toast"),
          ]);
          if (!cancelled) {
            setContent(
              <AppShell>
                <div className="flex-1" />
                <AccountInitDialog open onClose={() => {}} onDone={() => {}} />
                <ToastContainer />
              </AppShell>,
            );
          }
          return;
        }

        if (surface === "about") {
          const [{ AppShell }, { AboutModal }] = await Promise.all([
            import("./components/layout/AppShell"),
            import("./pages/AboutModal"),
          ]);
          if (!cancelled) {
            setContent(
              <AppShell>
                <div className="flex-1" />
                <AboutModal open onClose={() => {}} />
              </AppShell>,
            );
          }
          return;
        }

        const { default: App } = await import("./App");
        if (!cancelled) setContent(<App />);
      } catch (error) {
        if (!cancelled) {
          setContent(<div className="visual-audit-error">{String(error)}</div>);
        }
      }
    }

    void loadSurface();
    return () => {
      cancelled = true;
    };
  }, []);

  return <>{content}</>;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AuditRuntime />
  </React.StrictMode>,
);
