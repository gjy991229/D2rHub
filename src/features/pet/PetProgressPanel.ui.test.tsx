import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PetPanel } from "../settings/panels/PetPanel";
import { PetProgressPanel } from "./PetProgressPanel";
import { achievementProgress } from "./progress";
import { PET_ITEM_BY_ID, PET_ITEMS } from "./catalog";
import { CHATTER_LINES } from "./chatterData";
import { PetAccessory } from "./PetAccessory";
import { renderToStaticMarkup } from "react-dom/server";
import type { GlobalConfig } from "../../store/types";
import type { PetWardrobe } from "./types";

const state = vi.hoisted(() => ({
  refresh: vi.fn(async () => {}),
  value: { snapshot: { wardrobe: {
    owned: ["cloth-cap", "red-scarf", "bell", "rhythm-wraps"], equipped: {}, presets: [],
    seconds: 3660, inputs: 12345, days: 3, last_day: "2020-01-01", daily_rolls: 12,
    roll_seconds: 120, misses: 2, fragments: 14, tone: "mixed",
  } }, error: null as string | null },
}));
vi.mock("./wardrobeStore", () => ({
  usePetWardrobe: () => state.value,
  connectWardrobe: () => () => {},
  settlePetActivity: state.refresh,
  reloadWardrobe: vi.fn(),
  changeWardrobe: vi.fn(),
}));
afterEach(() => { cleanup(); state.value.error = null; });

describe("pet achievement journal", () => {
  it("renders every catalog reward in all poses and provides both chatter tones", () => {
    expect(new Set(PET_ITEMS.map(item => item.id)).size).toBe(PET_ITEMS.length);
    for (const item of PET_ITEMS) {
      for (const frame of ["up", "left", "right"] as const) {
        expect(renderToStaticMarkup(<svg><PetAccessory id={item.id} frame={frame} /></svg>), `${item.id}/${frame}`).not.toBe("<svg></svg>");
      }
      for (const topic of item.tags) for (const tone of ["gentle", "snarky"]) {
        expect(CHATTER_LINES.some(line => line.topic === topic && line.tone === tone && line.zh && line.en), `${topic}/${tone}`).toBe(true);
      }
    }
  });
  it("shows persistent totals, non-consecutive milestones and automatic rewards", () => {
    render(<PetProgressPanel />);
    expect(screen.getByText("12,345")).toBeTruthy();
    expect(screen.getByText("1 小时 1 分钟")).toBeTruthy();
    expect(screen.getByText("4 / 43")).toBeTruthy();
    expect(screen.getByText("键鼠敲击 10,000 / 10,000 次")).toBeTruthy();
    expect(screen.getByText("陪伴 3 / 30 天（无需连续）")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "已达成" }));
    expect(screen.getAllByRole("article")).toHaveLength(1);
    expect(screen.getByText("万次合拍·节奏护腕")).toBeTruthy();
    expect(screen.getByText("奖励：专属饰品 + 12 碎片")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "刷新统计" }));
    expect(state.refresh).toHaveBeenCalled();
  });

  it("keeps wardrobe, stats and behavior within the pet's own navigation", () => {
    render(<PetPanel config={{ app_language: "zh-CN", enable_bongo_cat: true } as GlobalConfig} windowPlacementBusy={null} updateConfig={vi.fn()} persistConfig={vi.fn()} onLocate={vi.fn()} />);
    const nav = screen.getByRole("navigation", { name: "小猫设置分页" });
    expect(screen.getByRole("region", { name: "装扮衣柜" })).toBeTruthy();
    fireEvent.click(within(nav).getByRole("button", { name: "成就与统计" }));
    expect(screen.getByText("累计键鼠敲击")).toBeTruthy();
    expect(screen.queryByRole("region", { name: "装扮衣柜" })).toBeNull();
    fireEvent.click(within(nav).getByRole("button", { name: "行为设置" }));
    expect(screen.getByText("猫咪显示缩放")).toBeTruthy();
    expect(screen.queryByText("累计键鼠敲击")).toBeNull();
  });

  it("renders English labels and storage errors", () => {
    state.value.error = "Save failed";
    render(<PetProgressPanel english />);
    expect(screen.getByText("Total inputs")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toBe("Save failed");
    expect(screen.getByText("10,000 / 10,000 keyboard & mouse inputs")).toBeTruthy();
  });

  it("counts distinct known accessories and handles older snapshots", () => {
    const w = { ...state.value.snapshot.wardrobe, owned: ["bell", "bell", "unknown"], inputs: undefined } as unknown as PetWardrobe;
    expect(achievementProgress(PET_ITEM_BY_ID.get("sun-crown")!, w)).toBe(1);
    expect(achievementProgress(PET_ITEM_BY_ID.get("rhythm-wraps")!, w)).toBe(0);
  });
});
