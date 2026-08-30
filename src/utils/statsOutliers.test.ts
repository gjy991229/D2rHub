declare const require: (name: string) => unknown;
declare const __dirname: string;

type Drop = { display_name?: string };
type RecordFixture = {
  id: number | string;
  scene_name: string;
  timer_seconds: number;
  drops: Drop[];
  tz?: boolean;
  journey_id?: string;
  segment_index?: number;
  _merged?: boolean;
  _segment_count?: number;
  _source_record_ids?: Array<number | string>;
  _outlier_status?: string;
  _stats_timer_seconds?: number;
};
type StrategyFixture = {
  id: string;
  name: string;
  scene_names: string[];
};
type OptimizerResult = {
  records: RecordFixture[];
  summary: { eligibleScenes: number; ignored: number; adjusted: number; shortIgnored: number };
};

const fs = require("fs") as { readFileSync(path: string, encoding: string): string };
const path = require("path") as { join(...parts: string[]): string };
const template = fs.readFileSync(path.join(__dirname, "../../docs/stats.html"), "utf8");
const optimizerSource = template
  .split("// OUTLIER_OPTIMIZER_START")[1]
  .split("// OUTLIER_OPTIMIZER_END")[0];
const optimizeOutlierRecords = new Function(`
  const number = value => Number.isFinite(Number(value)) ? Number(value) : 0;
  const getDrops = record => Array.isArray(record.drops) ? record.drops : [];
  ${optimizerSource}
  return optimizeOutlierRecords;
`)() as (records: RecordFixture[], enabled: boolean, shortThresholdSeconds?: number) => OptimizerResult;
const sceneGroupingSource = template
  .split("// SCENE_GROUPING_START")[1]
  .split("// SCENE_GROUPING_END")[0];
const groupedSceneStats = new Function(`
  const summarize = records => ({ runs: records.length });
  ${sceneGroupingSource}
  return groupedSceneStats;
`)() as (records: RecordFixture[]) => Array<{
  name: string;
  tz: boolean;
  items: RecordFixture[];
  summary: { runs: number };
}>;
const areaTypeFilterSource = template
  .split("// AREA_TYPE_FILTER_START")[1]
  .split("// AREA_TYPE_FILTER_END")[0];
const matchesAreaType = new Function(`
  ${areaTypeFilterSource}
  return matchesAreaType;
`)() as (record: RecordFixture, areaType: "all" | "normal" | "tz") => boolean;
const shareReportSource = template
  .split("// SHARE_REPORT_START")[1]
  .split("// SHARE_REPORT_END")[0];
const strategyMaterializerSource = template
  .split("// STRATEGY_MATERIALIZER_START")[1]
  .split("// STRATEGY_MATERIALIZER_END")[0];
const materializeRecords = new Function(`
  const number = value => Number.isFinite(Number(value)) ? Number(value) : 0;
  const getDrops = record => Array.isArray(record.drops) ? record.drops : [];
  ${strategyMaterializerSource}
  return materializeRecords;
`)() as (records: RecordFixture[], strategies: StrategyFixture[]) => RecordFixture[];
const summarySource = template
  .split("// STATS_SUMMARY_START")[1]
  .split("// STATS_SUMMARY_END")[0];
const summarize = new Function(`
  const number = value => Number.isFinite(Number(value)) ? Number(value) : 0;
  const statDuration = record => number(record._stats_timer_seconds ?? record.timer_seconds);
  const getDrops = record => Array.isArray(record.drops) ? record.drops : [];
  const isHighRune = () => false;
  const quantile = (values, q) => {
    const sorted = values.map(number).filter(value => value > 0).sort((a, b) => a - b);
    if (!sorted.length) return 0;
    const position = (sorted.length - 1) * q;
    const base = Math.floor(position);
    const remainder = position - base;
    return sorted[base + 1] === undefined
      ? sorted[base]
      : sorted[base] + remainder * (sorted[base + 1] - sorted[base]);
  };
  ${summarySource}
  return summarize;
`)() as (records: RecordFixture[]) => { runs: number; totalSeconds: number; average: number };

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(message);
}

function makeRecords(scene: string, count: number, seconds = 100): RecordFixture[] {
  return Array.from({ length: count }, (_, index) => ({
    id: index + 1,
    scene_name: scene,
    timer_seconds: seconds,
    drops: [],
  }));
}

{
  const records = [...makeRecords("不足十场", 8), { id: 9, scene_name: "不足十场", timer_seconds: 3000, drops: [] }];
  const result = optimizeOutlierRecords(records, true);
  assert(result.records.length === 9, "少于 10 场时不应过滤离群记录");
  assert(result.summary.eligibleScenes === 0, "少于 10 场的场景不应进入优化范围");
}

