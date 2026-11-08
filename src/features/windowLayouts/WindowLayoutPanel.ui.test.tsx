import { useRef, useState } from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AccountMeta, GlobalConfig, LayoutMonitor, WindowLayout } from "../../store/types";
import { useAccounts } from "../../store/accounts";
import { useLaunch } from "../../store/launch";
import { WindowLayoutPanel } from "./WindowLayoutPanel";
import { presetSlots } from "./model";

const mocks = vi.hoisted(() => ({ capture: vi.fn(), monitors: vi.fn(), order: vi.fn(), persist: vi.fn(), restore: vi.fn(), toast: vi.fn() }));
vi.mock("./gateway", () => ({ captureGameLayout: mocks.capture, getLayoutMonitors: mocks.monitors, getLayoutAccountOrder: mocks.order, restoreGameLayout: mocks.restore }));
vi.mock("../../components/ui/Toast", () => ({ showToast: mocks.toast }));
const displays: LayoutMonitor[] = [
  { id: "main", name: "Main", primary: true, scale_factor: 1.5, bounds: { x: 0, y: 0, width: 2560, height: 1440 }, work_area: { x: 0, y: 0, width: 2560, height: 1400 } },
  { id: "left", name: "Left", primary: false, scale_factor: 1, bounds: { x: -1920, y: 0, width: 1920, height: 1080 }, work_area: { x: -1920, y: 0, width: 1920, height: 1040 } },
];
const saved: WindowLayout = { id: "desk", name: "日常三开", monitors: displays, windows: presetSlots(displays, 3, "2k") };
let latest: GlobalConfig;
function Harness({ layouts = [] as WindowLayout[], enabled = false }) {
  const draft = useRef({ window_layouts: layouts, active_window_layout_id: layouts[0]?.id ?? null, window_layout_enabled: enabled, app_language: "zh-CN" } as unknown as GlobalConfig);
  const [, refresh] = useState(0);
  latest = draft.current;
  return <WindowLayoutPanel config={draft.current} readDraft={() => draft.current}
    updateConfig={mutate => { const next = structuredClone(draft.current); mutate(next); draft.current = next; refresh(index => index + 1); }}
    persistConfig={mocks.persist} />;
}
beforeEach(() => {
  vi.clearAllMocks();
  mocks.monitors.mockResolvedValue(displays);
  mocks.order.mockResolvedValue([]);
  mocks.persist.mockImplementation(async (config: GlobalConfig) => config);
  mocks.restore.mockResolvedValue({ applied: ["account1"], failures: [] });
  useLaunch.setState({ launching: false });
  useAccounts.setState({ accounts: [
    { id: "first", display_name: "雷董2", initialized: true, is_running: false, order: 1 },
    { id: "second", display_name: "立山", initialized: true, is_running: false, order: 2 },
  ] as AccountMeta[] });
});
afterEach(cleanup);
const create = async () => {
  await screen.findByText("已识别 2 台显示器");
  fireEvent.click(screen.getByRole("button", { name: "新建布局" }));
};
describe("window layout library and editor", () => {
  it("lets the user leave delete confirmation while keeping the saved layout", async () => {
    render(<Harness layouts={[saved]} />);
    fireEvent.click(screen.getByRole("button", { name: "编辑布局：日常三开" }));
    fireEvent.click(await screen.findByRole("button", { name: "删除布局" }));
    expect(screen.getByRole("button", { name: "确认删除" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "保留布局" }));
    expect(screen.queryByRole("button", { name: "确认删除" })).toBeNull();
    expect(mocks.persist).not.toHaveBeenCalled();
    expect(latest.window_layouts).toEqual([saved]);
  });

  it("captures physical positions into a named draft and persists them without moving windows", async () => {
    const captured: WindowLayout = { ...saved, id: "capture", windows: [
      { monitor_id: "left", x: 120, y: 90, width: 1280, height: 720 },
      { monitor_id: "main", x: 180, y: 140, width: 1600, height: 900 },
    ] };
    mocks.capture.mockResolvedValue({ layout: captured, account_ids: ["second", "first"] });
    render(<Harness layouts={[{ ...saved, name: "当前窗口布局" }]} />);
    await screen.findByText("已识别 2 台显示器");
    fireEvent.click(screen.getByRole("button", { name: "捕获当前布局" }));
    const name = await screen.findByRole("textbox", { name: "布局名称" });
    expect((name as HTMLInputElement).value).toBe("当前窗口布局 2");
    expect((screen.getByRole("spinbutton", { name: "X" }) as HTMLInputElement).value).toBe("-1800");
    expect(mocks.persist).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "保存布局" }));
    await screen.findByRole("button", { name: "编辑布局：当前窗口布局 2" });
    expect(latest.window_layouts?.[1].windows).toEqual(captured.windows);
    expect(latest.active_window_layout_id).toBe(saved.id);
    expect(mocks.restore).not.toHaveBeenCalled();
  });
  it("leaves saved layouts intact when capture fails and allows retry", async () => {
    mocks.capture.mockRejectedValueOnce(new Error("请还原为普通窗口"));
    render(<Harness layouts={[saved]} />);
    await screen.findByText("已识别 2 台显示器");
    fireEvent.click(screen.getByRole("button", { name: "捕获当前布局" }));
    expect((await screen.findByRole("alert")).textContent).toContain("请还原为普通窗口");
    expect(latest.window_layouts).toEqual([saved]);
    expect(mocks.persist).not.toHaveBeenCalled();
    expect(screen.queryByRole("textbox", { name: "布局名称" })).toBeNull();
    expect((screen.getByRole("button", { name: "捕获当前布局" }) as HTMLButtonElement).disabled).toBe(false);
  });
  it("opens the library first and only shows editor controls after selecting a capsule", async () => {
    render(<Harness layouts={[saved]} enabled />);
    expect(screen.getByRole("switch", { name: "启用布局" }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText("当前应用")).toBeTruthy();
    expect(screen.queryByRole("textbox", { name: "布局名称" })).toBeNull();
    expect(screen.queryByLabelText("窗口布局画布")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "编辑布局：日常三开" }));
    expect(await screen.findByRole("textbox", { name: "布局名称" })).toBeTruthy();
    expect(screen.queryByLabelText("布局预设模板")).toBeNull();
    expect(screen.getByRole("button", { name: "雷董2，1600 × 900" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "立山，1280 × 720" })).toBeTruthy();
  });
  it("fills a local draft from a disclosed preset, then saves a capsule with feedback", async () => {
    render(<Harness />);
    await create();
    expect(latest.window_layouts).toEqual([]);
    expect(screen.queryByLabelText("布局预设模板")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "使用预设" }));
    fireEvent.click(screen.getByRole("button", { name: "填充 2K 8 开预设" }));
    expect(screen.queryByLabelText("布局预设模板")).toBeNull();
    expect(mocks.persist).not.toHaveBeenCalled();
    expect(latest.window_layouts).toEqual([]);
    fireEvent.click(screen.getByRole("button", { name: /^保存布局$/ }));
    await screen.findByRole("button", { name: "编辑布局：新布局" });
    expect(latest.window_layouts?.[0].windows).toHaveLength(8);
    expect(latest.window_layouts?.[0].windows.every(slot => slot.monitor_id === "main")).toBe(true);
    expect(latest.active_window_layout_id).toBeNull();
    expect(screen.getByRole("status").textContent).toContain("“新布局”已保存");
    expect(screen.queryByRole("textbox", { name: "布局名称" })).toBeNull();
    expect(mocks.restore).not.toHaveBeenCalled();
  });
  it("labels live slots in launch order before previewing idle accounts in card order", async () => {
    mocks.order.mockResolvedValue(["first", "later"]);
    useAccounts.setState({ accounts: [
      { id: "idle", display_name: "白可鉴", initialized: true, is_running: false, order: 0 },
      { id: "later", display_name: "立山", initialized: true, is_running: true, order: 1, last_launched_at: "2026-10-07T12:00:00Z" },
      { id: "first", display_name: "山鬼", initialized: true, is_running: true, order: 2, last_launched_at: "2026-10-08T12:00:00Z" },
    ] as AccountMeta[] });
    render(<Harness layouts={[saved]} />);
    fireEvent.click(screen.getByRole("button", { name: "编辑布局：日常三开" }));
    const order = await screen.findByRole("group", { name: "启动顺序对应账号" });
    expect(Array.from(order.querySelectorAll("button > span")).map(label => label.textContent)).toEqual(["山鬼", "立山", "白可鉴"]);
  });
  it("discards new and edited local drafts when returning without saving", async () => {
    render(<Harness layouts={[saved]} />);
    await create();
    fireEvent.click(screen.getByRole("button", { name: "返回布局列表" }));
    fireEvent.click(screen.getByRole("button", { name: "编辑布局：日常三开" }));
    fireEvent.change(screen.getByRole("textbox", { name: "布局名称" }), { target: { value: "不保存的名称" } });
    fireEvent.click(screen.getByRole("button", { name: "返回布局列表" }));
    expect(latest.window_layouts).toEqual([saved]);
    expect(mocks.persist).not.toHaveBeenCalled();
  });
  it("keeps a failed save in the editor without adding it to saved configurations", async () => {
    mocks.persist.mockResolvedValueOnce(null);
    render(<Harness />);
    await create();
    fireEvent.click(screen.getByRole("button", { name: "保存布局" }));
    expect((await screen.findByRole("alert")).textContent).toContain("保存失败，草稿已保留");
    expect(latest.window_layouts).toEqual([]);
    expect(screen.getByRole("textbox", { name: "布局名称" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "保存布局" }));
    await screen.findByRole("button", { name: "编辑布局：新布局" });
    expect(mocks.persist).toHaveBeenCalledTimes(2);
  });
  it("supports game resolution, negative display coordinates, and keyboard movement", async () => {
    render(<Harness />);
    await create();
    fireEvent.change(screen.getByRole("combobox", { name: "游戏分辨率" }), { target: { value: "800x600" } });
    fireEvent.blur(screen.getByRole("combobox", { name: "游戏分辨率" }));
    fireEvent.change(screen.getByRole("combobox", { name: "所在显示器" }), { target: { value: "left" } });
    fireEvent.change(screen.getByRole("spinbutton", { name: "X" }), { target: { value: "-1800" } });
    fireEvent.blur(screen.getByRole("spinbutton", { name: "X" }));
    fireEvent.keyDown(screen.getByRole("button", { name: "雷董2，800 × 600" }), { key: "ArrowRight", ctrlKey: true });
    fireEvent.keyDown(screen.getByRole("button", { name: "雷董2，800 × 600" }), { key: "ArrowDown", shiftKey: true });
    fireEvent.click(screen.getByRole("button", { name: "保存布局" }));
    await waitFor(() => expect(latest.window_layouts?.[0].windows[0]).toMatchObject({ monitor_id: "left", x: 121, width: 800, height: 610 }));
  });
  it("saves and reopens negative border offsets on the primary monitor", async () => {
    render(<Harness />);
    await create();
    for (const [axis, value] of [["X", "-8"], ["Y", "-12"]]) {
      const field = screen.getByRole("spinbutton", { name: axis });
      fireEvent.change(field, { target: { value } });
      fireEvent.blur(field);
    }
    fireEvent.click(screen.getByRole("button", { name: "保存布局" }));
    const edit = await screen.findByRole("button", { name: "编辑布局：新布局" });
    expect(latest.window_layouts?.[0].windows[0]).toMatchObject({ x: -8, y: -12 });
    fireEvent.click(edit);
    expect((screen.getByRole("spinbutton", { name: "X" }) as HTMLInputElement).value).toBe("-8");
    expect((screen.getByRole("spinbutton", { name: "Y" }) as HTMLInputElement).value).toBe("-12");
  });
  it("preserves full display resolution despite the taskbar and a smaller monitor", async () => {
    render(<Harness />);
    await create();
    fireEvent.change(screen.getByRole("combobox", { name: "游戏分辨率" }), { target: { value: "2560x1440" } });
    fireEvent.blur(screen.getByRole("combobox", { name: "游戏分辨率" }));
    fireEvent.change(screen.getByRole("combobox", { name: "所在显示器" }), { target: { value: "left" } });
    expect((screen.getByRole("combobox", { name: "游戏分辨率" }) as HTMLInputElement).value).toBe("2560x1440");
    fireEvent.click(screen.getByRole("button", { name: "保存布局" }));
    await waitFor(() => expect(latest.window_layouts?.[0].windows[0]).toMatchObject({ monitor_id: "left", x: 0, y: 0, width: 2560, height: 1440 }));
  });
  it("saves and applies explicitly, and marks the applied capsule", async () => {
    render(<Harness />);
    await create();
    fireEvent.click(screen.getByRole("button", { name: "保存并使用布局" }));
    await waitFor(() => expect(mocks.restore).toHaveBeenCalledTimes(1));
    expect(latest.window_layout_enabled).toBe(true);
    expect(latest.active_window_layout_id).toBe(latest.window_layouts?.[0].id);
    expect(screen.getByText("当前应用")).toBeTruthy();
    expect(screen.getByRole("status").textContent).toContain("已应用到 1 个窗口");
  });
  it("shows detection progress and its result", async () => {
    render(<Harness />);
    await screen.findByText("已识别 2 台显示器");
    let finish!: (value: LayoutMonitor[]) => void;
    mocks.monitors.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
    fireEvent.click(screen.getByRole("button", { name: "检测显示器" }));
    expect((screen.getByRole("button", { name: "检测中…" }) as HTMLButtonElement).disabled).toBe(true);
    finish(displays);
    await screen.findByText("检测完成，已识别 2 台显示器");
  });
  it("shows monitor-read failures and keeps creation disabled", async () => {
    mocks.monitors.mockRejectedValueOnce(new Error("display unavailable"));
    render(<Harness />);
    expect((await screen.findByRole("alert")).textContent).toContain("display unavailable");
    expect((screen.getByRole("button", { name: "新建布局" }) as HTMLButtonElement).disabled).toBe(true);
    expect(latest.window_layouts).toEqual([]);
  });
});
