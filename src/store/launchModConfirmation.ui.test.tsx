import { beforeEach, expect, it, vi } from "vitest";
import { waitFor } from "@testing-library/react";
import { useLaunch } from "./launch";
import { useLaunchModConfirmation } from "./launchModConfirmation";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(), emit: vi.fn(), toast: vi.fn(),
  unminimize: vi.fn(), show: vi.fn(), setFocus: vi.fn(),
}));
vi.mock("../platform/tauri", () => ({ invokeCommand: mocks.invoke, emitEvent: mocks.emit }));
vi.mock("../components/ui/Toast", () => ({ showToast: mocks.toast }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => mocks }));

beforeEach(() => {
  vi.resetAllMocks();
  useLaunch.setState({ launching: false, error: null, results: [], progress: {} });
  useLaunchModConfirmation.setState({ warning: null, respond: () => {} });
  mocks.unminimize.mockResolvedValue(undefined);
  mocks.show.mockResolvedValue(undefined);
  mocks.setFocus.mockResolvedValue(undefined);
  mocks.invoke.mockRejectedValueOnce("[MOD_PREFLIGHT]Missing Mod feature").mockResolvedValue([]);
});

it.each([false, true])("reveals the main window and waits for an explicit decision: %s", async accepted => {
  let reveal!: () => void;
  mocks.show.mockImplementation(() => new Promise<void>(resolve => { reveal = resolve; }));
  const launch = useLaunch.getState().startSavedScheme("farm", 7);
  await waitFor(() => expect(mocks.show).toHaveBeenCalledOnce());
  expect(mocks.unminimize).toHaveBeenCalledOnce();
  expect(mocks.setFocus).not.toHaveBeenCalled();
  expect(useLaunchModConfirmation.getState().warning).toBeNull();
  reveal();
  await waitFor(() => expect(useLaunchModConfirmation.getState().warning).toBe("Missing Mod feature"));
  expect(mocks.setFocus).toHaveBeenCalledOnce();
  expect(mocks.invoke).toHaveBeenCalledTimes(1);
  expect(useLaunch.getState().launching).toBe(true);
  useLaunchModConfirmation.getState().respond(accepted);
  await launch;
  expect(mocks.invoke).toHaveBeenCalledTimes(accepted ? 2 : 1);
  if (accepted) expect(mocks.invoke).toHaveBeenLastCalledWith("launch_saved_scheme", {
    id: "farm", revision: 7, acceptedModWarning: "Missing Mod feature",
  });
  expect(useLaunch.getState().launching).toBe(false);
  expect(useLaunchModConfirmation.getState().warning).toBeNull();
});

it.each(["unminimize", "show", "setFocus"] as const)("releases the launch guard if %s fails", async operation => {
  mocks[operation].mockRejectedValue(new Error("Window unavailable"));
  await useLaunch.getState().startSavedScheme("farm", 7);
  expect(useLaunch.getState().launching).toBe(false);
  expect(useLaunch.getState().error).toContain("Window unavailable");
  expect(useLaunchModConfirmation.getState().warning).toBeNull();
  expect(mocks.invoke).toHaveBeenCalledTimes(1);
});

it.each(["cancel", "reset"] as const)("%s closes a pending decision and ignores a late acceptance", async action => {
  const launch = useLaunch.getState().startSavedScheme("farm", 7);
  await waitFor(() => expect(useLaunchModConfirmation.getState().warning).not.toBeNull());
  const lateResponse = useLaunchModConfirmation.getState().respond;
  if (action === "cancel") await useLaunch.getState().cancelLaunch();
  else useLaunch.getState().reset();
  await launch;
  lateResponse(true);
  expect(useLaunchModConfirmation.getState().warning).toBeNull();
  expect(useLaunch.getState().launching).toBe(false);
  expect(mocks.invoke.mock.calls.filter(([command]) => command === "launch_saved_scheme")).toHaveLength(1);
});

it("cancels immediately while the window is being revealed and never opens a late prompt", async () => {
  let reveal!: () => void;
  mocks.show.mockImplementation(() => new Promise<void>(resolve => { reveal = resolve; }));
  const launch = useLaunch.getState().startSavedScheme("farm", 7);
  await waitFor(() => expect(mocks.show).toHaveBeenCalledOnce());
  await useLaunch.getState().cancelLaunch();
  await launch;
  expect(useLaunch.getState().launching).toBe(false);
  reveal();
  await Promise.resolve();
  expect(mocks.setFocus).not.toHaveBeenCalled();
  expect(useLaunchModConfirmation.getState().warning).toBeNull();
  expect(mocks.invoke.mock.calls.filter(([command]) => command === "launch_saved_scheme")).toHaveLength(1);
});

it("does not open a confirmation if the preflight warning arrives after cancellation", async () => {
  let rejectPreflight!: (error: string) => void;
  mocks.invoke.mockReset().mockImplementation(command => command === "launch_saved_scheme"
    ? new Promise((_resolve, reject) => { rejectPreflight = reject; }) : Promise.resolve());
  const launch = useLaunch.getState().startSavedScheme("farm", 7);
  await useLaunch.getState().cancelLaunch();
  rejectPreflight("[MOD_PREFLIGHT]Late warning");
  await launch;
  expect(useLaunch.getState().launching).toBe(false);
  expect(useLaunchModConfirmation.getState().warning).toBeNull();
  expect(mocks.show).not.toHaveBeenCalled();
});

it("ignores acceptance followed by cancellation in the same turn", async () => {
  const launch = useLaunch.getState().startSavedScheme("farm", 7);
  await waitFor(() => expect(useLaunchModConfirmation.getState().warning).not.toBeNull());
  useLaunchModConfirmation.getState().respond(true);
  await useLaunch.getState().cancelLaunch();
  await launch;
  expect(mocks.invoke.mock.calls.filter(([command]) => command === "launch_saved_scheme")).toHaveLength(1);
  expect(useLaunch.getState().launching).toBe(false);
});
