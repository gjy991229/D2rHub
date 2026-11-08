export interface RuneAudioStatus {
  source_id?: string | null;
  running: boolean;
  account_id: string | null;
  target_pid: number | null;
  last_error: string | null;
  hostile_error?: string | null;
  captured_frames: number;
  audio_peak: number;
  decoded_packets: number;
  rune_events: number;
  item_events: number;
  scene_ocr_events: number;
  last_marker: string | null;
  last_confidence: number | null;
  last_detected_at: string | null;
  diagnostic_recording: boolean;
  diagnostic_recording_path: string | null;
}

export interface ExternalAudioInstance {
  pid: number;
  started_at: number;
  mod_name: string | null;
  ready: boolean;
  selected?: boolean;
  window_title?: string | null;
  message: string;
}

export const TRACKING_CATEGORIES = [
  { id: "runes", label: "符文", detail: "#1–#33" },
  { id: "gems", label: "宝石与骷髅", detail: "35 种等级/颜色" },
  { id: "charms", label: "护身符", detail: "小型/大型/超大型；不区分词缀" },
  { id: "jewels", label: "珠宝", detail: "基础珠宝；不区分品质或词缀" },
  { id: "keys", label: "钥匙", detail: "恐惧/憎恨/毁灭" },
  { id: "organs", label: "器官", detail: "角/眼/脑" },
  { id: "essences", label: "精华与徽章", detail: "四种精华及赦免徽章" },
] as const;
export const DEFAULT_TRACKING_CATEGORIES = TRACKING_CATEGORIES.map(category => category.id);
export const GEM_LEVELS = ["碎裂", "裂开", "普通", "无瑕疵", "完美"] as const;
export const CHARM_FILTERS = [
  { code: "cm1", label: "小型护身符", detail: "Small Charm" },
  { code: "cm2", label: "大型护身符", detail: "Large Charm" },
  { code: "cm3", label: "超大型护身符", detail: "Grand Charm" },
] as const;
export const AGGREGATE_ITEM_FILTERS = [
  { id: "jewels", label: "珠宝", detail: "全部基础珠宝，不区分品质或词缀" },
  { id: "keys", label: "钥匙", detail: "恐惧、憎恨、毁灭三把钥匙" },
  { id: "organs", label: "器官", detail: "角、眼、脑作为一整项" },
  { id: "essences", label: "精华与徽章", detail: "四种精华及赦免徽章" },
] as const;
