// ── 数据模型 ──

export interface GlobalConfig {
  version: number;
  cn_battle_net_path: string;
  cn_game_path: string;
  cn_saved_games_path: string;
  global_battle_net_path: string;
  global_game_path: string;
  global_saved_games_path: string;
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
  theme: string;
  theme_overlay: string;
  auto_close_browser: boolean;
  enable_auto_update: boolean;
  first_launch: boolean;
  ocr_enabled: boolean;
  ocr_target_account: string;
  ocr_ch_b_profiles_json: string;
  ocr_debug_output?: boolean;

  ocr_poll_interval_ms?: number;
  shortcut_bindings_json: string;
  overlay_opacity: number;
  main_opacity: number;
  font_scale: string;
  app_language?: string;
  agent_mode?: number;
  agent_delay_secs?: number;
  agent_threshold?: number;
}

// ── 数据统计 ──

/// 单条符文掉落记录（与 Rust RuneDropEntry 对应）
export interface RuneDropEntry {
  rune_number: number;       // 1-33
  rune_name: string;         // 中文名
  rune_name_en?: string | null;
  screenshot_path?: string | null;  // 截图相对路径（仅 #24+ 符文）
}

export interface SceneRecord {
  absolute_time: string;
  character_name: string;
  scene_name: string;
  timer_seconds: number;
  drops: RuneDropEntry[];    // 新版：每个元素 = 一次独立掉落
}

export interface StatsData {
  records: SceneRecord[];
}

/// OCR 通道B 的掉落结果（包含符文编号和截图路径）
export interface OcrDropItem {
  text: string;
  source: string;
  timestamp: string;
  rune_number?: number | null;
  screenshot_path?: string | null;
  is_town?: boolean;
  rune_name_en?: string | null;
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
