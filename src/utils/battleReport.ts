import { Image } from "@tauri-apps/api/image";
import { writeImage } from "@tauri-apps/plugin-clipboard-manager";

export type BattleReportDrop = {
  kind?: "rune" | "item";
  item_code?: string | null;
  category?: string;
  display_name?: string;
  rune_name?: string;
  rune_number?: number | null;
};

export type BattleReportRecord = {
  id?: number | string | null;
  absolute_time: string;
  character_name: string;
  scene_name: string;
  tz?: boolean;
  timer_seconds: number;
  journey_id?: string | null;
  segment_index?: number | null;
  drops?: BattleReportDrop[];
  _stats_timer_seconds?: number;
  _merged?: boolean;
  _segment_count?: number;
  _source_record_ids?: Array<number | string>;
};

export type BattleReportStrategy = {
  id: number | string;
  name: string;
  scene_names: string[];
  route?: string;
  preset?: boolean;
};

export type BattleReportStatsData = {
  records: BattleReportRecord[];
  strategies: BattleReportStrategy[];
};

export type BattleReportConfig = {
  range: "today" | "week" | "7" | "30" | "custom";
  customStart: string;
  customEnd: string;
  character: string;
  areaType: "all" | "normal" | "tz";
  dropScope: "all" | "any" | "high";
  customTitle: string;
  customHeadline: string;
  metrics: string[];
  chartMetric: "runs" | "average" | "drops" | "high";
  leftRanking: "scenes" | "characters";
  rightRanking: "drops" | "efficiency";
  dataSource: "raw" | "strategy";
  outliers: "optimized" | "short" | "raw";
  tzGrouping: "separate" | "combined";
};

export type StatsPagePreferences = {
  version?: number;
  activeStrategyIds?: string[];
  presetOverrides?: Record<string, { name: string; scene_names: string[] }>;
  reportConfig?: Partial<BattleReportConfig>;
  filtersCollapsed?: boolean;
  shortThresholdSeconds?: number;
  outlierEnabled?: boolean;
};

export const BATTLE_REPORT_PRESET_STRATEGIES: BattleReportStrategy[] = [
  { id: "preset-countess", name: "女伯爵", route: "黑色荒地 → 高塔 1–5", scene_names: ["黑色荒地", "黑色沼泽", "被遗忘的高塔", "遗忘之塔地牢第1层", "遗忘之塔地牢第2层", "遗忘之塔地牢第3层", "遗忘之塔地牢第4层", "遗忘之塔地牢第5层", "高塔地窖一层", "高塔地窖二层", "高塔地窖三层", "高塔地窖四层", "高塔地窖五层"], preset: true },
  { id: "preset-pit", name: "地穴", route: "外侧回廊 → 泰摩高地 → 深坑 1–2", scene_names: ["外侧回廊", "泰摩高地", "深坑一层", "深坑二层", "地穴一层", "地穴二层"], preset: true },
  { id: "preset-andariel", name: "安达利尔", route: "地下墓穴 2–4", scene_names: ["地下墓穴二层", "地下墓穴三层", "地下墓穴四层"], preset: true },
  { id: "preset-mephisto", name: "墨菲斯托", route: "憎恨囚牢 2–3", scene_names: ["憎恨囚牢二层", "憎恨囚牢三层", "憎恶地牢二层", "憎恶地牢三层"], preset: true },
  { id: "preset-chaos", name: "Chaos", route: "火焰之河 → 混沌避难所", scene_names: ["火焰之河", "混沌避难所", "混沌魔殿"], preset: true },
  { id: "preset-baal", name: "巴尔", route: "世界之石要塞 2–3 → 毁灭王座", scene_names: ["世界之石要塞二层", "世界之石要塞三层", "毁灭王座", "世界之石二层", "世界之石三层"], preset: true },
  { id: "preset-nihlathak", name: "尼拉塞克", route: "神殿 → 三层大厅", scene_names: ["尼拉塞克的神殿", "痛楚大厅", "苦痛大厅", "沃特大厅"], preset: true },
  { id: "preset-ancient-tunnels", name: "古代水道", route: "失落之城 → 古代水道", scene_names: ["失落之城", "古代水道"], preset: true },
  { id: "preset-stony-tomb", name: "碎石古墓", route: "碎石荒野 → 古墓 1–2", scene_names: ["碎石荒野", "碎石古墓一层", "碎石古墓二层"], preset: true },
  { id: "preset-burial-grounds", name: "墓园双陵", route: "埋骨之地 → 墓穴 / 寝陵", scene_names: ["埋骨之地", "墓穴", "寝陵"], preset: true },
];

const METRIC_IDS = ["runs", "duration", "average", "median", "best", "p90", "tz", "drops", "high"];
export const BATTLE_REPORT_CONFIG_DEFAULTS: BattleReportConfig = {
  range: "today",
  customStart: "",
  customEnd: "",
  character: "",
  areaType: "all",
  dropScope: "all",
  customTitle: "",
  customHeadline: "",
  metrics: ["runs", "duration", "average", "tz"],
  chartMetric: "runs",
  leftRanking: "scenes",
  rightRanking: "drops",
  dataSource: "strategy",
  outliers: "optimized",
  tzGrouping: "separate",
};

