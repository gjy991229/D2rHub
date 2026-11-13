import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ModResourceLibrary } from "./ModResourceLibrary";
import { ModProcessorStatus } from "./ModProcessorStatus";
import type { TaskSnapshot } from "../../tasks/types";
import type { ModCapsuleController } from "../../modCapsules/useModCapsulePool";
const invoke = vi.hoisted(() => vi.fn());
const picker = vi.hoisted(() => vi.fn());
const subscribe = vi.hoisted(() => vi.fn());
vi.mock("../../../platform/tauri", () => ({ invokeCommand: invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: picker }));
vi.mock("../../tasks/taskSync", () => ({ subscribeBeforeReadingTasks: subscribe }));
vi.mock("../../modResources/visionResourcesGateway", () => ({ visionResourcesGateway: {
  read: async () => ({ ready: true, preparing: false, progress: 100, message: "资源包已就绪", size: 88999851, path: "C:\\resources\\vision" }),
  prepare: vi.fn(), openFolder: vi.fn(),
} }));
const state = () => ({
  catalog: { release_url: "https://github.com/gjy991229/D2rHub/releases", assets: [
    { id: "processor", version: "1.4.0-beta.17", size: 3000000, url: "https://example.test/processor", game_data_version: null },
    { id: "LiteHub", version: "r1", size: 24000000, url: "https://example.test/mod", game_data_version: "93854" },
  ] },
  processor: { ready: false, installed_version: "1.3.3", recommended_version: "1.4.0-beta.17", installed_path: "C:\\old\\d2r-audio-mod.exe", install_directory: "C:\\User\\tools", legacy: true },
  mods_directory: "D:\\Game\\mods", game_data_version: "93854", warning: null,
});
const resourceTask = (overrides: Partial<TaskSnapshot> = {}): TaskSnapshot => ({
  revision: 1, task_id: 42, kind: "mod-resource-install", subject: "CN:LiteHub", conflict_key: null,
  state: "running", progress: 37, step: "verify", message: "正在校验", error_code: null,
  cancel_requested: false, retryable: false, retry_of: null, started_at_ms: 1, finished_at_ms: null,
  ...overrides,
});
beforeEach(() => { subscribe.mockReset().mockResolvedValue(() => {}); invoke.mockReset(); picker.mockReset(); invoke.mockImplementation(async (cmd: string) => cmd === "get_mod_resources" ? state() : { path: "C:\\User\\tools\\processor.exe" }); });
afterEach(cleanup);
describe("Mod resources", () => {
  it.each([[false, "当前已是最新版"], [true, "更新 Mod"]] as const)("allows replacing an installed Mod with update=%s", async (updateAvailable, label) => {
    const data = { ...state(), game_data_version: "99999", mods: [{ id: "LiteHub", installed_version: "r1", update_available: updateAvailable, protected: true, reason_code: "replace_available" }] };
    invoke.mockImplementation(async cmd => cmd === "get_mod_resources" ? data : { path: "D:\\Game\\mods\\LiteHub" });
    render(<ModResourceLibrary edition="CN" en={false} />);
    const button = await screen.findByRole("button", { name: label });
    expect((button as HTMLButtonElement).disabled).toBe(false);
    await userEvent.click(button);
    expect(invoke).toHaveBeenCalledWith("install_mod_resource", expect.objectContaining({ resourceId: "LiteHub" }));
  });

  it("orders Mods by memory reference and hides technical installation details", async () => {
    const data = state();
    invoke.mockResolvedValue({ ...data, catalog: { ...data.catalog, assets: [...data.catalog.assets,
      { ...data.catalog.assets[1], id: "BoHub" }, { ...data.catalog.assets[1], id: "NullHub" }] },
      mods: [{ id: "LiteHub", installed_version: "private-release-id", update_available: false }] });
    render(<ModResourceLibrary edition="CN" en={false} />);
    await screen.findByText("LiteHub");
    expect(screen.getAllByRole("article").map(card => card.getAttribute("aria-label"))).toEqual(["NullHub", "BoHub", "LiteHub", "视觉识别资源包"]);
    for (const [name, memory] of [["NullHub", 300], ["BoHub", 500], ["LiteHub", 800]] as const) {
      const card = screen.getByRole("article", { name });
      expect(within(card).getByText(`约 ${memory} MB`)).toBeTruthy();
      expect(within(card).getByRole("button", { name: "打开目录" })).toBeTruthy();
    }
    expect(screen.queryByText("安装位置")).toBeNull();
    expect(screen.queryByText("private-release-id")).toBeNull();
    expect(screen.queryByText(/C:\\old/)).toBeNull();
  });
  it("trusts a fresh missing-resource status over a stale scanned catalog", async () => {
    invoke.mockResolvedValue({ ...state(), mods: [{ id: "LiteHub", installed_version: null,
      update_available: false, protected: false, message: "", reason_code: "not_installed" }] });
    const catalog = { pool: { capsules: [{ edition: "CN", origin: "scanned", name: "LiteHub" }] } } as ModCapsuleController;
    render(<ModResourceLibrary edition="CN" en={false} catalog={catalog} />);
    const card = (await screen.findByText("LiteHub")).closest("article")!;
    expect(within(card).getByRole("button", { name: "下载并安装" })).toHaveProperty("disabled", false);
  });
  it("does not leave stale task locks when the task subscription fails", async () => {
    subscribe.mockImplementation(async (_gateway, listener) => {
      listener(new Map([[42, resourceTask()]]));
      throw new Error("Task list unavailable");
    });
    render(<ModResourceLibrary edition="CN" en={false} />);
    const card = (await screen.findByText("LiteHub")).closest("article")!;
    await waitFor(() => expect(within(card).getByRole("button", { name: "下载并安装" })).toHaveProperty("disabled", false));
    expect(screen.getByRole("button", { name: "检查更新" })).toHaveProperty("disabled", false);
    expect(screen.getByText(/暂时无法读取任务进度/)).toBeTruthy();
  });
  it("presents a cancelled installation neutrally without hiding a later folder error", async () => {
    let publish!: (snapshot: ReadonlyMap<number, TaskSnapshot>) => void;
    let rejectInstall!: (error: Error) => void;
    subscribe.mockImplementation(async (_gateway, listener) => { publish = listener; return () => {}; });
    invoke.mockImplementation((cmd: string) => cmd === "get_mod_resources" ? Promise.resolve(state())
      : cmd === "install_mod_resource" ? new Promise((_resolve, reject) => { rejectInstall = reject; })
        : Promise.reject(new Error("Folder unavailable")));
    render(<ModResourceLibrary edition="CN" en={false} />);
    const card = (await screen.findByText("LiteHub")).closest("article")!;
    await userEvent.click(within(card).getByRole("button", { name: "下载并安装" }));
    await act(async () => {
      publish(new Map([[42, resourceTask({ state: "cancelled", message: "用户取消" })]]));
      rejectInstall(new Error("用户取消"));
    });
    expect(within(card).getByText("安装已取消，可以重新开始。")).toBeTruthy();
    expect(within(card).queryByRole("alert")).toBeNull();
    await userEvent.click(within(card).getByText("手动下载"));
    await userEvent.click(within(card).getByRole("button", { name: "打开目录" }));
    expect((await within(card).findByRole("alert")).textContent).toContain("Folder unavailable");
  });
  it("reads locally on every visit and only refreshes online when requested", async () => {
    const first = render(<ModResourceLibrary edition="CN" en={false} />);
    await screen.findByText("LiteHub"); first.unmount();
    render(<ModResourceLibrary edition="CN" en={false} />);
    await screen.findByText("LiteHub");
    expect(invoke.mock.calls.filter(([cmd]) => cmd === "get_mod_resources").every(([, args]) => args.refresh === false)).toBe(true);
    await userEvent.click(screen.getByRole("button", { name: "检查更新" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_mod_resources", { edition: "CN", refresh: true }));
  });
  it("stops loading after an initial read failure and lets the user retry", async () => {
    invoke.mockRejectedValueOnce(new Error("资源目录暂时不可用"));
    render(<ModResourceLibrary edition="CN" en={false} />);

    expect((await screen.findByRole("alert")).textContent).toContain("资源目录暂时不可用");
    expect(screen.queryByText("正在检查本地资源…")).toBeNull();
    const retry = screen.getByRole("button", { name: "检查更新" }) as HTMLButtonElement;
    expect(retry.disabled).toBe(false);
    await userEvent.click(retry);

    expect(await screen.findByText("LiteHub")).toBeTruthy();
    expect(invoke).toHaveBeenLastCalledWith("get_mod_resources", { edition: "CN", refresh: true });
    expect(screen.queryByRole("alert")).toBeNull();
  });
  it("keeps a current processor quiet in the processing view", async () => {
    invoke.mockResolvedValue({ ready: true });
    const ready = vi.fn();
    const view = render(<ModProcessorStatus edition="CN" en={false} onReady={ready} />);
    await waitFor(() => expect(ready).toHaveBeenLastCalledWith(true));
    expect(view.container.textContent).toBe("");
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("get_bundled_processor_status");
  });
  it("blocks processing when the bundled processor is missing", async () => {
    invoke.mockResolvedValue({ ready: false, blocking_reason: "内置 Mod 加工器缺失，请修复 D2RHub" });
    const ready = vi.fn();
    render(<ModProcessorStatus edition="CN" en={false} onReady={ready} />);
    await waitFor(() => expect(ready).toHaveBeenLastCalledWith(false));
    expect(screen.getByText(/内置 Mod 加工器缺失/)).toBeTruthy();
    expect(screen.queryByText(/仍可/)).toBeNull();
  });
  it("shows an installation failure without a download action", async () => {
    const reason = "内置 Mod 加工器缺失，请修复 D2RHub。";
    invoke.mockResolvedValue({ ready: false, blocking_reason: reason });
    const ready = vi.fn();
    render(<ModProcessorStatus edition="CN" en={false} onReady={ready} />);
    expect(await screen.findByText(reason)).toBeTruthy();
    expect(ready).toHaveBeenLastCalledWith(false);
    expect(screen.queryByRole("button", { name: "下载与更新" })).toBeNull();
  });
  it("shows immediate feedback inside the clicked resource card before a task arrives", async () => {
    invoke.mockImplementation((cmd: string) => cmd === "get_mod_resources" ? Promise.resolve(state()) : new Promise(() => {}));
    render(<ModResourceLibrary edition="CN" en={false} />);
    const card = (await screen.findByText("LiteHub")).closest("article")!;
    await userEvent.click(within(card).getByRole("button", { name: "下载并安装" }));
    expect(within(card).getByText("正在准备…")).toBeTruthy();
    expect(within(card).getByRole("progressbar", { name: "LiteHub 进度" }).hasAttribute("aria-valuenow")).toBe(false);
    expect(within(card).getByRole("button", { name: "正在处理…" }).hasAttribute("disabled")).toBe(true);
    expect(screen.queryByRole("article", { name: "Mod 加工器" })).toBeNull();
  });
  it("reattaches running progress to its resource after returning to the page", async () => {
    subscribe.mockImplementation(async (_gateway, listener) => {
      listener(new Map([[42, { task_id: 42, kind: "mod-resource-install", subject: "CN:LiteHub", state: "running", progress: 37, message: "正在校验", cancel_requested: false }]]));
      return () => {};
    });
    render(<ModResourceLibrary edition="CN" en={false} />);
    const card = (await screen.findByText("LiteHub")).closest("article")!;
    expect(within(card).getByRole("progressbar").getAttribute("aria-valuenow")).toBe("37");
    expect(within(card).getByText("正在校验")).toBeTruthy();
    expect(within(card).getByRole("button", { name: "取消" })).toBeTruthy();
  });
  it("updates the parent busy state when an observed background task starts and finishes", async () => {
    let publish!: (snapshot: ReadonlyMap<number, TaskSnapshot>) => void;
    subscribe.mockImplementation(async (_gateway, listener) => { publish = listener; return () => {}; });
    const onBusy = vi.fn();
    render(<ModResourceLibrary edition="CN" en={false} onBusy={onBusy} />);
    await screen.findByText("LiteHub");
    expect(onBusy).toHaveBeenLastCalledWith(false);

    await act(async () => publish(new Map([[42, resourceTask()]])));
    expect(onBusy).toHaveBeenLastCalledWith(true);
    expect((screen.getByRole("button", { name: "检查更新" }) as HTMLButtonElement).disabled).toBe(true);

    await act(async () => publish(new Map([[42, resourceTask({ revision: 2, state: "succeeded", progress: 100, message: "已安装" })]])));
    expect(onBusy).toHaveBeenLastCalledWith(false);
    expect(screen.queryByRole("progressbar")).toBeNull();
  });
  it("releases the parent busy state and subscription when leaving during a running task", async () => {
    const stop = vi.fn();
    subscribe.mockImplementation(async (_gateway, listener) => {
      listener(new Map([[42, resourceTask()]]));
      return stop;
    });
    const onBusy = vi.fn();
    const view = render(<ModResourceLibrary edition="CN" en={false} onBusy={onBusy} />);
    await screen.findByText("LiteHub");
    expect(onBusy).toHaveBeenLastCalledWith(true);

    view.unmount();
    expect(onBusy).toHaveBeenLastCalledWith(false);
    expect(stop).toHaveBeenCalledTimes(1);
  });
  it("ignores a legacy processor asset in an old cached resource response", async () => {
    render(<ModResourceLibrary edition="Global" en={false} />);
    await screen.findByText("LiteHub");
    expect(screen.queryByRole("article", { name: "Mod 加工器" })).toBeNull();
    expect(screen.queryByRole("button", { name: "更新加工器" })).toBeNull();
  });
  it("allows packages with a different game data version", async () => {
    invoke.mockResolvedValue({ ...state(), game_data_version: "99999" });
    render(<ModResourceLibrary edition="CN" en={false} />);
    await screen.findByText("LiteHub");
    const card = screen.getByText("LiteHub").closest("article")!;
    expect((within(card).getByRole("button", { name: "下载并安装" }) as HTMLButtonElement).disabled).toBe(false);
    expect(screen.queryByRole("button", { name: "更新加工器" })).toBeNull();
  });
  it("restores a failed background installation after reopening the library", async () => {
    subscribe.mockImplementation(async (_gateway, listener) => {
      listener(new Map([[42, resourceTask({ state: "failed", message: "文件校验失败，请重试" })]]));
      return () => {};
    });
    const first = render(<ModResourceLibrary edition="CN" en={false} />);
    await screen.findByText("LiteHub");
    expect(screen.getByRole("alert").textContent).toContain("文件校验失败");
    first.unmount();
    render(<ModResourceLibrary edition="CN" en={false} />);
    const card = (await screen.findByText("LiteHub")).closest("article")!;
    expect(within(card).getByRole("alert").textContent).toContain("文件校验失败");
    expect(within(card).getByRole("button", { name: "下载并安装" })).toHaveProperty("disabled", false);
  });
  it("offers replacement with clear localized wording", async () => {
    invoke.mockResolvedValue({ ...state(), mods: [{ id: "LiteHub", installed_version: "r1",
      update_available: true, protected: false, message: "已安装；下载将覆盖同名 Mod", reason_code: "replace_available", integrity_checked: false }] });
    render(<ModResourceLibrary edition="CN" en />);
    const card = (await screen.findByText("LiteHub")).closest("article")!;
    expect(within(card).getByText(/Downloading replaces/)).toBeTruthy();
    expect(within(card).getByRole("button", { name: "Update Mod" })).toHaveProperty("disabled", false);
    expect(within(card).queryByText("本地文件已修改")).toBeNull();
  });
  it("ignores an old edition read that resolves after a new edition", async () => {
    let finish!: (value: ReturnType<typeof state>) => void;
    invoke.mockImplementation((cmd: string, args: { edition?: string }) => cmd === "get_mod_resources" && args.edition === "CN"
      ? new Promise(resolve => { finish = resolve; })
      : Promise.resolve({ ...state(), mods_directory: "G:\\Global\\mods" }));
    const view = render(<ModResourceLibrary edition="CN" en={false} />);
    view.rerender(<ModResourceLibrary edition="Global" en={false} />);
    await screen.findByText("LiteHub");
    await act(async () => finish(state()));
    const card = screen.getByText("LiteHub").closest("article")!;
    await userEvent.click(within(card).getByText("手动下载"));
    await userEvent.click(within(card).getByRole("button", { name: "打开目录" }));
    expect(invoke).toHaveBeenLastCalledWith("open_mods_directory", { edition: "Global" });
    expect(screen.queryByText("D:\\Game\\mods\\LiteHub")).toBeNull();
  });
  it("does not install into a stale edition after the native file picker returns", async () => {
    let choose!: (path: string) => void;
    picker.mockImplementation(() => new Promise(resolve => { choose = resolve; }));
    const view = render(<ModResourceLibrary edition="CN" en={false} />);
    await userEvent.click(await screen.findByText("手动下载"));
    await userEvent.click(screen.getByRole("button", { name: "导入已下载文件" }));
    view.rerender(<ModResourceLibrary edition="Global" en={false} />);
    await act(async () => choose("D:\\Downloads\\processor.exe"));
    expect(invoke.mock.calls.some(([command]) => command === "install_mod_resource")).toBe(false);
  });
  it("keeps a completed installation visible when the account catalog refresh fails", async () => {
    const refresh = vi.fn(async () => { throw new Error("账号刷新失败"); });
    render(<ModResourceLibrary edition="CN" en={false} catalog={{ refresh, pool: null } as never} />);
    const card = (await screen.findByText("LiteHub")).closest("article")!;
    await userEvent.click(within(card).getByRole("button", { name: "下载并安装" }));
    expect(await within(card).findByText("安装完成，可通过“打开目录”查看文件。")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("账号刷新失败");
  });
  it("imports a selected ZIP through the same verified installer", async () => {
    picker.mockResolvedValue("D:\\Downloads\\LiteHub.zip");
    const user = userEvent.setup(); render(<ModResourceLibrary edition="CN" en={false} />);
    await user.click(await screen.findByText("手动下载"));
    await user.click(screen.getByRole("button", { name: "导入已下载文件" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("install_mod_resource", { edition: "CN", resourceId: "LiteHub", localFile: "D:\\Downloads\\LiteHub.zip" }));
  });
});
