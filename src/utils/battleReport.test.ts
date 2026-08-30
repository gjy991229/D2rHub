import {
  BATTLE_REPORT_PRESET_STRATEGIES,
  buildBattleReportSnapshot,
  type BattleReportStatsData,
} from "./battleReport";

declare const __dirname: string;
declare function require(name: string): {
  readFileSync?: (filePath: string, encoding: string) => string;
  join?: (...parts: string[]) => string;
};

const fs = require("fs") as { readFileSync: (filePath: string, encoding: string) => string };
const path = require("path") as { join: (...parts: string[]) => string };

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(message);
}

const template = fs.readFileSync(path.join(__dirname, "../../docs/stats.html"), "utf8");
const templatePresetIds = [...template.matchAll(/\{id:"(preset-[^"]+)"/g)].map((match) => match[1]);
assert(
  JSON.stringify(templatePresetIds) === JSON.stringify(BATTLE_REPORT_PRESET_STRATEGIES.map((strategy) => strategy.id)),
  "主界面快捷战报与统计 HTML 必须使用同一组系统预设",
);

const stats: BattleReportStatsData = {
  records: [
    { id: 1, absolute_time: "2026/08/30/10:00:00", character_name: "Sor", scene_name: "黑色荒地", journey_id: "run-1", segment_index: 0, timer_seconds: 20, drops: [] },
    { id: 2, absolute_time: "2026/08/30/10:01:00", character_name: "Sor", scene_name: "遗忘之塔地牢第1层", journey_id: "run-1", segment_index: 1, timer_seconds: 30, drops: [] },
  ],
  strategies: [],
};
const now = new Date(2026, 7, 30, 12, 0, 0);
const defaultSnapshot = buildBattleReportSnapshot(stats, {}, now);
assert(defaultSnapshot.summary.runs === 1, "首次使用时全部预设应默认启用并合并路线分段");
assert(defaultSnapshot.summary.totalSeconds === 50, "策略合并不能改变战报总耗时");

const rawSnapshot = buildBattleReportSnapshot(stats, {
  activeStrategyIds: [],
  reportConfig: { dataSource: "strategy" },
}, now);
assert(rawSnapshot.summary.runs === 2, "用户明确清空策略后快捷战报应同步为原始分段口径");

console.log("battle report sharing tests passed");
