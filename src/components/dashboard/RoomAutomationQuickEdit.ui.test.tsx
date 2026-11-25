import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { RoomAutomationQuickEdit } from "./RoomAutomationQuickEdit";
const mocks = vi.hoisted(() => ({ save: vi.fn(), sync: null as null | ((s: unknown) => void), config: { enabled: true, name_prefix: "run-", password: "", next_sequence: 1, sequence_width: 3, primary_account_id: "keep-primary" } }));
vi.mock("../../features/roomAutomation/gateway", () => ({ roomAutomationGateway: {
  startSync: async (handlers: {onConfig: (s: unknown) => void}) => { mocks.sync = handlers.onConfig; handlers.onConfig({ generation: 4, config: mocks.config }); return () => {}; },
  getConfig: async () => ({ generation: 5, config: mocks.config }), saveConfig: mocks.save,
} }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ readText: vi.fn(), writeText: vi.fn(async () => {}) }));
vi.mock("../ui/Toast", () => ({ showToast: vi.fn() }));
afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  mocks.config = { enabled: true, name_prefix: "run-", password: "", next_sequence: 1, sequence_width: 3, primary_account_id: "keep-primary" };
  mocks.save.mockImplementation(async (_generation, config) => ({ snapshot: { generation: 6, config } }));
});

it("opens participant and rule editing on every panel visit", async () => {
  const user = userEvent.setup();
  render(<RoomAutomationQuickEdit active onOpenSettings={() => {}} />);
  const trigger = await screen.findByRole("button", { name: "跟房配置" });
  await user.click(trigger);
  const editing = screen.getByRole("button", { name: "收起详情", expanded: true });
  expect(screen.getByRole("combobox", { name: "主号" })).toBeTruthy();
  expect(screen.getByLabelText("房名开头")).toBeTruthy();

  await user.click(editing);
  expect(screen.getByRole("button", { name: "编辑跟房设置", expanded: false })).toBeTruthy();
  expect(screen.queryByRole("combobox", { name: "主号" })).toBeNull();
  await user.click(trigger);
  await user.click(trigger);

  expect(screen.getByRole("button", { name: "收起详情", expanded: true })).toBeTruthy();
  expect(screen.getByRole("combobox", { name: "主号" })).toBeTruthy();
});

it("saves in place, syncs Uber Diablo visibility, and autosaves the next edit on exit", async () => {
  const user = userEvent.setup();
  render(<RoomAutomationQuickEdit active onOpenSettings={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "跟房配置" }));
  expect(screen.queryByRole("radio", { name: /超级迪亚波罗/ })).toBeNull();
  act(() => mocks.sync?.({ generation: 5, config: { ...mocks.config, show_uber_diablo_in_quick_settings: true } }));
  await user.click(screen.getByRole("radio", { name: /超级迪亚波罗/ }));
  await user.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() => expect((screen.getByRole("button", { name: "保存" }) as HTMLButtonElement).disabled).toBe(true));
  expect(screen.getByRole("dialog")).toBeTruthy();
  expect(mocks.save).toHaveBeenCalledWith(5, expect.objectContaining({ collaboration_mode: "uber_diablo" }));
  await user.type(screen.getByLabelText("房间密码"), "new");
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mocks.save).toHaveBeenCalledTimes(2);
  expect(mocks.save).toHaveBeenLastCalledWith(5, expect.objectContaining({ password: "new" }));
});

