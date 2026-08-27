declare const require: (name: string) => unknown;
declare const __dirname: string;

type Drop = { display_name?: string };
type RecordFixture = {
  id: number;
  scene_name: string;
  timer_seconds: number;
  drops: Drop[];
  _outlier_status?: string;
  _stats_timer_seconds?: number;
};
type OptimizerResult = {
  records: RecordFixture[];
  summary: { eligibleScenes: number; ignored: number; adjusted: number };
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
  assert(result.records.length === 20 && result.summary.ignored === 1, "低于平均耗时 0.1 倍的无产出场次应被忽略");
}

console.log("stats outlier optimizer tests passed");