const categoryLabels: Record<string, string> = { runes: "符文", gems: "宝石", charms: "护身符", jewels: "珠宝", keys: "钥匙", organs: "器官", essences: "精华 / 徽章" };
const weekdays = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
const numberValue = (value: unknown) => Number.isFinite(Number(value)) ? Number(value) : 0;
const getDrops = (record: BattleReportRecord) => Array.isArray(record.drops) ? record.drops : [];
const dropKind = (drop: BattleReportDrop) => drop.kind || "rune";
const dropName = (drop: BattleReportDrop) => drop.display_name || drop.rune_name || "未知掉落";
const dropCode = (drop: BattleReportDrop) => drop.item_code || "";
const isHighRune = (drop: BattleReportDrop) => dropKind(drop) === "rune" && numberValue(drop.rune_number) >= 24;
const dropLabel = (drop: BattleReportDrop) => dropKind(drop) === "rune" ? `#${numberValue(drop.rune_number)} ${dropName(drop)}` : dropName(drop);
const statDuration = (record: BattleReportRecord) => numberValue(record._stats_timer_seconds ?? record.timer_seconds);

function option<T extends string>(value: unknown, allowed: readonly T[], fallback: T): T {
  return allowed.includes(value as T) ? value as T : fallback;
}

export function sanitizeBattleReportConfig(value: Partial<BattleReportConfig> = {}): BattleReportConfig {
  const metrics = [...new Set(Array.isArray(value.metrics) ? value.metrics : BATTLE_REPORT_CONFIG_DEFAULTS.metrics)]
    .filter((metric) => METRIC_IDS.includes(metric)).slice(0, 4);
  return {
    range: option(value.range, ["today", "week", "7", "30", "custom"], "today"),
    customStart: String(value.customStart || "").slice(0, 10),
    customEnd: String(value.customEnd || "").slice(0, 10),
    character: String(value.character || ""),
    areaType: option(value.areaType, ["all", "normal", "tz"], "all"),
    dropScope: option(value.dropScope, ["all", "any", "high"], "all"),
    customTitle: String(value.customTitle || "").trim().slice(0, 12),
    customHeadline: String(value.customHeadline || "").trim().slice(0, 36),
    metrics: metrics.length ? metrics : [...BATTLE_REPORT_CONFIG_DEFAULTS.metrics],
    chartMetric: option(value.chartMetric, ["runs", "average", "drops", "high"], "runs"),
    leftRanking: option(value.leftRanking, ["scenes", "characters"], "scenes"),
    rightRanking: option(value.rightRanking, ["drops", "efficiency"], "drops"),
    dataSource: option(value.dataSource, ["raw", "strategy"], "strategy"),
    outliers: option(value.outliers, ["optimized", "short", "raw"], "optimized"),
    tzGrouping: option(value.tzGrouping, ["separate", "combined"], "separate"),
  };
}

function parseRecordDate(value: string): Date | null {
  const match = String(value || "").match(/^(\d{4})\/(\d{2})\/(\d{2})\/(\d{2}):(\d{2}):(\d{2})$/);
  if (match) return new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]), Number(match[4]), Number(match[5]), Number(match[6]));
  const fallback = new Date(value);
  return Number.isNaN(fallback.getTime()) ? null : fallback;
}

function quantile(values: number[], q: number) {
  const sorted = values.map(numberValue).filter((value) => value > 0).sort((a, b) => a - b);
  if (!sorted.length) return 0;
  const position = (sorted.length - 1) * q;
  const base = Math.floor(position);
  const remainder = position - base;
  return sorted[base + 1] === undefined ? sorted[base] : sorted[base] + remainder * (sorted[base + 1] - sorted[base]);
}

type Summary = ReturnType<typeof summarize>;
function summarize(records: BattleReportRecord[]) {
  const durations = records.map(statDuration).filter((value) => value > 0);
  const drops = records.flatMap(getDrops);
  const highDrops = drops.filter(isHighRune);
  const totalSeconds = durations.reduce((sum, value) => sum + value, 0);
  return {
    runs: records.length,
    totalSeconds,
    average: durations.length ? totalSeconds / durations.length : 0,
    median: quantile(durations, .5),
    p90: quantile(durations, .9),
    best: durations.length ? Math.min(...durations) : 0,
    drops,
    highDrops,
  };
}

