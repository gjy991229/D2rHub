import { afterEach, describe, expect, it, vi } from "vitest";
import type { RoomAutomationConfigSnapshot, RoomAutomationWorkflowStatus } from "./types";

const platform = vi.hoisted(() => ({
  invokeCommand: vi.fn(),
  listenEvent: vi.fn(),
}));

vi.mock("../../platform/tauri", () => platform);
vi.mock("@tauri-apps/plugin-dialog", () => ({ confirm: vi.fn() }));
import { confirm } from "@tauri-apps/plugin-dialog";
import type { RoomAutomationConfig } from "./types";

import { roomAutomationGateway } from "./gateway";

afterEach(() => {
  vi.clearAllMocks();
});

describe("room automation gateway synchronization", () => {
  it("subscribes before reading and rejects an older initial workflow snapshot", async () => {
    const order: string[] = [];
    const listeners = new Map<string, (event: { payload: unknown }) => void>();
    let resolveStatus: ((status: RoomAutomationWorkflowStatus) => void) | undefined;
    const olderStatus = { revision: 4 } as RoomAutomationWorkflowStatus;
    const newerStatus = { revision: 9 } as RoomAutomationWorkflowStatus;
    const config = { generation: 2 } as RoomAutomationConfigSnapshot;

    platform.listenEvent.mockImplementation(async (name: string, listener: (event: { payload: unknown }) => void) => {
      order.push(`listen:${name}`);
      listeners.set(name, listener);
      return () => undefined;
    });
    platform.invokeCommand.mockImplementation((name: string) => {
      order.push(`invoke:${name}`);
      if (name === "room_automation_get_config") return Promise.resolve(config);
      if (name === "room_automation_get_status") {
        return new Promise<RoomAutomationWorkflowStatus>((resolve) => { resolveStatus = resolve; });
      }
      throw new Error(`Unexpected command ${name}`);
    });
    const observed: number[] = [];
    const sync = roomAutomationGateway.startSync({
      onConfig: () => undefined,
      onStatus: (status) => observed.push(status.revision),
    });

    await vi.waitFor(() => expect(listeners.size).toBe(2));
    listeners.get("room-automation://status-changed")?.({ payload: newerStatus });
    resolveStatus?.(olderStatus);
    const stop = await sync;

    expect(order.slice(0, 2)).toEqual([
      "listen:room-automation://status-changed",
      "listen:room-automation://config-committed",
    ]);
    expect(order[2]?.startsWith("invoke:")).toBe(true);
    expect(observed).toEqual([9]);
    stop();
  });
});


it("does not download resources for ordinary modes", async () => {
  platform.invokeCommand.mockResolvedValue({});
  await roomAutomationGateway.saveConfig(1, { collaboration_mode: "default" } as RoomAutomationConfig);
  expect(platform.invokeCommand).not.toHaveBeenCalledWith("vision_prepare_resource_package");
  expect(confirm).not.toHaveBeenCalled();
});
it("cancelled confirmation leaves follower configuration unchanged", async () => {
  platform.invokeCommand.mockResolvedValue({ ready: false, size: 90000000 });
  vi.mocked(confirm).mockResolvedValue(false);
  await expect(roomAutomationGateway.saveConfig(1, { collaboration_mode: "follower" } as RoomAutomationConfig)).rejects.toThrow("已取消");
  expect(platform.invokeCommand).not.toHaveBeenCalledWith("vision_prepare_resource_package");
  expect(platform.invokeCommand.mock.calls.some(([name]) => name === "room_automation_save_config")).toBe(false);
});
it("downloads only after confirmation and then applies follower configuration", async () => {
  platform.invokeCommand.mockResolvedValue({ ready: false, size: 90000000 });
  vi.mocked(confirm).mockResolvedValue(true);
  await roomAutomationGateway.saveConfig(1, { collaboration_mode: "follower" } as RoomAutomationConfig);
  const names = platform.invokeCommand.mock.calls.map(([name]) => name);
  expect(names.indexOf("vision_prepare_resource_package")).toBeLessThan(names.indexOf("room_automation_save_config"));
});
