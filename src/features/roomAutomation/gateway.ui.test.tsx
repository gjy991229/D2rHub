import { afterEach, describe, expect, it, vi } from "vitest";
import type { RoomAutomationConfigSnapshot, RoomAutomationWorkflowStatus } from "./types";

const platform = vi.hoisted(() => ({
  invokeCommand: vi.fn(),
  listenEvent: vi.fn(),
}));

vi.mock("../../platform/tauri", () => platform);

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