export function materializeBattleReportRecords(rawRecords: BattleReportRecord[], strategies: BattleReportStrategy[]) {
  if (!strategies.length) return rawRecords.slice();
  const candidates = strategies.map((strategy) => ({ strategy, selected: new Set(strategy.scene_names || []) }));
  const result: BattleReportRecord[] = [];
  for (let index = 0; index < rawRecords.length;) {
    const first = rawRecords[index];
    const firstIndex = Number(first.segment_index);
    if (!first.journey_id || !Number.isInteger(firstIndex)) {
      result.push(first);
      index++;
      continue;
    }
    let best: { strategy: BattleReportStrategy; group: BattleReportRecord[]; nextIndex: number } | null = null;
    for (const candidate of candidates) {
      if (!candidate.selected.has(first.scene_name)) continue;
      const group = [first];
      let previousIndex = firstIndex;
      let nextIndex = index + 1;
      while (nextIndex < rawRecords.length) {
        const next = rawRecords[nextIndex];
        const nextSegmentIndex = Number(next.segment_index);
        if (next.journey_id !== first.journey_id || !candidate.selected.has(next.scene_name) || !Number.isInteger(nextSegmentIndex) || nextSegmentIndex !== previousIndex + 1) break;
        group.push(next);
        previousIndex = nextSegmentIndex;
        nextIndex++;
      }
      if (!best || group.length > best.group.length) best = { strategy: candidate.strategy, group, nextIndex };
    }
    if (!best) {
      result.push(first);
      index++;
      continue;
    }
    result.push({
      ...first,
      id: `strategy-${best.strategy.id}-${first.journey_id}-${firstIndex}`,
      scene_name: best.strategy.name,
      tz: best.group.some((record) => Boolean(record.tz)),
      timer_seconds: best.group.reduce((sum, record) => sum + numberValue(record.timer_seconds), 0),
      drops: best.group.flatMap(getDrops),
      _merged: true,
      _segment_count: best.group.length,
      _source_record_ids: best.group.map((record) => record.id).filter((id): id is number | string => id != null),
    });
    index = best.nextIndex;
  }
  return result;
}

function optimizeOutliers(records: BattleReportRecord[], thresholdSeconds: number) {
  const candidates = records.filter((record) => numberValue(record.timer_seconds) > thresholdSeconds || getDrops(record).length);
  const groups = new Map<string, BattleReportRecord[]>();
  candidates.forEach((record) => {
    const scene = record.scene_name || "未知场景";
    groups.set(scene, [...(groups.get(scene) || []), record]);
  });
  const averages = new Map<string, number>();
  groups.forEach((items, scene) => {
    const durations = items.map((item) => numberValue(item.timer_seconds)).filter((seconds) => seconds > 0);
    if (durations.length >= 10) averages.set(scene, durations.reduce((sum, seconds) => sum + seconds, 0) / durations.length);
  });
  return candidates.flatMap((record) => {
    const average = averages.get(record.scene_name || "未知场景");
    const seconds = numberValue(record.timer_seconds);
    const outlier = average !== undefined && average > 0 && (seconds > average * 10 || seconds < average * .1);
    if (!outlier) return [record];
    return getDrops(record).length ? [{ ...record, _stats_timer_seconds: average }] : [];
  });
}

function activeStrategies(data: BattleReportStatsData, preferences: StatsPagePreferences) {
  const overrides = preferences.presetOverrides || {};
  const presets = BATTLE_REPORT_PRESET_STRATEGIES.map((strategy) => {
    const override = overrides[String(strategy.id)];
    return override ? { ...strategy, name: override.name, scene_names: override.scene_names } : strategy;
  });
  const strategies = [...presets, ...(data.strategies || []).map((strategy) => ({ ...strategy, id: String(strategy.id) }))];
  const ids = new Set((preferences.activeStrategyIds ?? presets.map((strategy) => String(strategy.id))).map(String));
  return strategies.filter((strategy) => ids.has(String(strategy.id)));
}

function localDayStart(date = new Date()) { return new Date(date.getFullYear(), date.getMonth(), date.getDate()); }
function shiftLocalDate(date: Date, days: number) { const shifted = new Date(date); shifted.setDate(shifted.getDate() + days); return shifted; }
function parseReportDate(value: string) {
  const match = String(value || "").match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!match) return null;
  const date = new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
  return Number.isNaN(date.getTime()) ? null : date;
}

