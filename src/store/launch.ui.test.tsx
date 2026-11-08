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
  account_id: "one", success: true,
  d2r_pid: 123, error: null, mutex_killed: true,
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

    it("reports a failed login as failure even with a surviving game process", async () => {
      const timeout = { ...confirmed, success: false, error: "登录等待超时" };
      mocks.invokeCommand.mockResolvedValue([confirmed, timeout]);
      await start();
      expect(useLaunch.getState().launching).toBe(false);
      expect(mocks.emitEvent).toHaveBeenCalledWith("launch-ended", { success: false });
      expect(mocks.showToast).not.toHaveBeenCalled();
    });

    it("reports a process failure", async () => {
      mocks.invokeCommand.mockResolvedValue([failed]);
      await start();
      expect(mocks.emitEvent).toHaveBeenCalledWith("launch-ended", { success: false });
    });

    it("still reports confirmed launches as successful", async () => {
      mocks.invokeCommand.mockResolvedValue([confirmed]);
      await start();
      expect(mocks.emitEvent).toHaveBeenCalledWith("launch-ended", {
        success: true,
      });
      expect(mocks.showToast).not.toHaveBeenCalled();
    });

    it("preserves the game's result and warns if reopening Battle.net fails", async () => {
      const warning = "游戏启动已完成，但保留战网失败: 战网配置快照不可用";
      const result = { ...confirmed, error: warning };
      mocks.invokeCommand.mockResolvedValue([result]);
      await start();
      expect(useLaunch.getState().results).toEqual([result]);
      expect(mocks.emitEvent).toHaveBeenCalledWith("launch-ended", { success: true });
      expect(mocks.showToast).toHaveBeenCalledWith("warning", warning);
    });
  });
}
vi.mock("./launchModConfirmation", () => ({ confirmLaunchMod: vi.fn(), cancelLaunchModConfirmation: vi.fn() }));
it.each([false, true])("independent launch retries only on acceptance (%s)", async accept => {
  const { confirmLaunchMod: ask } = await import("./launchModConfirmation");
  vi.mocked(ask).mockResolvedValue(accept);
  mocks.invokeCommand.mockRejectedValueOnce("配置错误: [MOD_PREFLIGHT]账号 one 使用的 Mod Plain 不支持识别").mockResolvedValue([confirmed]);
  await useLaunch.getState().startLaunch(["one"]);
  expect(ask).toHaveBeenCalledWith("账号 one 使用的 Mod Plain 不支持识别");
  expect(mocks.invokeCommand).toHaveBeenCalledTimes(accept ? 2 : 1);
  if (accept) expect(mocks.invokeCommand).toHaveBeenLastCalledWith("launch_accounts", { accountIds: ["one"], acceptedModWarning: "账号 one 使用的 Mod Plain 不支持识别" });
  expect(useLaunch.getState().launching).toBe(false);
  expect(useLaunch.getState().error).toBeNull();
});
it("preserves the saved scheme revision when continuing past a room warning", async () => {
  const { confirmLaunchMod: ask } = await import("./launchModConfirmation");
  vi.mocked(ask).mockResolvedValue(true);
  mocks.invokeCommand.mockRejectedValueOnce("[MOD_PREFLIGHT]room warning").mockResolvedValue([confirmed]);
  await useLaunch.getState().startSavedScheme("scheme", 4);
  expect(mocks.invokeCommand).toHaveBeenLastCalledWith("launch_saved_scheme", { id: "scheme", revision: 4, acceptedModWarning: "room warning" });
});