{
  const records = [...makeRecords("无产出离群", 20), { id: 21, scene_name: "无产出离群", timer_seconds: 3000, drops: [] }];
  const result = optimizeOutlierRecords(records, true);
  assert(result.records.length === 20, "无产出的超长离群场次应被忽略");
  assert(result.summary.ignored === 1 && result.summary.adjusted === 0, "应正确汇总忽略场次");
}

{
  const outlier: RecordFixture = { id: 21, scene_name: "有产出离群", timer_seconds: 3000, drops: [{ display_name: "掉落" }] };
  const records = [...makeRecords("有产出离群", 20), outlier];
  const result = optimizeOutlierRecords(records, true);
  const adjusted = result.records.find(record => record.id === 21);
  const expectedAverage = 5000 / 21;
  assert(result.records.length === 21, "有产出的离群场次应继续参与统计");
  assert(result.summary.adjusted === 1 && result.summary.ignored === 0, "应正确汇总校正场次");
  assert(adjusted?.timer_seconds === 3000, "校正不应覆盖原始耗时");
  assert(Math.abs((adjusted?._stats_timer_seconds ?? 0) - expectedAverage) < 0.000001, "统计耗时应使用同场景平均值");
  assert(outlier._stats_timer_seconds === undefined, "优化不应修改传入的原记录对象");
}

{
  const records = [...makeRecords("极短离群", 20), { id: 21, scene_name: "极短离群", timer_seconds: 1, drops: [] }];
  const result = optimizeOutlierRecords(records, true);
  assert(result.records.length === 20 && result.summary.shortIgnored === 1, "1 秒内的无产出场次应被过滤");
}

{
  const records: RecordFixture[] = [
    { id: 1, scene_name: "未满十场", timer_seconds: 1, drops: [] },
    { id: 2, scene_name: "未满十场", timer_seconds: 1.1, drops: [] },
    { id: 3, scene_name: "未满十场", timer_seconds: 1, drops: [{ display_name: "掉落" }] },
  ];
  const result = optimizeOutlierRecords(records, true);
  assert(result.records.map(record => record.id).join(",") === "2,3", "短场过滤不应依赖同场景满 10 场，并应保留有产出的记录");
  assert(result.summary.eligibleScenes === 0 && result.summary.ignored === 1, "短场过滤应独立计入忽略数量");
  const disabled = optimizeOutlierRecords(records, false);
  assert(disabled.records.length === 3 && disabled.summary.shortIgnored === 0, "关闭优化时不应过滤短场记录");
}

{
  const records: RecordFixture[] = [
    { id: 1, scene_name: "阈值场景", timer_seconds: 1.5, drops: [] },
    { id: 2, scene_name: "阈值场景", timer_seconds: 3, drops: [] },
  ];
  assert(optimizeOutlierRecords(records, true, 2).records.length === 1, "短空场阈值应过滤不超过滑条值的无掉落场次");
  assert(optimizeOutlierRecords(records, true, 1).records.length === 2, "调整阈值后不应继续写死为 1 秒");
}

{
  const records: RecordFixture[] = [
    { id: 1, scene_name: "营房", timer_seconds: 30, drops: [] },
    { id: 2, scene_name: "营房", timer_seconds: 40, drops: [], tz: true },
    { id: 3, scene_name: "营房", timer_seconds: 50, drops: [] },
  ];
  const groups = groupedSceneStats(records);
  const normal = groups.find(group => !group.tz);
  const tz = groups.find(group => group.tz);
  assert(groups.length === 2, "同名普通区域与 TZ 区域应拆分为两组");
  assert(normal?.summary.runs === 2 && tz?.summary.runs === 1, "普通与 TZ 场次应分别汇总");
}

{
  const normal: RecordFixture = { id: 1, scene_name: "营房", timer_seconds: 30, drops: [] };
  const tz: RecordFixture = { id: 2, scene_name: "营房", timer_seconds: 40, drops: [], tz: true };
  assert(matchesAreaType(normal, "all") && matchesAreaType(tz, "all"), "全部区域应保留普通与 TZ 记录");
  assert(matchesAreaType(normal, "normal") && !matchesAreaType(tz, "normal"), "普通区域筛选应排除 TZ 记录");
  assert(matchesAreaType(tz, "tz") && !matchesAreaType(normal, "tz"), "TZ 区域筛选应排除普通及缺失 tz 字段的旧记录");
}

