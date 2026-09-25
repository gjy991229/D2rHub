import { beforeEach, describe, expect, it, vi } from "vitest";
import { useLaunch } from "./launch";
import type { LaunchResult } from "./types";

const mocks = vi.hoisted(() => ({
  invokeCommand: vi.fn(),
  emitEvent: vi.fn(),
  showToast: vi.fn(),
}));
vi.mock("../platform/tauri", () => ({
  invokeCommand: mocks.invokeCommand,
  emitEvent: mocks.emitEvent,
}));
vi.mock("../components/ui/Toast", () => ({ showToast: mocks.showToast }));

const confirmed: LaunchResult = {
  account_id: "one", success: true, login_unconfirmed: false,
  d2r_pid: 123, error: null, mutex_killed: true,
};
const unconfirmed: LaunchResult = {
  ...confirmed, success: false, login_unconfirmed: true, error: "登录状态未确认",
};
const failed: LaunchResult = {
  ...confirmed, account_id: "two", success: false, d2r_pid: null,
  mutex_killed: false, error: "游戏进程已退出",
};

beforeEach(() => {
  vi.clearAllMocks();
  useLaunch.setState({ launching: false, results: [], error: null, progress: {} });
});

for (const mode of ["accounts", "scheme"] as const) {
  describe(`${mode} launch outcome`, () => {
    const start = () => mode === "accounts"
      ? useLaunch.getState().startLaunch(["one", "two"])
      : useLaunch.getState().startSchemeLaunch([]);

    it("keeps an unconfirmed login distinct from success and failure", async () => {
      mocks.invokeCommand.mockResolvedValue([unconfirmed]);
      await start();
      expect(useLaunch.getState().results).toEqual([unconfirmed]);
      expect(useLaunch.getState().launching).toBe(false);
      expect(mocks.showToast).toHaveBeenCalledWith("warning", "登录状态未确认");
      expect(mocks.emitEvent).toHaveBeenCalledWith("launch-ended", {
        success: false, login_unconfirmed: true,
      });
    });

    it("does not hide a real failure when another login is unconfirmed", async () => {
      mocks.invokeCommand.mockResolvedValue([unconfirmed, failed]);
      await start();
      expect(mocks.emitEvent).toHaveBeenCalledWith("launch-ended", {
        success: false, login_unconfirmed: false,
      });
    });

    it("still reports confirmed launches as successful", async () => {
      mocks.invokeCommand.mockResolvedValue([confirmed]);
      await start();
      expect(mocks.emitEvent).toHaveBeenCalledWith("launch-ended", {
        success: true, login_unconfirmed: false,
      });
      expect(mocks.showToast).not.toHaveBeenCalled();
    });
  });
}
