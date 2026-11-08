import type { ComponentProps } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within, type RenderResult } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invokeCommand } from "../../platform/tauri";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import type { AccountMeta, GlobalConfig } from "../../store/types";
import { AccountInitDialog } from "./AccountInitDialog";

const mocks = vi.hoisted(() => ({
  config: {} as GlobalConfig,
  createAccount: vi.fn(), deleteAccount: vi.fn(), loadAccounts: vi.fn(),
  initializeBnetAccount: vi.fn(), reinitializeAccount: vi.fn(),
}));
vi.mock("../../store/globalConfig", () => ({ useGlobalConfig: () => ({ config: mocks.config }) }));
vi.mock("../../store/accounts", () => ({ useAccounts: () => mocks }));
vi.mock("../../platform/tauri", () => ({ invokeCommand: vi.fn(), listenEvent: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn(async () => {}) }));
vi.mock("../ui/Toast", () => ({ showToast: vi.fn() }));

const validToken = "EU-1234567890abcdefghijklmnop-12345678";
const existing = { id: "existing", display_name: "Existing", auth_mode: "token", region: "EU",
  language: "enUS", voicelanguage: "zhTW", initialized: true } as AccountMeta;

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(finish => { resolve = finish; });
  return { promise, resolve };
}

function mount(overrides: Partial<ComponentProps<typeof AccountInitDialog>> = {}) {
  let view!: RenderResult;
  const onDone = vi.fn();
  const onClose = vi.fn(() => update({ open: false }));
  let props: ComponentProps<typeof AccountInitDialog> = { open: true, onDone, onClose, ...overrides };
  const update = (patch: Partial<ComponentProps<typeof AccountInitDialog>>) => {
    props = { ...props, ...patch };
    view.rerender(<AccountInitDialog {...props} />);
  };
  view = render(<AccountInitDialog {...props} />);
  return { ...view, onDone, onClose, update };
}

async function enterAuthentication(user: ReturnType<typeof userEvent.setup>, nickname = "Main") {
  await user.type(screen.getByRole("textbox", { name: "账号昵称" }), nickname);
  await user.click(screen.getByRole("button", { name: "选择认证方式" }));
}

function enterBnetWithEvents() {
  fireEvent.change(screen.getByRole("textbox", { name: "账号昵称" }), { target: { value: "Main" } });
  fireEvent.click(screen.getByRole("button", { name: "选择认证方式" }));
  fireEvent.click(screen.getByRole("radio", { name: /战网客户端认证/ }));
  fireEvent.click(screen.getByRole("button", { name: "开始战网登录" }));
}

