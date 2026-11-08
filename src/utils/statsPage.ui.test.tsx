import { afterEach, expect, it } from "vitest";
import { fireEvent, screen } from "@testing-library/react";
import template from "../../docs/stats.html?raw";

afterEach(() => { document.body.innerHTML = ""; });

it("keeps statistics tabs keyboard accessible and links each tab to its named panel", () => {
  const parsed = new DOMParser().parseFromString(template, "text/html");
  document.body.innerHTML = parsed.body.innerHTML;
  const source = template.split("// VIEW_TABS_START")[1].split("// VIEW_TABS_END")[0];
  const bind = new Function("$", "$$", "state", `${source}; return bindViewTabs;`)(
    (selector: string) => document.querySelector(selector),
    (selector: string) => Array.from(document.querySelectorAll(selector)),
    { activeView: "overview" },
  );
  bind();
  const overview = screen.getByRole("tab", { name: "总览" });
  overview.focus();
  fireEvent.keyDown(overview, { key: "ArrowRight" });
  const runes = screen.getByRole("tab", { name: "符文" });
  expect(document.activeElement).toBe(runes);
  expect(runes.getAttribute("aria-selected")).toBe("true");
  expect(overview.tabIndex).toBe(-1);
  expect(screen.getByRole("tabpanel", { name: "符文" }).id).toBe(runes.getAttribute("aria-controls"));
  fireEvent.keyDown(runes, { key: "End" });
  expect(document.activeElement).toBe(screen.getByRole("tab", { name: "场次记录" }));
  expect(screen.queryByRole("tabpanel", { name: "符文" })).toBeNull();
});

const runSamples = [
  { scene_name: "深坑", timer_seconds: 60, character_name: "主号", absolute_time: "2026-11-09T00:00:00", drops: [] },
  { scene_name: "混沌避难所", timer_seconds: 120, character_name: "主号", absolute_time: "2026-11-09T00:05:00", drops: [{ rune_number: 30 }] },
  { scene_name: "高塔", timer_seconds: 180, character_name: "小号", absolute_time: "2026-11-09T00:10:00", drops: [] },
];

function recentRunMarkup(records = runSamples) {
  const source = template.split("// RUN_DURATION_CHART_START")[1].split("// RUN_DURATION_CHART_END")[0];
  const render = new Function("statDuration", "quantile", "formatDuration", "getDrops", "isHighRune", "escapeHtml",
    `${source}; return buildRecentRunChart;`)(
    (record: typeof runSamples[number]) => record.timer_seconds,
    (values: number[]) => values.slice().sort((a, b) => a - b)[Math.floor(values.length / 2)],
    (seconds: number) => `${seconds} 秒`,
    (record: typeof runSamples[number]) => record.drops,
    (drop: { rune_number: number }) => drop.rune_number >= 24,
    String,
  );
  return render(records, records.length);
}

it("shows explicit time and run labels with bar heights proportional to duration", () => {
  document.body.innerHTML = recentRunMarkup();
  expect(screen.getByText("中位耗时 120 秒")).toBeTruthy();
  expect(screen.getByText("1分")).toBeTruthy();
  expect(screen.getByText("第 3 场 · 最近")).toBeTruthy();
  expect(screen.getAllByRole("button")).toHaveLength(3);
  const bars = [...document.querySelectorAll(".run-duration-bar")];
  const heights = bars.map(bar => Number(bar.getAttribute("height")));
  expect(heights[0] / heights[1]).toBeCloseTo(.5);
  expect(heights[2] / heights[1]).toBeCloseTo(1.5);
  expect(bars.map(bar => Number(bar.getAttribute("y")) + Number(bar.getAttribute("height"))))
    .toEqual([202, 202, 202]);
  expect(bars[1].getAttribute("fill")).toBe("var(--violet)");
  expect(document.querySelectorAll(".run-drop-marker")).toHaveLength(1);
  expect(recentRunMarkup([])).toContain("暂无有效耗时");
});

it("keeps per-run details available through bar clicks and keyboard activation", () => {
  document.body.innerHTML = `<div id="run-scatter">${recentRunMarkup()}</div><div id="run-detail"></div>`;
  const source = template.slice(template.indexOf("    function showExplorerRun("), template.indexOf("    function renderAll(){"));
  new Function("$", "window", "formatDate", "formatDuration", "statDuration", "getDrops", source)(
    (selector: string) => document.querySelector(selector),
    { explorerRecords: runSamples },
    String,
    (seconds: number) => `${seconds} 秒`,
    (record: typeof runSamples[number]) => record.timer_seconds,
    (record: typeof runSamples[number]) => record.drops,
  );
  fireEvent.click(document.querySelectorAll(".run-duration-bar")[1]);
  expect(document.querySelector("#run-detail")?.textContent).toContain("混沌避难所 · 主号");
  expect(document.querySelector("#run-detail")?.textContent).toContain("120 秒 · 1 次掉落");
  fireEvent.keyDown(screen.getAllByRole("button")[2], { key: "Enter" });
  expect(document.querySelector("#run-detail")?.textContent).toContain("高塔 · 小号");
});
