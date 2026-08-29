declare const require: (name: string) => unknown;
declare const __dirname: string;

type Drop = { display_name?: string };
type RecordFixture = {
  id: number;
  scene_name: string;
  timer_seconds: number;
  drops: Drop[];
  tz?: boolean;
  _outlier_status?: string;
  _stats_timer_seconds?: number;
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
`)() as (records: RecordFixture[], enabled: boolean) => OptimizerResult;
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
  assert(template.includes('id="report-canvas" width="1080" height="1350"'), "图片战报应使用适合分享的 4:5 高清画布");
  assert(shareReportSource.includes("reportSourceRecords()") && shareReportSource.includes("state.records:state.rawRecords"), "图片战报应允许在原始分段与当前策略结果间选择");
  assert(!shareReportSource.includes("state.filtered"), "图片战报不应依赖统计页筛选结果");
  assert(shareReportSource.includes("reportGroupedScenes(records)") && shareReportSource.includes('tzGrouping==="separate"'), "图片战报应支持普通区域与 TZ 分开或合并统计");
  assert(!shareReportSource.includes("有产出场次") && !shareReportSource.includes("是否有产出"), "图片战报不应展示低价值的产出状态指标");
  assert(template.includes('id="open-report"'), "统计页顶部应保留图片战报入口");
  assert(template.includes('id="report-config"') && template.includes("统计范围") && template.includes("展示内容") && template.includes("统计口径"), "图片战报应提供完整的配置页面");
  assert(template.includes('data-report-range="custom"') && template.includes('id="report-date-start"') && template.includes('id="report-date-end"'), "图片战报应支持自定义日期范围");
  assert(shareReportSource.includes("REPORT_METRIC_IDS") && shareReportSource.includes("slice(0,4)"), "图片战报应允许选择并限制顶部指标数量");
  assert(shareReportSource.includes("reportEditorialRule") && shareReportSource.includes("#cc785c"), "图片战报应使用暖色编辑式视觉语言");
  assert(shareReportSource.includes("每日战报") && shareReportSource.includes("今日小结"), "图片战报应保留清晰的战报与小结信息层级");
}

console.log("stats outlier optimizer tests passed");