beforeEach(() => {
  mocks.config = {
    app_language: "zh-CN", cn_game_path: "D:\\CN", cn_saved_games_path: "D:\\SavesCN",
    cn_battle_net_path: "D:\\Battle.net.exe", global_game_path: "D:\\Global", global_saved_games_path: "D:\\SavesGlobal",
    browser_path: "", browser_type: "", auto_close_browser: false,
  } as GlobalConfig;
  mocks.createAccount.mockReset().mockResolvedValue("pending");
  mocks.deleteAccount.mockReset().mockResolvedValue(undefined);
  mocks.loadAccounts.mockReset().mockResolvedValue(undefined);
  mocks.initializeBnetAccount.mockReset().mockResolvedValue(undefined);
  mocks.reinitializeAccount.mockReset().mockResolvedValue(undefined);
  vi.mocked(invokeCommand).mockReset().mockResolvedValue(undefined);
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

describe("account creation wizard", () => {
  it("keeps the nickname, region, and language choices when returning or refreshing config", async () => {
    const user = userEvent.setup();
    const { update } = mount();
    const progress = screen.getByRole("list", { name: "账号创建进度" });
    expect(within(progress).getAllByRole("listitem")).toHaveLength(3);
    expect(within(progress).getByText("账号信息").closest("li")?.getAttribute("aria-current")).toBe("step");
    await user.type(screen.getByRole("textbox", { name: "账号昵称" }), "Support");
    await user.click(screen.getByRole("radio", { name: /亚洲/ }));
    await user.click(within(screen.getByRole("group", { name: "界面语言" })).getByRole("radio", { name: "English" }));
    await user.click(within(screen.getByRole("group", { name: "配音语言" })).getByRole("radio", { name: "简体中文" }));
    await user.click(screen.getByRole("button", { name: "选择认证方式" }));
    expect(screen.getByRole("heading", { name: "选择适合你的登录方式" })).toBeTruthy();
    expect(within(progress).getByText("认证方式").closest("li")?.getAttribute("aria-current")).toBe("step");
    expect(screen.queryByRole("radio", { name: /战网客户端认证/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "上一步" }));
    mocks.config = { ...mocks.config, global_game_path: "D:\\MovedGlobal" };
    update({});
    expect((screen.getByRole("textbox", { name: "账号昵称" }) as HTMLInputElement).value).toBe("Support");
    expect((screen.getByRole("radio", { name: /亚洲/ }) as HTMLInputElement).checked).toBe(true);
    expect((within(screen.getByRole("group", { name: "界面语言" })).getByRole("radio", { name: "English" }) as HTMLInputElement).checked).toBe(true);
    expect((within(screen.getByRole("group", { name: "配音语言" })).getByRole("radio", { name: "简体中文" }) as HTMLInputElement).checked).toBe(true);
    await user.click(screen.getByRole("button", { name: "选择认证方式" }));
    await user.click(screen.getByRole("button", { name: "打开登录页面" }));
    await waitFor(() => expect(screen.getByText("登录页面已打开")).toBeTruthy());
    expect(within(progress).getByText("连接账号").closest("li")?.getAttribute("aria-current")).toBe("step");
    expect(mocks.createAccount).toHaveBeenCalledWith("Support", "token", "KR", undefined, "enUS", "zhCN", true);
    expect(openUrl).toHaveBeenCalledWith(expect.stringContaining("kr.battle.net"));
  });

  it("disables unavailable client regions and Battle.net authentication without its executable", async () => {
    mocks.config = { ...mocks.config, global_game_path: "", cn_battle_net_path: "" };
    const user = userEvent.setup();
    mount();
    for (const name of [/亚洲/, /美洲/, /欧洲/]) {
      expect((screen.getByRole("radio", { name }) as HTMLInputElement).disabled).toBe(true);
    }
    await enterAuthentication(user);
    expect((screen.getByRole("radio", { name: /战网客户端认证/ }) as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByRole("radio", { name: /网页 Token 认证/ }) as HTMLInputElement).checked).toBe(true);
    expect(mocks.createAccount).not.toHaveBeenCalled();
  });

  it("creates a pending Token account once and reclaims a late create response after cancel", async () => {
    const creation = deferred<string>();
    mocks.createAccount.mockReturnValue(creation.promise);
    const user = userEvent.setup();
    const view = mount();
    await enterAuthentication(user);
    const next = screen.getByRole("button", { name: "打开登录页面" });
    act(() => { fireEvent.click(next); fireEvent.click(next); });
    expect(mocks.createAccount).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "取消" }));
    await waitFor(() => expect(view.onClose).toHaveBeenCalledTimes(1));
    await act(async () => creation.resolve("late-pending"));
    expect(mocks.deleteAccount).toHaveBeenCalledExactlyOnceWith("late-pending");
    expect(openUrl).not.toHaveBeenCalled();
    view.update({ open: true });
    expect((screen.getByRole("textbox", { name: "账号昵称" }) as HTMLInputElement).value).toBe("");
  });

  it("deletes a pending account when cancelling at the connection step", async () => {
    const user = userEvent.setup();
    const view = mount();
    await enterAuthentication(user);
    await user.click(screen.getByRole("button", { name: "打开登录页面" }));
    await waitFor(() => expect(screen.getByText("登录页面已打开")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: "取消" }));
    await waitFor(() => expect(view.onClose).toHaveBeenCalledTimes(1));
    expect(mocks.deleteAccount).toHaveBeenCalledExactlyOnceWith("pending");
    expect(view.onDone).not.toHaveBeenCalled();
  });

  it("does not close configured browser processes when cancelled before opening a login page", async () => {
    mocks.config = { ...mocks.config, browser_path: "C:\\Browser.exe", browser_type: "edge", auto_close_browser: true };
    const user = userEvent.setup();
    const view = mount();
    await enterAuthentication(user);
    await user.click(screen.getByRole("button", { name: "取消" }));
    await waitFor(() => expect(view.onClose).toHaveBeenCalledTimes(1));
    expect(invokeCommand).not.toHaveBeenCalledWith("kill_browser_processes", expect.anything());
    expect(mocks.createAccount).not.toHaveBeenCalled();
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("updates an existing account in English without creating another, and clears old Token input on reopen", async () => {
    mocks.config = { ...mocks.config, app_language: "en-US" };
    const user = userEvent.setup();
    const view = mount({ updateAccount: existing });
    await waitFor(() => expect(screen.getByText("Login page opened")).toBeTruthy());
    expect(screen.getByRole("heading", { name: "Sign in and connect" })).toBeTruthy();
    expect(screen.queryByRole("textbox", { name: "Account nickname" })).toBeNull();
    await user.type(screen.getByLabelText("Login link or Token"), validToken);
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(view.onClose).toHaveBeenCalledTimes(1));
    view.update({ open: true });
    await waitFor(() => expect(screen.getByText("Login page opened")).toBeTruthy());
    const input = screen.getByLabelText("Login link or Token") as HTMLInputElement;
    expect(input.value).toBe("");
    expect((screen.getByRole("button", { name: "Save Token" }) as HTMLButtonElement).disabled).toBe(true);
    await user.click(input);
    await user.paste(`https://example.invalid/?ST=${validToken}&result=ok`);
    const save = deferred<void>();
    vi.mocked(invokeCommand).mockImplementation(async command => command === "update_account_meta" ? save.promise as never : undefined);
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Save Token" }));
      fireEvent.keyDown(input, { key: "Enter" });
    });
    expect(vi.mocked(invokeCommand).mock.calls.filter(([command]) => command === "update_account_meta")).toHaveLength(1);
    expect(invokeCommand).toHaveBeenCalledWith("update_account_meta", {
      accountId: "existing", authMode: "token", token: validToken, region: "EU", language: "enUS", voicelanguage: "zhTW",
    });
    await act(async () => save.resolve());
    expect(mocks.createAccount).not.toHaveBeenCalled();
    expect(mocks.deleteAccount).not.toHaveBeenCalled();
    expect(view.onDone).toHaveBeenCalledExactlyOnceWith("existing");
    expect(view.onClose).toHaveBeenCalledTimes(2);
  });

  it("ignores an old browser response after a different account session is opened", async () => {
    vi.useFakeTimers();
    mocks.config = { ...mocks.config, browser_path: "C:\\Browser.exe", browser_type: "edge" };
    const oldLogin = deferred<void>();
    const newLogin = deferred<void>();
    vi.mocked(invokeCommand).mockImplementation(async (command, args) => command === "open_token_login_url"
      ? ((args as { accountId: string }).accountId === "existing" ? oldLogin.promise : newLogin.promise) as never
      : undefined);
    const view = mount({ updateAccount: existing });
    expect(invokeCommand).toHaveBeenCalledWith("open_token_login_url", expect.objectContaining({ accountId: "existing" }));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "取消" })));
    view.update({ open: true, updateAccount: { ...existing, id: "another", display_name: "Another" } });
    expect(invokeCommand).toHaveBeenCalledWith("open_token_login_url", expect.objectContaining({ accountId: "another" }));
    await act(async () => { oldLogin.resolve(); await vi.advanceTimersByTimeAsync(1200); });
    expect(screen.getByText("正在打开登录页面…")).toBeTruthy();
    expect(invokeCommand).not.toHaveBeenCalledWith("bring_self_to_foreground");
    await act(async () => { newLogin.resolve(); await vi.advanceTimersByTimeAsync(1200); });
    expect(screen.getByText("登录页面已打开")).toBeTruthy();
    expect(vi.mocked(invokeCommand).mock.calls.filter(([command]) => command === "bring_self_to_foreground")).toHaveLength(1);
  });

  it("reclaims a cancelled Battle.net account when creation returns late", async () => {
    const creation = deferred<string>();
    mocks.createAccount.mockReturnValue(creation.promise);
    const view = mount();
    enterBnetWithEvents();
    expect(mocks.createAccount).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    await act(async () => creation.resolve("late-bnet"));
    await waitFor(() => expect(view.onClose).toHaveBeenCalledTimes(1));
    expect(mocks.deleteAccount).toHaveBeenCalledExactlyOnceWith("late-bnet");
    expect(mocks.initializeBnetAccount).not.toHaveBeenCalled();
    expect(view.onDone).not.toHaveBeenCalled();
  });

  it.each(["button", "external-close"] as const)("does not repeat Battle.net completion after %s", async action => {
    vi.useFakeTimers();
    const view = mount();
    enterBnetWithEvents();
    await act(async () => {});
    const finish = screen.getByRole("button", { name: "完成" });
    if (action === "button") act(() => { fireEvent.click(finish); fireEvent.click(finish); });
    else view.update({ open: false });
    await act(async () => vi.advanceTimersByTimeAsync(2000));
    expect(view.onDone).toHaveBeenCalledTimes(action === "button" ? 1 : 0);
    expect(view.onClose).toHaveBeenCalledTimes(action === "button" ? 1 : 0);
  });

  it("does not let a late Battle.net initialization complete a newly opened session", async () => {
    vi.useFakeTimers();
    const initialization = deferred<void>();
    mocks.initializeBnetAccount.mockReturnValue(initialization.promise);
    const view = mount();
    enterBnetWithEvents();
    await act(async () => {});
    expect(mocks.initializeBnetAccount).toHaveBeenCalledExactlyOnceWith("pending");
    view.update({ open: false });
    view.update({ open: true });
    fireEvent.change(screen.getByRole("textbox", { name: "账号昵称" }), { target: { value: "New session" } });
    await act(async () => { initialization.resolve(); await vi.advanceTimersByTimeAsync(2000); });
    expect(screen.getByRole("heading", { name: "先认识一下这个账号" })).toBeTruthy();
    expect((screen.getByRole("textbox", { name: "账号昵称" }) as HTMLInputElement).value).toBe("New session");
    expect(view.onDone).not.toHaveBeenCalled();
    expect(view.onClose).not.toHaveBeenCalled();
  });
});
