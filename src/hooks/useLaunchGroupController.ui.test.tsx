import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useLaunchGroupController } from "./useLaunchGroupController";
import { useAccounts } from "../store/accounts";
import { useGlobalConfig } from "../store/globalConfig";
import { useLaunch } from "../store/launch";
import type { AccountMeta, GlobalConfig, LaunchGroup, SchemeEnvironment } from "../store/types";
import { invokeCommand } from "../platform/tauri";
import { LaunchGroupEditor } from "../components/dashboard/LaunchGroupEditor";

vi.mock("../platform/tauri", () => ({ invokeCommand: vi.fn(), listenEvent: vi.fn(), emitEvent: vi.fn() }));
vi.mock("../components/ui/Toast", () => ({ showToast: vi.fn() }));

const environment: SchemeEnvironment = {
  schema_version: 2,
  settings: { rune_audio_target_account: "", keep_battle_net_account_id: null },
  room: { primary_account_id: "", follower_account_ids: [] },
  layout: null,
};
const accounts = ["a", "b"].map((id, order) => ({ id, display_name: id, initialized: true, order, region: "KR", auth_mode: "token", mod_args: "", position_presets: [] })) as unknown as AccountMeta[];
const scheme: LaunchGroup = { id: "one", name: "三开", revision: 4, account_ids: ["b", "a"],
  environment, members: accounts.slice().reverse().map(account => ({ account_id: account.id, mod_args: "", graphics_configured: true, resolution: "1280x720", fps: 30,
    snapshot: { game_settings: { Quality: 2 }, position: null, region: "KR", language: null, voicelanguage: null } })) };
const mutatingCalls = () => vi.mocked(invokeCommand).mock.calls.filter(([command]) => !command.startsWith("get_"));

beforeEach(() => {
  vi.clearAllMocks();
  useAccounts.setState({ accounts: structuredClone(accounts) });
  useGlobalConfig.setState({ saving: false, config: { launch_groups: [structuredClone(scheme)] } as GlobalConfig, load: vi.fn(async () => {}) });
  useLaunch.setState({ startSavedScheme: vi.fn(async () => {}) });
  vi.mocked(invokeCommand).mockImplementation(async command => {
    if (command === "get_launch_scheme_defaults") return structuredClone(environment) as never;
    if (command === "get_account_settings") return { "Screen Resolution (Windowed)": "1920x1080", "Framerate Cap": 60, Quality: 3 } as never;
    return undefined as never;
  });
});
afterEach(cleanup);