function reportPeriodRange(config: BattleReportConfig, now = new Date()) {
  const today = localDayStart(now);
  let start: Date;
  let next: Date;
  let title: string;
  let kicker: string;
  if (config.range === "custom") {
    const fallbackStart = shiftLocalDate(today, -6);
    const parsedStart = parseReportDate(config.customStart) || fallbackStart;
    const parsedEnd = parseReportDate(config.customEnd) || today;
    const requestedStart = parsedStart > today ? today : parsedStart;
    const requestedEnd = parsedEnd > today ? today : parsedEnd;
    start = localDayStart(requestedStart <= requestedEnd ? requestedStart : requestedEnd);
    next = shiftLocalDate(localDayStart(requestedStart <= requestedEnd ? requestedEnd : requestedStart), 1);
    title = "自定义战报";
    kicker = "CUSTOM REPORT";
  } else if (config.range === "week") {
    start = shiftLocalDate(today, -((today.getDay() + 6) % 7)); next = shiftLocalDate(start, 7); title = "一周战报"; kicker = "WEEKLY REPORT";
  } else if (config.range === "7") {
    start = shiftLocalDate(today, -6); next = shiftLocalDate(start, 7); title = "七日战报"; kicker = "7-DAY REPORT";
  } else if (config.range === "30") {
    start = shiftLocalDate(today, -29); next = shiftLocalDate(start, 30); title = "三十日战报"; kicker = "30-DAY REPORT";
  } else {
    start = today; next = shiftLocalDate(start, 1); title = "每日战报"; kicker = "DAILY REPORT";
  }
  const end = next > now && start <= now ? now : next;
  const periodDuration = Math.max(1, next.getTime() - start.getTime());
  const observedDuration = Math.max(1, end.getTime() - start.getTime());
  const previousStart = new Date(start.getTime() - periodDuration);
  const previousEnd = new Date(previousStart.getTime() + observedDuration);
  const pad = (value: number) => String(value).padStart(2, "0");
  const full = (date: Date) => `${date.getFullYear()}.${pad(date.getMonth() + 1)}.${pad(date.getDate())}`;
  const last = shiftLocalDate(next, -1);
  const suffix = config.range === "today" ? " · 今日" : config.range === "week" ? " · 本周" : config.range === "7" ? " · 近 7 天" : config.range === "30" ? " · 近 30 天" : "";
  const label = config.range === "today" ? `${full(start)}${suffix}` : `${full(start)} — ${full(last)}${suffix}`;
  return { start, end, next, label, previousStart, previousEnd, title, kicker };
}

function groupedStats(records: BattleReportRecord[], key: "scene_name" | "character_name") {
  const groups = new Map<string, BattleReportRecord[]>();
  records.forEach((record) => {
    const name = record[key] || (key === "scene_name" ? "未知场景" : "未知角色");
    groups.set(name, [...(groups.get(name) || []), record]);
  });
  return [...groups.entries()].map(([name, items]) => ({ name, items, summary: summarize(items) }));
}

function groupedScenes(records: BattleReportRecord[], config: BattleReportConfig) {
  if (config.tzGrouping === "combined") return groupedStats(records, "scene_name").map((group) => ({ ...group, tz: false }));
  const groups = new Map<string, BattleReportRecord[]>();
  records.forEach((record) => {
    const name = record.scene_name || "未知场景";
    const key = `${name}\u0000${record.tz ? "1" : "0"}`;
    groups.set(key, [...(groups.get(key) || []), record]);
  });
  return [...groups.entries()].map(([key, items]) => ({ name: key.split("\u0000")[0], tz: key.endsWith("1"), items, summary: summarize(items) }));
}

type DropAggregate = { label: string; category: string; count: number; rune: number; score: number };
function aggregateDrops(records: BattleReportRecord[]) {
  const map = new Map<string, DropAggregate>();
  const priority: Record<string, number> = { organs: 900, keys: 850, essences: 800, charms: 760, jewels: 720, gems: 600 };
  records.flatMap(getDrops).forEach((drop) => {
    const rune = numberValue(drop.rune_number);
    const category = drop.category || "";
    const key = dropKind(drop) === "rune" ? `rune:${rune}` : `${category}:${dropCode(drop) || dropName(drop)}`;
    const score = dropKind(drop) === "rune" ? (rune >= 24 ? 1000 + rune * 10 : rune >= 16 ? 700 + rune : 500 + rune) : (priority[category] || 400);
    const current = map.get(key) || { label: dropLabel(drop), category: categoryLabels[category] || "掉落", count: 0, rune, score };
    current.count++;
    map.set(key, current);
  });
  return [...map.values()].sort((a, b) => b.score - a.score || b.count - a.count || a.label.localeCompare(b.label, "zh-CN"));
}

function formatDuration(seconds: number) {
  const total = Math.max(0, numberValue(seconds));
  if (total < 60) return `${total.toFixed(total < 10 ? 1 : 0)} 秒`;
  const minutes = Math.floor(total / 60);
  const remainder = Math.round(total % 60);
  if (minutes < 60) return `${minutes}分 ${remainder}秒`;
  return `${Math.floor(minutes / 60)}时 ${minutes % 60}分`;
}