{
  const records: RecordFixture[] = [
    { id: 1, journey_id: "countess", segment_index: 0, scene_name: "黑色荒地", timer_seconds: 20, drops: [] },
    { id: 2, journey_id: "countess", segment_index: 1, scene_name: "遗忘之塔地牢第1层", timer_seconds: 30, drops: [] },
    { id: 3, journey_id: "baal", segment_index: 0, scene_name: "世界之石要塞二层", timer_seconds: 40, drops: [] },
    { id: 4, journey_id: "baal", segment_index: 1, scene_name: "毁灭王座", timer_seconds: 50, drops: [] },
  ];
  const strategies: StrategyFixture[] = [
    { id: "short", name: "短路线", scene_names: ["黑色荒地"] },
    { id: "countess", name: "女伯爵", scene_names: ["黑色荒地", "遗忘之塔地牢第1层"] },
    { id: "baal", name: "巴尔", scene_names: ["世界之石要塞二层", "毁灭王座"] },
  ];
  const result = materializeRecords(records, strategies);
  assert(result.length === 2, "同时启用多条策略时应分别合并各自的连续行程");
  assert(result.map(record => record.scene_name).join(",") === "女伯爵,巴尔", "重叠策略应优先采用可覆盖更多连续分段的路线");
  assert(result.reduce((sum, record) => sum + (record._segment_count ?? 1), 0) === 4, "多选策略不能重复消费同一条原始分段");
  assert(result.reduce((sum, record) => sum + record.timer_seconds, 0) === 140, "多选合并后总时长必须与原始分段一致");
  const summary = summarize(result);
  assert(summary.runs === 2, "策略组的每个合并结果必须计为一场，而不是按原始分段计场次");
  assert(summary.totalSeconds === 140 && summary.average === 70, "策略组平均耗时应先汇总组内分段，再按合并后的场次数计算");
}

{
  assert(template.includes("常用 Farm 策略组 · 可多选") && template.includes("每个原始分段最多只会归入一个策略"), "统计策略应明确支持多选且不重复计数");
  assert(["女伯爵", "地穴", "安达利尔", "墨菲斯托", "Chaos", "巴尔", "尼拉塞克", "古代水道", "碎石古墓"].every(name => template.includes(`name:\"${name}\"`)), "统计页应内置常用 Farm 路线预设");
  assert(template.includes('id="filters-toggle"') && template.includes('aria-controls="filters-body"'), "统计筛选器应支持折叠并保持可访问状态");
  assert(template.includes('id="filter-outliers" type="checkbox" checked') && template.includes('id="filter-short-threshold" type="range"'), "离群优化应默认开启并提供短空场阈值滑条");
  assert(template.includes('data-strategy-edit="${escapeHtml(strategy.id)}"') && template.includes('method=editing?"PUT":"POST"') && template.includes("savePresetOverrides"), "预设和自定义策略都应支持逐项编辑与持久化");
  assert(template.includes("短空场 + 极端耗时优化（推荐）") && !template.includes("完整优化（推荐）"), "海报应以可理解的文字解释离群优化，不再使用含糊的“完整优化”");
  assert(template.includes('id="report-canvas" width="1080" height="1350"'), "图片战报应使用适合分享的 4:5 高清画布");
  assert(shareReportSource.includes("reportSourceRecords()") && shareReportSource.includes("state.records:state.rawRecords"), "图片战报应允许在原始分段与当前策略结果间选择");
  assert(!shareReportSource.includes("state.filtered"), "图片战报不应依赖统计页筛选结果");
  assert(shareReportSource.includes("reportGroupedScenes(records)") && shareReportSource.includes('tzGrouping==="separate"'), "图片战报应支持普通区域与 TZ 分开或合并统计");
  assert(!shareReportSource.includes("有产出场次") && !shareReportSource.includes("是否有产出"), "图片战报不应展示低价值的产出状态指标");
  assert(template.includes('id="open-report"'), "统计页顶部应保留图片战报入口");
  assert(template.includes("INITIAL_STATS_PREFERENCES") && template.includes('state.activeStrategyIds=Array.isArray(saved)?saved.map(String):PRESET_STRATEGIES.map(strategy=>strategy.id)'), "统计页应从共享偏好同步策略，并在首次使用时默认启用全部预设");
  assert(template.includes('id="report-config"') && template.includes("统计范围") && template.includes("展示内容") && template.includes("统计口径"), "图片战报应提供完整的配置页面");
  assert(template.includes('data-report-range="custom"') && template.includes('id="report-date-start"') && template.includes('id="report-date-end"'), "图片战报应支持自定义日期范围");
  assert(shareReportSource.includes("REPORT_METRIC_IDS") && shareReportSource.includes("slice(0,4)"), "图片战报应允许选择并限制顶部指标数量");
  assert(shareReportSource.includes("reportEditorialRule") && shareReportSource.includes("#cc785c"), "图片战报应使用暖色编辑式视觉语言");
  assert(shareReportSource.includes("每日战报") && shareReportSource.includes("今日小结"), "图片战报应保留清晰的战报与小结信息层级");
}

console.log("stats outlier optimizer tests passed");
