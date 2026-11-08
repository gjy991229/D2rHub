import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listenEvent } from "../platform/tauri";
import { useGlobalConfig } from "../store/globalConfig";
import { useLaunch } from "../store/launch";
import type { GlobalConfig, LaunchGroup } from "../store/types";
import { useLaunchSchemeShortcut } from "./useLaunchSchemeShortcut";
import { showToast } from "../components/ui/Toast";

vi.mock("../platform/tauri", () => ({ invokeCommand: vi.fn(), emitEvent: vi.fn(), listenEvent: vi.fn() }));
vi.mock("../components/ui/Toast", () => ({ showToast: vi.fn() }));

let fire: (payload: string) => void;
const stop = vi.fn();
const group: LaunchGroup = { id: "saved", name: "Saved scheme", revision: 4, account_ids: ["one"] };

beforeEach(() => {
  useGlobalConfig.setState({ initialLoading: false, saving: false, restarting: false,
    config: { app_language: "en-US", launch_groups: [group] } as GlobalConfig });
  useLaunch.setState({ launching: false });
  vi.mocked(listenEvent).mockImplementation(async (_name, listener) => {
    fire = payload => listener({ event: "launch-scheme-shortcut", id: 0, payload });
    return stop;
  });
});
afterEach(cleanup);

describe("launch scheme global shortcuts", () => {
  it("obeys current startup/editing guards without re-registering the global listener", async () => {
    const launch = vi.fn(async () => {});
    const { rerender } = renderHook(({ enabled }) => useLaunchSchemeShortcut(enabled, launch), { initialProps: { enabled: false } });
    await act(async () => fire("saved"));
    expect(launch).not.toHaveBeenCalled();
    rerender({ enabled: true });
    await act(async () => fire("saved"));
    expect(launch).toHaveBeenCalledWith(group);
    expect(listenEvent).toHaveBeenCalledTimes(1);
  });

  it.each(["initialLoading", "saving", "restarting", "launching"] as const)("does not dispatch while %s", async flag => {
    const launch = vi.fn(async () => {});
    renderHook(() => useLaunchSchemeShortcut(true, launch));
    if (flag === "launching") useLaunch.setState({ launching: true });
    else useGlobalConfig.setState({ [flag]: true });
    await act(async () => fire("saved"));
    expect(launch).not.toHaveBeenCalled();
  });

  it("ignores repeat presses while preflight is pending and reads the latest saved revision", async () => {
    let finish!: () => void;
    const pending = new Promise<void>(resolve => { finish = resolve; });
    const launch = vi.fn(async () => pending);
    renderHook(() => useLaunchSchemeShortcut(true, launch));
    act(() => { fire("saved"); fire("saved"); });
    expect(launch).toHaveBeenCalledTimes(1);
    await act(async () => finish());
    useGlobalConfig.setState({ config: { launch_groups: [{ ...group, revision: 5 }] } as GlobalConfig });
    await act(async () => fire("saved"));
    expect(launch).toHaveBeenLastCalledWith(expect.objectContaining({ revision: 5 }));
  });

  it("ignores missing schemes, disposes its listener, and surfaces preflight failures", async () => {
    const launch = vi.fn(async () => { throw new Error("Pending settings could not be saved"); });
    const { unmount } = renderHook(() => useLaunchSchemeShortcut(true, launch));
    await act(async () => fire("deleted"));
    expect(launch).not.toHaveBeenCalled();
    await act(async () => fire("saved"));
    expect(showToast).toHaveBeenCalledWith("error", expect.stringContaining("Pending settings could not be saved"));
    unmount();
    await act(async () => fire("saved"));
    expect(launch).toHaveBeenCalledTimes(1);
    expect(stop).toHaveBeenCalled();
  });
});