type SeriesItem = { label: string; value: number; display: string };
function seriesValue(records: BattleReportRecord[], metric: BattleReportConfig["chartMetric"]) {
  const summary = summarize(records);
  if (metric === "average") return summary.average;
  if (metric === "drops") return summary.drops.length;
  if (metric === "high") return summary.highDrops.length;
  return summary.runs;
}
function seriesDisplay(value: number, metric: BattleReportConfig["chartMetric"]) { return metric === "average" ? (value ? formatDuration(value) : "0") : String(Math.round(value)); }
function barSeries(records: BattleReportRecord[], range: ReturnType<typeof reportPeriodRange>, config: BattleReportConfig): SeriesItem[] {
  if (config.range === "today") {
    const buckets = Array.from({ length: 6 }, () => [] as BattleReportRecord[]);
    records.forEach((record) => { const date = parseRecordDate(record.absolute_time); if (date) buckets[Math.min(5, Math.floor(date.getHours() / 4))].push(record); });
    return buckets.map((items, index) => { const value = seriesValue(items, config.chartMetric); return { label: `${String(index * 4).padStart(2, "0")}时`, value, display: seriesDisplay(value, config.chartMetric) }; });
  }
  const totalDays = Math.max(1, Math.round((range.next.getTime() - range.start.getTime()) / 864e5));
  const daysPerBucket = Math.max(1, Math.ceil(totalDays / 7));
  const bucketCount = Math.ceil(totalDays / daysPerBucket);
  const buckets = Array.from({ length: bucketCount }, () => [] as BattleReportRecord[]);
  const pad = (value: number) => String(value).padStart(2, "0");
  const short = (date: Date) => `${pad(date.getMonth() + 1)}.${pad(date.getDate())}`;
  records.forEach((record) => {
    const date = parseRecordDate(record.absolute_time);
    if (!date) return;
    const day = Math.floor((localDayStart(date).getTime() - range.start.getTime()) / 864e5);
    const index = Math.floor(day / daysPerBucket);
    if (index >= 0 && index < bucketCount) buckets[index].push(record);
  });
  return buckets.map((items, index) => {
    const bucketStart = shiftLocalDate(range.start, index * daysPerBucket);
    const bucketEnd = shiftLocalDate(bucketStart, Math.min(daysPerBucket, totalDays - index * daysPerBucket) - 1);
    const value = seriesValue(items, config.chartMetric);
    const label = daysPerBucket === 1 ? (config.range === "week" ? weekdays[(bucketStart.getDay() + 6) % 7].slice(1) : short(bucketStart)) : `${short(bucketStart)}-${short(bucketEnd)}`;
    return { label, value, display: seriesDisplay(value, config.chartMetric) };
  });
}

type ReportGroup = { name: string; tz: boolean; summary: Summary };
type ReportMetric = { label: string; value: string; note: string; tone?: "violet" | "clay" };
export type BattleReportSnapshot = {
  range: ReturnType<typeof reportPeriodRange>;
  records: BattleReportRecord[];
  summary: Summary;
  previous: Summary;
  scenes: ReportGroup[];
  drops: DropAggregate[];
  tzRuns: number;
  series: SeriesItem[];
  leftGroups: ReportGroup[];
  title: string;
  kicker: string;
  headline: string;
  chartTitle: string;
  chartSubtitle: string;
  leftTitle: string;
  leftSubtitle: string;
  rightMode: "drops" | "efficiency";
  rightTitle: string;
  rightSubtitle: string;
  summaryLabel: string;
  metrics: ReportMetric[];
};

function percentDelta(current: number, previous: number) {
  if (!previous) return "同期暂无基线";
  const value = (current - previous) / previous * 100;
  return `较同期 ${value > 0 ? "+" : ""}${value.toFixed(0)}%`;
}