describe("independent launch scheme drafts", () => {
  it("identifies saved overrides and keeps account fields available beyond layout slots", async () => {
    const { result } = renderHook(() => useLaunchGroupController());
    await act(async () => result.current.edit(scheme));
    act(() => result.current.updateEnvironment(env => { env.layout = {
      id: "layout", name: "One slot", monitors: [],
      windows: [{ monitor_id: "display", x: 0, y: 0, width: 1600, height: 900 }],
    }; }));
    const view = render(<LaunchGroupEditor launchGroups={result.current} />);
    expect(screen.getByText(/方案布局 · 位置 1/)).toBeTruthy();
    expect(screen.queryByRole("textbox", { name: "b 分辨率" })).toBeNull();
    expect((screen.getByRole("textbox", { name: "a 分辨率" }) as HTMLInputElement).disabled).toBe(false);
    act(() => result.current.updateMember("a", { snapshot: { ...result.current.draft!.members[1].snapshot!, game_settings: {} } }));
    view.rerender(<LaunchGroupEditor launchGroups={result.current} />);
    expect(mutatingCalls()).toEqual([]);
  });

  it("cancelling nested edits never writes the scheme, account or current settings", async () => {
    const before = structuredClone(useGlobalConfig.getState().config);
    const { result } = renderHook(() => useLaunchGroupController());
    await act(async () => result.current.edit(scheme));
    act(() => result.current.updateEnvironment(env => { env.room.primary_account_id = "a"; env.settings.rune_audio_target_account = "b"; }));
    act(() => result.current.updateMember("a", { fps: 144 }));
    expect(environment.room.primary_account_id).toBe("");
    expect(useGlobalConfig.getState().config).toEqual(before);
    act(() => result.current.cancelDraft());
    expect(result.current.draft).toBeNull();
    expect(mutatingCalls()).toEqual([]);
  });

  it("saves only through the scheme endpoint with its original revision and own order", async () => {
    const { result } = renderHook(() => useLaunchGroupController());
    await act(async () => result.current.edit(scheme));
    act(() => result.current.updateMember("b", { fps: 144 }));
    await act(async () => result.current.saveDraft());
    expect(invokeCommand).toHaveBeenCalledWith("save_launch_scheme", expect.objectContaining({ expectedRevision: 4,
      scheme: expect.objectContaining({ account_ids: ["b", "a"], members: [expect.objectContaining({ account_id: "b", fps: 144 }), expect.objectContaining({ account_id: "a", fps: 30 })] }) }));
    expect(useGlobalConfig.getState().config?.launch_groups[0]).toEqual(scheme);
  });

  it("inherits account game settings when adding an account and keeps a duplicate independent", async () => {
    const { result } = renderHook(() => useLaunchGroupController());
    await act(async () => result.current.create());
    act(() => result.current.toggleAccount("a"));
    await waitFor(() => expect(result.current.draft?.members[0].snapshot).toBeTruthy());
    expect(result.current.draft?.members[0]).toMatchObject({ resolution: "1920x1080", fps: 60, snapshot: { game_settings: {} } });
    await act(async () => result.current.edit(scheme));
    act(() => result.current.duplicate());
    expect(result.current.draft?.id).toBeNull();
    expect(result.current.draft?.revision).toBeNull();
    act(() => result.current.updateEnvironment(env => { env.room.primary_account_id = "a"; }));
    expect(scheme.environment?.room.primary_account_id).toBe("");
  });

  it("launches by saved identity and never projects settings from the frontend", () => {
    const { result } = renderHook(() => useLaunchGroupController());
    act(() => result.current.launch(scheme));
    expect(useLaunch.getState().startSavedScheme).toHaveBeenCalledWith("one", 4);
    expect(mutatingCalls()).toEqual([]);
  });

  it("requires explicit editing for an old scheme without assigning global module settings", async () => {
    const { result } = renderHook(() => useLaunchGroupController());
    await act(async () => result.current.launch({ ...scheme, environment: null }));
    expect(useLaunch.getState().startSavedScheme).not.toHaveBeenCalled();
    expect(result.current.draft?.environment.room.follower_account_ids).toEqual([]);
    expect(mutatingCalls()).toEqual([]);
  });

  it("clears role references when removing an account without touching public settings", async () => {
    const { result } = renderHook(() => useLaunchGroupController());
    await act(async () => result.current.edit(scheme));
    act(() => result.current.updateEnvironment(env => {
      env.room.primary_account_id = "a";
      env.settings.rune_audio_target_account = "a";
      env.settings.keep_battle_net_account_id = "a";
    }));
    act(() => result.current.toggleAccount("a"));
    expect(result.current.draft?.environment).toMatchObject({ room: { primary_account_id: "", follower_account_ids: [] },
      settings: { rune_audio_target_account: "", keep_battle_net_account_id: null } });
    expect(mutatingCalls()).toEqual([]);
  });

  it("shows account-only room and recognition controls and links to shared settings", async () => {
    const { result } = renderHook(() => useLaunchGroupController());
    await act(async () => result.current.edit(scheme));
    const openSettings = vi.fn();
    const view = render(<LaunchGroupEditor launchGroups={result.current} onOpenSettings={openSettings} />);
    fireEvent.click(screen.getByRole("button", { name: "跟房" }));
    expect(screen.getByLabelText("主号")).toBeTruthy();
    expect(screen.queryByLabelText("启用自动跟房")).toBeNull();
    expect(screen.queryByLabelText("房名前缀")).toBeNull();
    expect(screen.queryByText("快捷键与输入时序")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "前往公共跟房设置" }));
    expect(openSettings).toHaveBeenCalledWith("room-automation");
    fireEvent.click(screen.getByRole("button", { name: "识别" }));
    expect(screen.getByLabelText("识别账号")).toBeTruthy();
    expect(screen.queryByLabelText("启用识别")).toBeNull();
    expect(screen.queryByText("记录内容")).toBeNull();
    expect(screen.queryByLabelText("统计悬浮窗")).toBeNull();
    view.rerender(<LaunchGroupEditor launchGroups={result.current} onOpenSettings={openSettings} hidden />);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(result.current.draft).toBeTruthy();
  });
});
