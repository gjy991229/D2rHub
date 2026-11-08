import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { version as appVersion } from "../package.json";
import { mockConvertFileSrc, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { AccountMeta, GlobalConfig, LayoutMonitor, ModCapsulePool } from "./store/types";
import { presetSlots } from "./features/windowLayouts/model";
import { recognitionSourceId } from "./utils/recognitionSource";
import { normalizeTheme } from "./store/themeCatalog";
import { PET_ITEMS, PET_ITEM_BY_ID } from "./features/pet/catalog";
import type { PetAction, PetOutcome } from "./features/pet/types";
import { APPLICATION_DISCLOSURE_REVISION } from "./features/disclosures/disclosureStorage";
import waypointCatalog from "../src-tauri/resources/waypoint-catalog.json";
import type {
  RoomAutomationConfigSnapshot,
  RoomAutomationWorkflowStatus,
} from "./features/roomAutomation/types";
import "./styles/globals.css";
import "./styles/visualAudit.css";

type Surface =
  | "mod-resources"
  | "software-update"
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
// Isolated browser-only fixture: start at the dashboard when reviewing batch controls.
let acceptedDisclosureRevision: number | null = params.get("batch") === "1"
  ? APPLICATION_DISCLOSURE_REVISION : null;
let auditVisionSnapshot = { generation: 0, config: { diagnostic_enabled: false, chat_send: false, chat_follow: false, chat_account_id: "", poll_ms: 2000, chat_roi: { x: 0, y: 0.4, width: 0.31, height: 0.4 }, area_enabled: true, area_roi: { x: 0.4, y: 0.2, width: 0.2, height: 0.09 } } };
const surface = ((params.get("surface") as Surface | null) || "main") as Surface;
const requestedTheme = normalizeTheme(params.get("theme") === "dark" ? "onyx" : params.get("theme"));
const requestedFont = ["small", "large"].includes(params.get("font") || "") ? params.get("font")! : "default";
const requestedLanguage = params.get("lang") === "en" ? "en-US" : "zh-CN";
const auditFrame = params.get("frame");
const settingsState = params.get("settingsState");
const settingsTab = params.get("settingsTab");
const audioModState = params.get("audioModState");
const seedSampleDrops = params.get("drops") === "sample";
const seedManyAccounts = params.get("accounts") === "many";
const seedSampleTasks = params.get("tasks") === "sample";
const currentWindowLabel =
  surface === "overlay"
    ? "overlay"
    : surface === "stats-overlay"
      ? "stats-overlay"
      : surface === "bongo"
        ? "bongo-cat"
        : "main";

const petPreview: PetOutcome = {
  snapshot: { generation: 1, revision: 1, wardrobe: {
    owned: PET_ITEMS.filter(item => item.source === "starter").map(item => item.id),
    equipped: {}, presets: [{}, {}, {}], seconds: 3660, inputs: 3000, days: 3,
    last_day: "2026-09-29", daily_rolls: 0, roll_seconds: 120, misses: 2, fragments: 14, tone: "mixed",
  } }, rewards: [],
};

document.documentElement.setAttribute("data-theme", requestedTheme);
document.documentElement.dataset.fontScale = requestedFont;
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
if (surface === "stats-overlay" && auditFrame === "mini-stats-overlay") {
  localStorage.setItem("d2rhub-statistics-overlay-mode", "mini");
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
if (params.get("batch") === "1") {
  accounts.slice(0, 3).forEach((account, index) => {
    account.auth_mode = "token";
    account.region = "KR";
    account.is_running = index < 2;
  });
  accounts.push({ ...accounts[2], id: "batch-idle", display_name: "Idle Druid", order: 5, is_running: false });
}

const layoutDisplaySize = params.get("display") === "4k" ? [3840, 2160] : params.get("display") === "1080p" ? [1920, 1080] : [2560, 1440];
const layoutMonitors: LayoutMonitor[] = [{ id: "DISPLAY1", name: "DISPLAY1", primary: true, scale_factor: 1.5,
  bounds: { x: 0, y: 0, width: layoutDisplaySize[0], height: layoutDisplaySize[1] },
  work_area: { x: 0, y: 0, width: layoutDisplaySize[0], height: layoutDisplaySize[1] - 48 } }];
if (params.get("monitors") === "dual") layoutMonitors.push({ id: "DISPLAY2", name: "DISPLAY2", primary: false, scale_factor: 1,
  bounds: { x: -1920, y: 200, width: 1920, height: 1080 }, work_area: { x: -1920, y: 200, width: 1920, height: 1040 } });
// Browser fixture mirrors physical, visible-only frame geometry at each DPI.
for (const monitor of layoutMonitors) {
  const scale = monitor.scale_factor;
  monitor.frame = { dpi: Math.round(96 * scale), style: 0, ex_style: 0,
    visible: { left: Math.round(scale), top: Math.round(31 * scale), right: Math.round(scale), bottom: Math.round(scale) },
    invisible: { left: Math.round(7 * scale), top: 0, right: Math.round(7 * scale), bottom: Math.round(7 * scale) } };
}
const baseConfig: GlobalConfig = {
  version: 11,
  cn_battle_net_path: "C:\\Program Files (x86)\\Battle.net CN\\Battle.net.exe",
  cn_game_path: "D:\\Games\\Diablo II Resurrected CN",
  cn_saved_games_path: "C:\\Users\\Player\\Saved Games\\Diablo II Resurrected (CN)",
  global_game_path: "D:\\Games\\Diablo II Resurrected",
  global_saved_games_path: "C:\\Users\\Player\\Saved Games\\Diablo II Resurrected",
  program_data_agent_path: "C:\\ProgramData\\Battle.net\\Agent",
  app_data_roaming_bnet_path: "C:\\Users\\Player\\AppData\\Roaming\\Battle.net",
  accounts_dir: "D:\\D2RHub\\accounts",
  first_run_complete: surface !== "setup",
  feature_profile: "normal",
  feature_profile_prompt_revision: 1,
  installed_optional_modules: ["overlays", "pet", "automation", "room-automation"],
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
  show_main_window_shortcut: "Ctrl+Alt+Home",
  hide_main_window_shortcut: "",
  overlay_opacity: 94,
  main_opacity: 96,
  font_scale: requestedFont,
  app_language: requestedLanguage,
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
  window_layout_enabled: params.get("layouts") !== "empty",
  active_window_layout_id: params.get("layouts") === "empty" ? null : "daily-layout",
  window_layouts: params.get("layouts") === "empty" ? [] : [{ id: "daily-layout", name: "日常多开", monitors: layoutMonitors,
    windows: presetSlots(layoutMonitors, 5, layoutDisplaySize[0] === 3840 ? "4k" : layoutDisplaySize[0] === 1920 ? "1080p" : "2k") }],
};

let persistedGlobalConfig: GlobalConfig = { ...baseConfig,
  ...(params.get("recognition") === "external" ? { rune_audio_external_target: { edition: "Global" as const, mod_name: "jcy-tz" } } : {}) };
let externalSelection: { pid: number; started_at: number } | null = null;
if (params.get("accounts") === "none") accounts.splice(0);

let roomAutomationSnapshot: RoomAutomationConfigSnapshot = {
  schema_version: 1,
  generation: 6,
  config: {
    enabled: true,
    primary_account_id: "sorc-01",
    follower_account_ids: ["barb-02", "pala-03"],
    auto_followers_enabled: false,
    auto_followers_delay_secs: 5,
    follower_join_mode: "simultaneous",
    follower_join_interval_secs: 3,
    shortcut: "Ctrl+Alt+R",
    join_shortcut: "Ctrl+Alt+J",
    name_prefix: "chaos-",
    password: "pw",
    next_sequence: 27,
    sequence_width: 3,
    background_text_strategy: "send_keys",
    strategy_version: 17,
    flow: { step_delay_ms: 100, character_delay_ms: 25 },
  },
  normalization: {
    source_strategy_version: 17,
    target_strategy_version: 17,
    changed: false,
  },
};

const roomAutomationStatus: RoomAutomationWorkflowStatus = {
  revision: 4,
  task_id: null,
  running: false,
  phase: "idle",
  recovery_action: null,
  waiting_mode: null,
  room_name: null,
  room_sequence: null,
  attempt: 0,
  primary_account_id: null,
  follower_account_ids: [],
  completed_follower_account_ids: [],
  started_at: null,
  last_error: null,
};

const modCapsulePool: ModCapsulePool = {
  generation: 1,
  scanned_at: "2026-09-02T00:00:00+08:00",
  capsules: [{
    id: "global:jcy-tz",
    edition: "Global",
    name: "jcy-tz",
    origin: "scanned",
    launch_arguments: "-mod jcy-tz -txt -assettestmode 1",
    default_launch_arguments: "-mod jcy-tz -txt -assettestmode 1",
    feature_groups: ["audio_telemetry", "in_game_room_tools"],
    processed: true,
    source_eligible: true,
    update_required: false,
    ready: true,
    deletable: false,
    assigned_account_ids: audioModState === "legacy" ? ["sorc-01", "barb-02", "pala-03"] : ["barb-02", "pala-03"],
  }],
  accounts: ["sorc-01", "barb-02", "pala-03"].map((accountId) => ({
    account_id: accountId,
    account_name: accounts.find((account) => account.id === accountId)?.display_name || accountId,
    edition: "Global",
    selected_capsule_id: accountId === "sorc-01" && audioModState !== "legacy" ? null : "global:jcy-tz",
    legacy_mod_arguments: accountId === "sorc-01" && audioModState !== "legacy" ? "" : "-mod jcy-tz -txt -assettestmode 1",
    issue: null,
  })),
};

if (params.get("hubSettings") === "1") {
  modCapsulePool.capsules = ["LiteHub", "MyBoAudio", "NullHub"].map((name, index) => ({
    id: `cn:${name}`, edition: "CN", name, origin: "scanned",
    launch_arguments: `-mod ${name} -txt -assettestmode 1`, default_launch_arguments: `-mod ${name} -txt -assettestmode 1`,
    lightweight_profile: (["main", "filler", "min"] as const)[index], source_mod_name: index === 1 ? "BoHub" : null,
    feature_groups: index === 1 ? ["audio_telemetry", "in_game_room_tools"] : [], processed: index === 1,
    source_eligible: true, update_required: false, ready: true, deletable: true, assigned_account_ids: [],
  }));
}
let hubSettingsVersion = "93854";
let hubWaypoints = [...waypointCatalog.defaults];

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
      case "get_launch_scheme_defaults": {
        const keys = ["rune_audio_enabled", "rune_audio_target_account", "rune_audio_external_target", "rune_audio_detection_threshold", "rune_audio_tracked_categories", "rune_audio_min_rune_number", "rune_audio_min_gem_level", "rune_audio_tracked_charm_codes", "enable_tz_overlay", "enable_stats_overlay", "launch_trim_memory", "keep_battle_net_account_id", "auto_close_browser", "agent_mode", "agent_delay_secs", "agent_threshold"];
        return { settings: { ...Object.fromEntries(keys.map(key => [key, (persistedGlobalConfig as unknown as Record<string, unknown>)[key]])), rune_audio_enabled: false, rune_audio_target_account: "", rune_audio_external_target: null, keep_battle_net_account_id: null },
          room: { ...structuredClone(roomAutomationSnapshot.config), enabled: false, primary_account_id: "", follower_account_ids: [] }, layout: null };
      }
      case "save_launch_scheme": {
        const { scheme } = payload as { scheme: GlobalConfig["launch_groups"][number] };
        const saved = { ...scheme, revision: (scheme.revision ?? 0) + 1 };
        persistedGlobalConfig.launch_groups = [...persistedGlobalConfig.launch_groups.filter(group => group.id !== saved.id), saved];
        return saved;
      }
      case "delete_launch_scheme": {
        const { id } = payload as { id: string };
        persistedGlobalConfig.launch_groups = persistedGlobalConfig.launch_groups.filter(group => group.id !== id);
        return null;
      }

      case "pet_get_wardrobe":
      case "pet_settle_activity":
        return structuredClone(petPreview);
      case "pet_wardrobe_action": {
        const { action } = payload as { action: PetAction };
        const wardrobe = petPreview.snapshot.wardrobe;
        if (action.kind === "equip") {
          const item = PET_ITEM_BY_ID.get(action.id);
          if (!item || !wardrobe.owned.includes(item.id)) throw new Error("尚未拥有此饰品");
          wardrobe.equipped[item.slot] = item.id;
        } else if (action.kind === "unequip") delete wardrobe.equipped[action.slot];
        else if (action.kind === "clear") wardrobe.equipped = {};
        else if (action.kind === "tone") wardrobe.tone = action.tone;
        else if (action.kind === "save_preset") wardrobe.presets[action.index] = { ...wardrobe.equipped };
        else if (action.kind === "load_preset") wardrobe.equipped = { ...(wardrobe.presets[action.index] || {}) };
        else throw new Error("浏览器预览不执行饰品兑换，请在桌面程序中操作。");
        petPreview.snapshot.revision += 1;
        petPreview.snapshot.generation += 1;
        return structuredClone(petPreview);
      }
      case "vision_get_config":
        return structuredClone(auditVisionSnapshot);
      case "vision_save_config":
        auditVisionSnapshot = { generation: auditVisionSnapshot.generation + 1, config: (payload as { config: typeof auditVisionSnapshot.config }).config };
        return structuredClone(auditVisionSnapshot);
      case "vision_get_status":
        return { running: false, ready: false, message: "尚未启用视觉辅助", checked: 0, matched: 0, rejected: 0, unavailable: 0 };
      case "vision_get_resource_package":
      case "vision_prepare_resource_package":
        return { ready: true, preparing: false, progress: 100, message: "资源包已就绪", size: 88999851, path: "C:\\Users\\Demo\\AppData\\Roaming\\D2RHub\\resources\\vision" };
      case "vision_open_resource_directory":
        return;
      case "get_global_config":
        return { ...persistedGlobalConfig, first_run_complete: surface !== "setup" };
      case "get_capability_statuses":
        return {
          revision: 3,
          capabilities: [
            { id: "desktop-pet", requested_enabled: true, state: "running", reason_code: null },
            { id: "room-automation", requested_enabled: true, state: "running", reason_code: null },
          ],
        };
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
      case "get_game_layout_account_order":
        return ["barb-02", "sorc-01"].filter(id => accounts.some(account => account.id === id && account.is_running));
      case "get_game_layout_monitors":
        return layoutMonitors;
      case "restore_game_window_layout":
        return { applied: accounts.filter(account => account.is_running).map(account => account.id), failures: [] };
      case "capture_account_window_position": {
        const { accountId, activate } = payload as { accountId: string; activate?: boolean };
        const account = accounts.find(account => account.id === accountId)!;
        const preset = { id: crypto.randomUUID(), name: `当前位置 ${(account.position_presets?.length ?? 0) + 1}`, x: 320, y: 180 };
        account.position_presets = [...(account.position_presets ?? []), preset];
        if (activate !== false) { account.active_position_id = preset.id; account.window_x = preset.x; account.window_y = preset.y; }
        return account;
      }
      case "update_account_positions": {
        const { accountId, activePositionId, positionPresets } = payload as { accountId: string; activePositionId: string | null; positionPresets: AccountMeta["position_presets"] };
        const account = accounts.find(account => account.id === accountId)!;
        account.position_presets = positionPresets; account.active_position_id = activePositionId;
        return account;
      }
      case "refresh_account_running_state":
        return accounts.filter((account) => account.is_running).map((account) => account.id);
      case "inspect_account_launch_health":
        return accounts.map(account => ({ account_id: account.id, error: account.initialized ? null : "账号尚未初始化" }));
      case "close_selected_accounts": {
        const ids = (payload as { accountIds: string[] }).accountIds;
        for (const account of accounts) if (ids.includes(account.id)) account.is_running = false;
        return ids.map(account_id => ({ account_id, error: null }));
      }
      case "get_d2r_window_titles":
        return ["D2R - Ladder Sorc", "D2R - Trav Barb"];
      case "get_foreground_window_title":
        return foregroundWindowTitle;
      case "bring_window_by_title_to_front":
        foregroundWindowTitle = String(
          (payload as Record<string, unknown> | undefined)?.windowTitle || foregroundWindowTitle,
        );
        return true;
      case "patch_account_settings":
        Object.assign(accountSettings, (payload as { patch: SettingsMap }).patch);
        return { ...accountSettings };
      case "get_account_settings":
        if (settingsState === "missing") {
          throw new Error("系统 Settings.json 不存在");
        }
        return accountSettings;
      case "snapshot_system_settings_to_account":
        return accountSettings;
      case "get_scene_stats":
        return { avg_time: 73.4, total_runs: 218 };
      case "get_mod_capsule_pool":
      case "scan_mod_capsule_pool":
      case "add_mod_capsule":
      case "update_mod_capsule":
      case "delete_mod_capsule":
        return modCapsulePool;
      case "get_hub_mod_settings":
      case "save_hub_mod_data_version": {
        const args = payload as { modName: string; gameDataVersion?: string };
        if (args.gameDataVersion) hubSettingsVersion = args.gameDataVersion;
        return { base_mod: args.modName === "MyBoAudio" ? "BoHub" : args.modName,
          game_data_version: hubSettingsVersion, etag: hubSettingsVersion, waypoints_supported: args.modName !== "NullHub" };
      }
      case "get_mod_waypoints":
      case "save_mod_waypoints": {
        const args = payload as { selected?: string[] };
        if (args.selected) hubWaypoints = args.selected;
        return { ...waypointCatalog, selected: hubWaypoints, etag: JSON.stringify(hubWaypoints) };
      }
      case "assign_mod_capsule_to_account":
        return null;
      case "check_software_update":
        return { version: appVersion, available: surface === "software-update" || params.get("update") === "available", downloaded: false };
      case "download_software_update":
        return { version: appVersion, available: true, downloaded: true };
      case "check_mod_resource_updates":
        return ["国服 LiteHub"];
      case "get_bundled_processor_status":
        return { ready: true, blocking_reason: null };
      case "get_mod_resources":
        return {
          catalog: { release_url: "https://github.com/gjy991229/D2rHub/releases", assets: [
            ...["LiteHub", "BoHub", "NullHub"].map((id) => ({ id, version: "mod-resources-20260927.1", size: 24000000, url: "https://github.com/gjy991229/D2rHub/releases", game_data_version: "93854" })),
          ] },
          preferred_source: "gitee", mods: [{id:"LiteHub",installed_version:"mod-resources-20260920.1",update_available:true,protected:false,message:"发现 Mod 更新；关闭游戏后可原位更新"}],
          mods_directory: "C:\\Diablo II Resurrected\\mods", game_data_version: "93854", warning: null,
        };
      case "get_external_audio_instances": {
        const count = params.get("instances") === "many" ? 2 : params.get("instances") === "one" ? 1 : 0;
        return Array.from({ length: count }, (_, index) => ({ pid: 42 + index, started_at: index + 1, mod_name: "jcy-tz", window_title: ["Ladder Sorc", "Trav Barb"][index],
          ready: true, selected: externalSelection?.pid === 42 + index && externalSelection.started_at === index + 1, message: "" }));
      }
      case "select_external_audio_instance":
        externalSelection = (payload as { identity: { pid: number; started_at: number } }).identity;
        return null;
      case "get_rune_audio_status":
        return { running: !!externalSelection && persistedGlobalConfig.rune_audio_enabled, account_id: null, source_id: externalSelection ? recognitionSourceId(persistedGlobalConfig) : null,
          target_pid: externalSelection?.pid ?? null, last_error: null, captured_frames: 0, audio_peak: 0, decoded_packets: 0, rune_events: 0, item_events: 0, scene_ocr_events: 0, last_marker: null, last_confidence: null, last_detected_at: null, diagnostic_recording: false, diagnostic_recording_path: null };
      case "get_audio_mod_setup_state":
        if ((payload as { edition?: string })?.edition) return {
          account_id: "", account_name: "本机游戏 · 国际服", current_mod_name: "jcy-tz", launch_arguments: "-mod jcy-tz -txt", has_txt: true, ready: true, update_required: false, recipe_version: 33, required_recipe_version: 33, build_mode: "augment", source_mod_name: "jcy", feature_groups: ["audio_telemetry"], auto_exit_on_death_enabled: false, reason_code: "ready", message: "准备完成", installed_mods: [{ name: "jcy", audio_ready: false, update_required: false, source_eligible: true, feature_groups: [], audio_reusable: false }], running_pid: null, session_verified: false, active_session_ready: null, active_session_update_required: null, restart_required: false,
        };
        if (audioModState === "legacy") {
          return {
            account_id: String((payload as { accountId?: string })?.accountId ?? "sorc-01"),
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
            feature_groups: ["audio_telemetry"],
            reason_code: "update_available",
            message: "旧版识别 Mod 仍可使用；更新后可获得即时恐怖区域识别",
            installed_mods: [
              { name: "jcy", audio_ready: false, update_required: false, source_eligible: true, feature_groups: [], audio_reusable: false },
              { name: "jcy-tz", audio_ready: true, update_required: true, source_eligible: false, feature_groups: ["audio_telemetry"], audio_reusable: false },
            ],
            running_pid: 28420,
            session_verified: true,
            active_session_ready: true,
            active_session_update_required: true,
            restart_required: false,
          };
        }
        return {
          account_id: String((payload as { accountId?: string })?.accountId ?? "sorc-01"),
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
          feature_groups: [],
          reason_code: "missing_mod",
          message: "当前账号还没有使用识别 Mod",
          installed_mods: [
            { name: "jcy-tz", audio_ready: true, update_required: false, source_eligible: true, feature_groups: ["audio_telemetry", "in_game_room_tools"], audio_reusable: true },
            { name: "ReMoDDeD", audio_ready: false, update_required: false, source_eligible: true, feature_groups: [], audio_reusable: false },
            { name: "VanillaPlus", audio_ready: false, update_required: false, source_eligible: true, feature_groups: [], audio_reusable: false },
          ],
          running_pid: null,
          session_verified: false,
          active_session_ready: null,
          active_session_update_required: null,
          restart_required: false,
        };
      case "room_automation_get_config":
        return roomAutomationSnapshot;
      case "get_tasks":
        if (params.get("softwareTask") === "1") return [{ revision: 1, task_id: 106, kind: "software-update-download", subject: appVersion, state: "running", progress: 42, message: "正在从 Gitee 下载：3 / 7 MB", cancel_requested: false, started_at_ms: Date.now() }];
        if (params.get("resourceTask") === "1") return [{ revision: 1, task_id: 105, kind: "mod-resource-install", subject: "CN:LiteHub", state: "running", progress: 46, message: "正在从 Gitee 下载：11 / 23 MB", cancel_requested: false, started_at_ms: Date.now() }];
        return seedSampleTasks ? [
          {
            revision: 4,
            task_id: 42,
            kind: "audio-mod-prepare",
            subject: "sorc-01",
            conflict_key: "audio-mod-build",
            state: "running",
            progress: 64,
            step: "generating",
            message: requestedLanguage === "en-US" ? "Generating localized data files" : "正在生成本地化数据文件",
            error_code: null,
            cancel_requested: false,
            retryable: false,
            retry_of: null,
            started_at_ms: Date.now() - 38_000,
            finished_at_ms: null,
          },
          {
            revision: 7,
            task_id: 41,
            kind: "account-reinitialize",
            subject: "barb-02",
            conflict_key: "account:barb-02",
            state: "failed",
            progress: 20,
            step: "failed",
            message: requestedLanguage === "en-US" ? "Battle.net login timed out" : "等待 Battle.net 登录超时",
            error_code: "account-initialization-failed",
            cancel_requested: false,
            retryable: true,
            retry_of: null,
            started_at_ms: Date.now() - 320_000,
            finished_at_ms: Date.now() - 200_000,
          },
        ] : [];
      case "export_diagnostic_bundle":
        return "C:\\Users\\Player\\AppData\\Roaming\\D2RHub\\diagnostics\\D2RHub-diagnostics.zip";
      case "get_task_timeline":
        return [
          {
            revision: 1,
            timestamp_ms: Date.now() - 38_000,
            state: "running",
            progress: 0,
            step: "preflight",
            message: requestedLanguage === "en-US" ? "Environment checked" : "运行环境检查完成",
            error_code: null,
            cancel_requested: false,
          },
          {
            revision: 4,
            timestamp_ms: Date.now() - 8_000,
            state: "running",
            progress: 64,
            step: "generating",
            message: requestedLanguage === "en-US" ? "Generating localized data files" : "正在生成本地化数据文件",
            error_code: null,
            cancel_requested: false,
          },
        ];
      case "room_automation_save_config": {
        const args = payload as { config?: RoomAutomationConfigSnapshot["config"] } | undefined;
        roomAutomationSnapshot = {
          ...roomAutomationSnapshot,
          generation: roomAutomationSnapshot.generation + 1,
          config: args?.config ?? roomAutomationSnapshot.config,
        };
        return { snapshot: roomAutomationSnapshot, apply_warning: null };
      }
      case "room_automation_get_status":
      case "room_automation_start_primary":
      case "room_automation_start_followers":
      case "room_automation_retry":
      case "room_automation_cancel":
        return roomAutomationStatus;
      case "get_terror_zone_snapshot":
        return {
          current: {
            start_time: Math.floor(Date.now() / 3_600_000) * 3600,
            end_time: (Math.floor(Date.now() / 3_600_000) + 1) * 3600,
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
            start_time: (Math.floor(Date.now() / 3_600_000) + 1) * 3600,
            end_time: (Math.floor(Date.now() / 3_600_000) + 2) * 3600,
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
        return appVersion;
      case "get_application_disclosure_acceptance":
        return acceptedDisclosureRevision;
      case "accept_application_disclosure":
        acceptedDisclosureRevision = APPLICATION_DISCLOSURE_REVISION;
        return null;
      case "check_cloud_version":
        return { has_update: false, version: appVersion, download_url: "" };
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
    config: { ...persistedGlobalConfig, first_run_complete: surface !== "setup" },
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
      sessionRuns: { "normal:混沌魔殿": 6 },
      currentDrops: [
        { kind: "rune", telemetryId: 15, itemCode: "r15", category: "runes", name: "海尔", nameEn: "Hel", runeNumber: 15, screenshotPath: null },
        { kind: "rune", telemetryId: 15, itemCode: "r15", category: "runes", name: "海尔", nameEn: "Hel", runeNumber: 15, screenshotPath: null },
        { kind: "rune", telemetryId: 30, itemCode: "r30", category: "runes", name: "贝", nameEn: "Ber", runeNumber: 30, screenshotPath: null },
        { kind: "item", telemetryId: 40, itemCode: "pk1", category: "keys", name: "恐惧之钥", nameEn: "Key of Terror", runeNumber: null, screenshotPath: null },
        { kind: "item", telemetryId: 40, itemCode: "pk1", category: "keys", name: "恐惧之钥", nameEn: "Key of Terror", runeNumber: null, screenshotPath: null },
        { kind: "item", telemetryId: 36, itemCode: "cm1", category: "charms", name: "小型护身符", nameEn: "Small Charm", runeNumber: null, screenshotPath: null },
      ],
      currentRunDrops: [
        { kind: "rune", telemetryId: 15, itemCode: "r15", category: "runes", name: "海尔", nameEn: "Hel", runeNumber: 15, screenshotPath: null },
        { kind: "rune", telemetryId: 30, itemCode: "r30", category: "runes", name: "贝", nameEn: "Ber", runeNumber: 30, screenshotPath: null },
        { kind: "item", telemetryId: 40, itemCode: "pk1", category: "keys", name: "恐惧之钥", nameEn: "Key of Terror", runeNumber: null, screenshotPath: null },
      ],
      previousRunDrops: [
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
        if (surface === "software-update") {
          const { default: UpdateConfirmModal } = await import("./components/ui/UpdateConfirmModal");
          if (!cancelled) setContent(<UpdateConfirmModal open onClose={() => {}} version={appVersion} />);
          return;
        }
        if (surface === "mod-resources") {
          const { ModResourceLibrary } = await import("./features/settings/panels/ModResourceLibrary");
          const { AppShell } = await import("./components/layout/AppShell");
          if (!cancelled) setContent(<AppShell><div className="visual-downloads-preview"><ModResourceLibrary edition="CN" en={requestedLanguage === "en-US"} /></div></AppShell>);
          return;
        }

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
              <>
                <AppShell>
                  <div className="flex-1" />
                </AppShell>
                <SettingsCenter
                  open
                  onClose={() => {}}
                  onReconfigure={() => {}}
                  onInitializeAccount={() => {}}
                  initialTab={settingsTab}
                />
                <ToastContainer />
              </>,
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

const auditWindow = window as Window & { __d2rAuditRoot?: ReturnType<typeof ReactDOM.createRoot> };
auditWindow.__d2rAuditRoot ??= ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
auditWindow.__d2rAuditRoot.render(
  <React.StrictMode>
    <AuditRuntime />
  </React.StrictMode>,
);