export function buildBattleReportSnapshot(data: BattleReportStatsData, preferences: StatsPagePreferences = {}, now = new Date()): BattleReportSnapshot {
  const config = sanitizeBattleReportConfig(preferences.reportConfig || {});
  const strategies = activeStrategies(data, preferences);
  const source = config.dataSource === "strategy" && strategies.length ? materializeBattleReportRecords(data.records || [], strategies) : (data.records || []).slice();
  const threshold = Math.max(0, Math.min(10, numberValue(preferences.shortThresholdSeconds ?? 1)));
  const range = reportPeriodRange(config, now);
  const recordsBetween = (start: Date, end: Date) => {
    const scoped = source.filter((record) => {
      const date = parseRecordDate(record.absolute_time);
      if (!date || date < start || date >= end) return false;
      if (config.character && record.character_name !== config.character) return false;
      if (config.areaType === "normal" && record.tz || config.areaType === "tz" && !record.tz) return false;
      const drops = getDrops(record);
      if (config.dropScope === "any" && !drops.length || config.dropScope === "high" && !drops.some(isHighRune)) return false;
      return true;
    });
    if (config.outliers === "optimized") return optimizeOutliers(scoped, threshold);
    if (config.outliers === "short") return scoped.filter((record) => numberValue(record.timer_seconds) > threshold || getDrops(record).length);
    return scoped;
  };
  const records = recordsBetween(range.start, range.end);
  const previousRecords = recordsBetween(range.previousStart, range.previousEnd);
  const summary = summarize(records);
  const previous = summarize(previousRecords);
  const scenes = groupedScenes(records, config).sort((a, b) => b.summary.runs - a.summary.runs || a.summary.average - b.summary.average);
  const drops = aggregateDrops(records);
  const tzRuns = records.filter((record) => Boolean(record.tz)).length;
  const leftGroups = (config.leftRanking === "characters" ? groupedStats(records, "character_name").map((group) => ({ ...group, tz: false })) : scenes)
    .sort((a, b) => b.summary.runs - a.summary.runs || a.summary.average - b.summary.average);
  const topScene = scenes[0] || null;
  const topDrop = drops[0] || null;
  const headline = topDrop && topDrop.rune >= 24 ? `${topDrop.label}，写进本期战绩` : topDrop ? `${topDrop.label} ×${topDrop.count}，领衔本期战利品` : topScene ? `${topScene.tz ? "TZ " : ""}${topScene.name}，${summary.runs} 场实录` : "等待下一段地狱征途";
  const chartLabels: Record<BattleReportConfig["chartMetric"], [string, string]> = { runs: ["场次节奏", "各时段完成场次"], average: ["耗时节奏", "各时段平均耗时"], drops: ["掉落节奏", "各时段记录掉落"], high: ["高级符文", "各时段 #24+ 符文"] };
  const rightUsesDrops = config.rightRanking === "drops" && drops.length > 0;
  const snapshot: BattleReportSnapshot = {
    range, records, summary, previous, scenes, drops, tzRuns,
    series: barSeries(records, range, config),
    leftGroups,
    title: config.customTitle || range.title,
    kicker: range.kicker,
    headline: config.customHeadline || headline,
    chartTitle: chartLabels[config.chartMetric][0],
    chartSubtitle: chartLabels[config.chartMetric][1],
    leftTitle: config.leftRanking === "characters" ? "主要角色" : "主要区域",
    leftSubtitle: config.leftRanking === "characters" ? "按完成场次排序" : config.tzGrouping === "separate" ? "普通 / TZ 分开统计" : "普通 / TZ 合并统计",
    rightMode: rightUsesDrops ? "drops" : "efficiency",
    rightTitle: rightUsesDrops ? "重点掉落" : "效率概览",
    rightSubtitle: rightUsesDrops ? "按稀有度与数量排序" : "本期战斗节奏",
    summaryLabel: config.range === "today" ? "今日小结" : config.range === "week" ? "本周小结" : "本期小结",
    metrics: [],
  };
  const areaCount = records.length ? new Set(records.map((record) => record.scene_name)).size : 0;
  const definitions: Record<string, ReportMetric> = {
    runs: { label: "完成场次", value: summary.runs.toLocaleString(), note: `记录 ${areaCount} 个区域` },
    duration: { label: "战斗时长", value: formatDuration(summary.totalSeconds), note: "本期累计" },
    average: { label: "场均耗时", value: summary.average ? formatDuration(summary.average) : "—", note: percentDelta(summary.average, previous.average) },
    median: { label: "中位耗时", value: summary.median ? formatDuration(summary.median) : "—", note: "一半场次快于此值" },
    best: { label: "最快完成", value: summary.best ? formatDuration(summary.best) : "—", note: "本期最佳" },
    p90: { label: "P90 耗时", value: summary.p90 ? formatDuration(summary.p90) : "—", note: "九成场次快于此值" },
    tz: { label: "TZ 场次", value: tzRuns.toLocaleString(), note: `占比 ${summary.runs ? Math.round(tzRuns / summary.runs * 100) : 0}%`, tone: "violet" },
    drops: { label: "记录掉落", value: summary.drops.length.toLocaleString(), note: "已启用声纹项目" },
    high: { label: "高级符文", value: summary.highDrops.length.toLocaleString(), note: "#24 及以上", tone: "clay" },
  };
  snapshot.metrics = config.metrics.map((metric) => definitions[metric]).filter(Boolean);
  return snapshot;
}

function canvasFont(size: number, weight = 700) { return `${weight} ${size}px Inter, "Segoe UI", "Microsoft YaHei", "PingFang SC", sans-serif`; }
function displayFont(size: number, weight = 700) { return `${weight} ${size}px Georgia, "STZhongsong", "SimSun", "Microsoft YaHei", serif`; }
function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number, radius: number, fill: string, stroke = "") {
  const resolvedRadius = Math.min(radius, width / 2, height / 2);
  ctx.beginPath(); ctx.moveTo(x + resolvedRadius, y); ctx.arcTo(x + width, y, x + width, y + height, resolvedRadius); ctx.arcTo(x + width, y + height, x, y + height, resolvedRadius); ctx.arcTo(x, y + height, x, y, resolvedRadius); ctx.arcTo(x, y, x + width, y, resolvedRadius); ctx.closePath();
  if (fill) { ctx.fillStyle = fill; ctx.fill(); }
  if (stroke) { ctx.strokeStyle = stroke; ctx.lineWidth = 1; ctx.stroke(); }
}
function canvasText(ctx: CanvasRenderingContext2D, text: unknown, x: number, y: number, maxWidth: number, size: number, weight = 700, color = "#2f2a26", align: CanvasTextAlign = "left", display = false) {
  ctx.save(); ctx.fillStyle = color; ctx.textAlign = align; ctx.textBaseline = "alphabetic";
  let actual = size; ctx.font = display ? displayFont(actual, weight) : canvasFont(actual, weight);
  while (actual > (display ? 22 : 18) && ctx.measureText(String(text)).width > maxWidth) { actual -= 2; ctx.font = display ? displayFont(actual, weight) : canvasFont(actual, weight); }
  ctx.fillText(String(text), x, y, maxWidth); ctx.restore();
}
function rule(ctx: CanvasRenderingContext2D, y: number, x = 76, width = 928, color = "#d8d0c7") { ctx.fillStyle = color; ctx.fillRect(x, y, width, 1); }
function sectionTitle(ctx: CanvasRenderingContext2D, title: string, subtitle: string, x: number, y: number, width: number, ink = "#2f2a26", muted = "#766e66") { canvasText(ctx, title, x, y, width, 25, 700, ink, "left", true); if (subtitle) canvasText(ctx, subtitle, x + width, y, width, 13, 600, muted, "right"); }
function rank(ctx: CanvasRenderingContext2D, value: number, x: number, y: number, accent = "#cc785c") { ctx.save(); ctx.strokeStyle = accent; ctx.lineWidth = 1.5; ctx.beginPath(); ctx.arc(x, y, 13, 0, Math.PI * 2); ctx.stroke(); canvasText(ctx, value, x, y + 5, 22, 13, 750, accent, "center"); ctx.restore(); }