it("saves with Escape and returns focus to its trigger", async () => {
  const user = userEvent.setup();
  render(<RoomAutomationQuickEdit active onOpenSettings={() => {}} />);
  const trigger = await screen.findByRole("button", { name: "跟房配置" });
  await user.click(trigger);
  await user.clear(screen.getByLabelText("房名开头"));
  await user.type(screen.getByLabelText("房名开头"), "changed-");
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mocks.save).toHaveBeenCalledWith(5, expect.objectContaining({ name_prefix: "changed-" }));
  expect(document.activeElement).toBe(trigger);
  await user.click(trigger);
  expect((screen.getByLabelText("房名开头") as HTMLInputElement).value).toBe("changed-");
});
it("preserves the draft on save failure and saves against the latest generation on retry", async () => {
  const user = userEvent.setup();
  mocks.save.mockRejectedValueOnce(new Error("disk error")).mockResolvedValueOnce({ snapshot: { generation: 6, config: mocks.config } });
  render(<RoomAutomationQuickEdit active onOpenSettings={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "跟房配置" }));
  await user.clear(screen.getByLabelText("房名开头"));
  await user.type(screen.getByLabelText("房名开头"), "new-");
  await user.click(screen.getByRole("button", { name: "关闭" }));
  expect(await screen.findByRole("alert")).toBeTruthy();
  expect((screen.getByLabelText("房名开头") as HTMLInputElement).value).toBe("new-");
  await user.click(screen.getByRole("button", { name: "关闭" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mocks.save).toHaveBeenLastCalledWith(5, expect.objectContaining({ name_prefix: "new-", primary_account_id: "keep-primary" }));
});


it("copies and pastes only the room rule without changing participants", async () => {
  const user = userEvent.setup();
  mocks.save.mockImplementation(async (_generation, config) => ({ snapshot: { generation: 6, config } }));
  vi.mocked(readText).mockResolvedValue(JSON.stringify({ format: "d2rhub-room-rule", version: 1, name_prefix: "friend-", password: "secret", next_sequence: 12, sequence_width: 2 }));
  render(<RoomAutomationQuickEdit active onOpenSettings={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "跟房配置" }));
  await user.click(screen.getByRole("button", { name: "粘贴" }));
  expect((screen.getByLabelText("房名开头") as HTMLInputElement).value).toBe("friend-");
  expect(mocks.save).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "复制" }));
  expect(JSON.parse(vi.mocked(writeText).mock.calls[0][0])).toEqual({ format: "d2rhub-room-rule", version: 1, name_prefix: "friend-", password: "secret", next_sequence: 12, sequence_width: 2 });
  expect(screen.queryByLabelText("仅跟房（不创建）")).toBeNull();
  await user.click(screen.getByRole("button", { name: "关闭" }));
  expect(mocks.save).toHaveBeenCalledWith(5, expect.objectContaining({ primary_account_id: "keep-primary", name_prefix: "friend-", next_sequence: 12 }), true);
});
it("rejects malformed clipboard rules without changing the current draft", async () => {
  const user = userEvent.setup();
  vi.mocked(readText).mockResolvedValue("bad-json");
  render(<RoomAutomationQuickEdit active onOpenSettings={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "跟房配置" }));
  await user.click(screen.getByRole("button", { name: "粘贴" }));
  expect(await screen.findByRole("alert")).toBeTruthy();
  expect((screen.getByLabelText("房名开头") as HTMLInputElement).value).toBe("run-");
  expect(mocks.save).not.toHaveBeenCalled();
});

it("resets identical pasted rules only when leaving, retaining the request on failure", async () => {
  const user = userEvent.setup();
  vi.mocked(readText).mockResolvedValue(JSON.stringify({ format: "d2rhub-room-rule", version: 1, ...mocks.config }));
  mocks.save.mockRejectedValueOnce(new Error("disk error")).mockImplementation(async (_generation, config) => ({ snapshot: { generation: 6, config } }));
  render(<RoomAutomationQuickEdit active onOpenSettings={() => {}} />);
  const trigger = await screen.findByRole("button", { name: "跟房配置" });
  await user.click(trigger);
  await user.click(screen.getByRole("button", { name: "粘贴" }));
  expect(mocks.save).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "关闭" }));
  expect(await screen.findByRole("alert")).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "关闭" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mocks.save).toHaveBeenLastCalledWith(5, expect.objectContaining({ name_prefix: "run-" }), true);
  await user.click(trigger);
  await user.click(screen.getByRole("button", { name: "粘贴" }));
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  const count = mocks.save.mock.calls.length;
  await user.click(trigger);
  await user.click(screen.getByRole("button", { name: "关闭" }));
  expect(mocks.save).toHaveBeenCalledTimes(count);
});

