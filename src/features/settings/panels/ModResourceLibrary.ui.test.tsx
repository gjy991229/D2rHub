import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ModResourceLibrary } from "./ModResourceLibrary";
import { ModProcessorStatus } from "./ModProcessorStatus";
const invoke = vi.hoisted(() => vi.fn());
const picker = vi.hoisted(() => vi.fn());
const subscribe = vi.hoisted(() => vi.fn());
vi.mock("../../../platform/tauri", () => ({ invokeCommand: invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: picker }));
vi.mock("../../tasks/taskSync", () => ({ subscribeBeforeReadingTasks: subscribe }));
const state = () => ({
  catalog: { release_url: "https://github.com/gjy991229/D2rHub/releases", assets: [
    { id: "processor", version: "1.4.0-beta.17", size: 3000000, url: "https://example.test/processor", game_data_version: null },
    { id: "LiteHub", version: "r1", size: 24000000, url: "https://example.test/mod", game_data_version: "93854" },
  ] },
  processor: { ready: false, installed_version: "1.3.3", recommended_version: "1.4.0-beta.17", installed_path: "C:\\old\\d2r-audio-mod.exe", install_directory: "C:\\User\\tools", legacy: true },
  mods_directory: "D:\\Game\\mods", game_data_version: "93854", warning: null,
});
beforeEach(() => { subscribe.mockReset().mockResolvedValue(() => {}); invoke.mockReset(); picker.mockReset(); invoke.mockImplementation(async (cmd: string) => cmd === "get_mod_resources" ? state() : { path: "C:\\User\\tools\\processor.exe" }); });
afterEach(cleanup);
describe("Mod resources", () => {
  it("reads locally on every visit and only refreshes online when requested", async () => {
    const first = render(<ModResourceLibrary edition="CN" en={false} />);
    await screen.findByText("LiteHub"); first.unmount();
    render(<ModResourceLibrary edition="CN" en={false} />);
    await screen.findByText("LiteHub");
    expect(invoke.mock.calls.filter(([cmd]) => cmd === "get_mod_resources").every(([, args]) => args.refresh === false)).toBe(true);
    await userEvent.click(screen.getByRole("button", { name: "检查更新" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_mod_resources", { edition: "CN", refresh: true }));
  });
  it("keeps a current processor quiet in the processing view", async () => {
    invoke.mockResolvedValue({ ...state(), processor: { ...state().processor, ready: true, update_available: false } });
    const ready = vi.fn();
    const view = render(<ModProcessorStatus edition="CN" en={false} onReady={ready} onManage={vi.fn()} />);
    await waitFor(() => expect(ready).toHaveBeenLastCalledWith(true));
    expect(view.container.textContent).toBe("");
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("get_mod_resources", { edition: "CN", refresh: false });
  });
  it("shows immediate feedback inside the clicked resource card before a task arrives", async () => {
    invoke.mockImplementation((cmd: string) => cmd === "get_mod_resources" ? Promise.resolve(state()) : new Promise(() => {}));
    render(<ModResourceLibrary edition="CN" en={false} />);
    const card = (await screen.findByText("LiteHub")).closest("article")!;
    await userEvent.click(within(card).getByRole("button", { name: "下载并安装" }));
    expect(within(card).getByText("正在准备…")).toBeTruthy();
    expect(within(card).getByRole("progressbar", { name: "LiteHub 进度" }).hasAttribute("aria-valuenow")).toBe(false);
    expect(within(card).getByRole("button", { name: "正在处理…" }).hasAttribute("disabled")).toBe(true);
    expect(within(screen.getByRole("article", { name: "Mod 加工器" })).queryByRole("progressbar")).toBeNull();
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
  it("shows a legacy processor version and installs the update in the managed location", async () => {
    const user = userEvent.setup(); render(<ModResourceLibrary edition="Global" en={false} />);
    await screen.findByText(/检测到旧版内置加工器/);
    expect(screen.getByText("C:\\User\\tools")).toBeTruthy();
    expect(screen.getByText("D:\\Game\\mods\\LiteHub")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "更新加工器" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("install_mod_resource", { edition: "Global", resourceId: "processor", localFile: null }));
  });
  it("blocks incompatible game packages without blocking processor installation", async () => {
    invoke.mockResolvedValue({ ...state(), game_data_version: "99999" });
    render(<ModResourceLibrary edition="CN" en={false} />);
    await screen.findByText("LiteHub");
    const card = screen.getByText("LiteHub").closest("article")!;
    expect((within(card).getByRole("button", { name: "下载并安装" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "更新加工器" }) as HTMLButtonElement).disabled).toBe(false);
  });
  it("imports a selected EXE through the same verified installer", async () => {
    picker.mockResolvedValue("D:\\Downloads\\tool.exe");
    const user = userEvent.setup(); render(<ModResourceLibrary edition="CN" en={false} processorOnly />);
    await user.click(await screen.findByRole("button", { name: "导入已下载文件" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("install_mod_resource", { edition: "CN", resourceId: "processor", localFile: "D:\\Downloads\\tool.exe" }));
  });
});