export function drawBattleReport(canvas: HTMLCanvasElement, snapshot: BattleReportSnapshot) {
  canvas.width = 1080; canvas.height = 1350;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("无法创建战报画布");
  const width = canvas.width;
  const paper = "#f4f0e8", paperDeep = "#e9e1d7", ink = "#2f2a26", muted = "#766e66", reportRule = "#d8d0c7", clay = "#cc785c", clayDeep = "#ad5f49", clayPale = "#ead8ce", violet = "#7656a4";
  ctx.clearRect(0, 0, width, canvas.height); ctx.fillStyle = paper; ctx.fillRect(0, 0, width, canvas.height);
  const warmWash = ctx.createRadialGradient(940, 0, 0, 940, 0, 520); warmWash.addColorStop(0, "rgba(204,120,92,.13)"); warmWash.addColorStop(1, "rgba(204,120,92,0)"); ctx.fillStyle = warmWash; ctx.fillRect(420, 0, 660, 560);
  let seed = 0x2d2a29; const random = () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 4294967296; };
  ctx.save(); for (let index = 0; index < 900; index++) { const shade = random() > .5 ? 74 : 255; ctx.fillStyle = `rgba(${shade},${shade},${shade},${.009 + random() * .012})`; ctx.fillRect(Math.floor(random() * width), Math.floor(random() * canvas.height), 1, 1); } ctx.restore();
  ctx.fillStyle = clay; ctx.beginPath(); ctx.arc(84, 70, 7, 0, Math.PI * 2); ctx.fill(); canvasText(ctx, "D2RHub", 104, 78, 220, 22, 780, ink); canvasText(ctx, "本地声纹战绩", 226, 77, 200, 14, 600, muted); canvasText(ctx, snapshot.range.label, 1004, 77, 360, 15, 650, muted, "right"); rule(ctx, 105, 76, 928, reportRule);
  canvasText(ctx, snapshot.kicker, 76, 151, 300, 13, 760, clay); canvasText(ctx, snapshot.title, 76, 218, 600, 62, 700, ink, "left", true); canvasText(ctx, snapshot.headline, 76, 265, 860, 25, 650, snapshot.drops[0]?.rune >= 24 ? clayDeep : ink); rule(ctx, 302, 76, 928, reportRule);
  const metricWidth = 928 / Math.max(1, snapshot.metrics.length); snapshot.metrics.forEach((metric, index) => { const x = 76 + index * metricWidth; const color = metric.tone === "violet" ? violet : metric.tone === "clay" ? clayDeep : ink; if (index) { ctx.fillStyle = reportRule; ctx.fillRect(x - 20, 330, 1, 110); } canvasText(ctx, metric.label, x, 346, metricWidth - 28, 14, 650, metric.tone === "violet" ? violet : muted); canvasText(ctx, metric.value, x, 399, metricWidth - 28, 39, 700, color, "left", true); canvasText(ctx, metric.note, x, 434, metricWidth - 28, 13, 580, muted); });
  rule(ctx, 476, 76, 928, reportRule); sectionTitle(ctx, snapshot.chartTitle, snapshot.chartSubtitle, 76, 530, 928, ink, muted);
  const chartX = 76, chartY = 573, chartWidth = 928, max = Math.max(1, ...snapshot.series.map((item) => item.value)), gap = 16, barWidth = (chartWidth - gap * (snapshot.series.length - 1)) / Math.max(1, snapshot.series.length); ctx.fillStyle = reportRule; ctx.fillRect(chartX, chartY + 99, chartWidth, 1);
  snapshot.series.forEach((item, index) => { const x = chartX + index * (barWidth + gap), height = item.value / max * 82, center = x + barWidth / 2; roundRect(ctx, x, chartY + 99 - height, barWidth, height, Math.min(8, barWidth / 2), index === snapshot.series.length - 1 ? clayDeep : clay); canvasText(ctx, item.display, center, chartY + 88 - height, barWidth, 13, 700, item.value ? ink : muted, "center"); canvasText(ctx, item.label, center, chartY + 125, barWidth, 12, 600, muted, "center"); });
  rule(ctx, 745, 76, 928, reportRule); sectionTitle(ctx, snapshot.leftTitle, snapshot.leftSubtitle, 76, 801, 548, ink, muted); sectionTitle(ctx, snapshot.rightTitle, snapshot.rightSubtitle, 686, 801, 318, ink, muted); ctx.fillStyle = reportRule; ctx.fillRect(655, 783, 1, 300);
  const zoneMax = Math.max(1, ...snapshot.leftGroups.slice(0, 4).map((group) => group.summary.runs)); snapshot.leftGroups.slice(0, 4).forEach((group, index) => { const y = 858 + index * 66, color = group.tz ? violet : ink; rank(ctx, index + 1, 91, y - 7, group.tz ? violet : clay); canvasText(ctx, `${group.tz ? "TZ · " : ""}${group.name}`, 120, y, 318, 18, 700, color); canvasText(ctx, `${group.summary.runs} 场`, 624, y, 86, 15, 720, color, "right"); canvasText(ctx, group.summary.average ? `场均 ${formatDuration(group.summary.average)}` : "暂无耗时", 120, y + 24, 220, 12, 580, muted); ctx.fillStyle = paperDeep; ctx.fillRect(350, y + 18, 274, 4); ctx.fillStyle = group.tz ? violet : clay; ctx.fillRect(350, y + 18, Math.max(6, 274 * group.summary.runs / zoneMax), 4); });
  if (snapshot.rightMode === "drops") snapshot.drops.slice(0, 4).forEach((drop, index) => { const y = 858 + index * 66, color = drop.rune >= 24 ? clayDeep : ink; rank(ctx, index + 1, 701, y - 7, drop.rune >= 24 ? clayDeep : clay); canvasText(ctx, drop.label, 730, y, 208, 18, 700, color); canvasText(ctx, `×${drop.count}`, 1004, y, 58, 17, 760, color, "right"); canvasText(ctx, drop.category, 730, y + 24, 190, 12, 580, muted); });
  else [{ label: "最快完成", value: snapshot.summary.best ? formatDuration(snapshot.summary.best) : "—" }, { label: "中位耗时", value: snapshot.summary.median ? formatDuration(snapshot.summary.median) : "—" }, { label: "P90 耗时", value: snapshot.summary.p90 ? formatDuration(snapshot.summary.p90) : "—" }, { label: "高级符文", value: `${snapshot.summary.highDrops.length} 次` }].forEach((item, index) => { const y = 858 + index * 66; canvasText(ctx, item.label, 686, y, 140, 14, 600, muted); canvasText(ctx, item.value, 1004, y + 2, 220, 24, 700, ink, "right", true); if (index < 3) rule(ctx, y + 27, 686, 318, reportRule); });
  const topScene = snapshot.scenes[0]; const insight = topScene ? `${topScene.tz ? "TZ " : ""}${topScene.name}贡献 ${Math.round(topScene.summary.runs / Math.max(1, snapshot.summary.runs) * 100)}% 场次，场均 ${formatDuration(topScene.summary.average)}。${snapshot.tzRuns ? `本期 TZ 共完成 ${snapshot.tzRuns} 场。` : ""}` : "开始记录后，这里会出现最值得分享的一句战绩。"; roundRect(ctx, 76, 1128, 928, 126, 16, clayPale); canvasText(ctx, snapshot.summaryLabel, 104, 1168, 180, 14, 760, clayDeep); canvasText(ctx, insight, 104, 1216, 848, 23, 600, ink, "left", true);
  canvasText(ctx, "每一场，都算数。", 76, 1305, 300, 17, 700, ink); const generated = new Date(); const stamp = `D2RHub · 本地声纹统计 · ${generated.getFullYear()}.${String(generated.getMonth() + 1).padStart(2, "0")}.${String(generated.getDate()).padStart(2, "0")}`; canvasText(ctx, stamp, 1004, 1305, 520, 13, 580, muted, "right");
}

export async function copyBattleReportToClipboard(data: BattleReportStatsData, preferences: StatsPagePreferences = {}) {
  const snapshot = buildBattleReportSnapshot(data, preferences);
  if (!snapshot.summary.runs) throw new Error(`${snapshot.range.label}没有可分享的场次`);
  const canvas = document.createElement("canvas");
  drawBattleReport(canvas, snapshot);
  const context = canvas.getContext("2d");
  if (!context) throw new Error("无法读取战报画布");
  const rgba = context.getImageData(0, 0, canvas.width, canvas.height).data;
  const image = await Image.new(new Uint8Array(rgba.buffer), canvas.width, canvas.height);
  try {
    await writeImage(image);
  } finally {
    await image.close();
  }
  return { runs: snapshot.summary.runs, rangeLabel: snapshot.range.label };
}