it("applies one collaboration mode with the same room rule and participants", async () => {
  const user = userEvent.setup();
  mocks.save.mockImplementation(async (_generation, config) => ({ snapshot: { generation: 6, config } }));
  render(<RoomAutomationQuickEdit active onOpenSettings={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "跟房配置" }));
  await user.click(screen.getByRole("radio", { name: /拼房跟随/ }));
  expect((screen.getByRole("radio", { name: /拼房跟随/ }) as HTMLInputElement).checked).toBe(true);
  expect((screen.getByRole("radio", { name: /拼房主机/ }) as HTMLInputElement).checked).toBe(false);
  expect(mocks.save).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "关闭" }));
  await waitFor(() => expect(mocks.save).toHaveBeenCalledWith(5, expect.objectContaining({ collaboration_mode: "follower", primary_account_id: "keep-primary", name_prefix: "run-" })));
});

it("closes untouched rules without a save and exposes no Apply or Cancel", async () => {
  const user = userEvent.setup();
  render(<RoomAutomationQuickEdit active onOpenSettings={() => {}} />);
  const trigger = await screen.findByRole("button", { name: "跟房配置" });
  await user.click(trigger);
  expect(screen.queryByRole("button", { name: /应用|取消/ })).toBeNull();
  await user.click(trigger);
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mocks.save).not.toHaveBeenCalled();
});

it("keeps invalid edits open when leaving and prevents navigation", async () => {
  const user = userEvent.setup(); const navigate = vi.fn();
  render(<RoomAutomationQuickEdit active onOpenSettings={navigate} />);
  await user.click(await screen.findByRole("button", { name: "跟房配置" }));
  await user.clear(screen.getByLabelText("房名开头"));
  await user.click(screen.getByRole("button", { name: "完整设置" }));
  expect(await screen.findByRole("alert")).toBeTruthy();
  expect(screen.getByRole("dialog")).toBeTruthy();
  expect(navigate).not.toHaveBeenCalled(); expect(mocks.save).not.toHaveBeenCalled();
});

it("saves outside clicks and preserves a sequence advanced by the backend", async () => {
  const user = userEvent.setup();
  render(<><RoomAutomationQuickEdit active onOpenSettings={() => {}} /><button>Outside</button></>);
  await user.click(await screen.findByRole("button", { name: "跟房配置" }));
  await user.type(screen.getByLabelText("房间密码"), "new");
  mocks.config = { ...mocks.config, next_sequence: 8 };
  await user.click(screen.getByRole("button", { name: "Outside" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mocks.save).toHaveBeenCalledOnce();
  expect(mocks.save).toHaveBeenCalledWith(5, expect.objectContaining({ password: "new", next_sequence: 8 }));
});

it("finishes saving before opening the full settings page", async () => {
  const user = userEvent.setup(); const navigate = vi.fn();
  let finish!: (value: unknown) => void;
  mocks.save.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  render(<RoomAutomationQuickEdit active onOpenSettings={navigate} />);
  await user.click(await screen.findByRole("button", { name: "跟房配置" }));
  await user.type(screen.getByLabelText("房间密码"), "new");
  await user.click(screen.getByRole("button", { name: "完整设置" }));
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
  expect(navigate).not.toHaveBeenCalled(); expect(mocks.save).toHaveBeenCalledOnce();
  await act(async () => finish({ snapshot: { generation: 6, config: { ...mocks.config, password: "new" } } }));
  expect(navigate).toHaveBeenCalledOnce();
});

it("flushes valid changes when its page unmounts", async () => {
  const user = userEvent.setup();
  const view = render(<RoomAutomationQuickEdit active onOpenSettings={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "跟房配置" }));
  await user.type(screen.getByLabelText("房间密码"), "new");
  view.unmount();
  await waitFor(() => expect(mocks.save).toHaveBeenCalledWith(5, expect.objectContaining({ password: "new" })));
});

it("waits for a pending paste before flushing on page exit", async () => {
  const user = userEvent.setup(); let finish!: (value: string) => void;
  vi.mocked(readText).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  const view = render(<RoomAutomationQuickEdit active onOpenSettings={() => {}} />);
  await user.click(await screen.findByRole("button", { name: "跟房配置" }));
  await user.click(screen.getByRole("button", { name: "粘贴" }));
  view.unmount();
  await act(async () => finish(JSON.stringify({ format: "d2rhub-room-rule", version: 1, ...mocks.config, name_prefix: "friend-" })));
  await waitFor(() => expect(mocks.save).toHaveBeenCalledWith(5, expect.objectContaining({ name_prefix: "friend-" }), true));
});
