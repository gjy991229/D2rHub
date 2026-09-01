import { describe, expect, it, vi } from "vitest";

import type { TaskGateway } from "./gateway";
import { mergeTaskSnapshot, subscribeBeforeReadingTasks } from "./taskSync";
import type { TaskSnapshot } from "./types";

function task(revision: number, state: TaskSnapshot["state"] = "running"): TaskSnapshot {
  return {
    revision,
    task_id: 7,
    kind: "account-launch",
    subject: null,
    conflict_key: "host-runtime-launch",
    state,
    progress: state === "succeeded" ? 100 : 10,
    step: "running",
    message: "",
    error_code: null,
    cancel_requested: false,
    retryable: true,
    retry_of: null,
    started_at_ms: 1,
    finished_at_ms: state === "running" ? null : 2,
  };
}

describe("task snapshot synchronization", () => {
  it("rejects a stale snapshot for the same task", () => {
    const current = new Map([[7, task(3)]]);
    expect(mergeTaskSnapshot(current, task(2))).toBe(current);
    expect(mergeTaskSnapshot(current, task(4, "succeeded")).get(7)?.state).toBe("succeeded");
  });

  it("subscribes before reading and preserves an intervening event", async () => {
    let listener: ((snapshot: TaskSnapshot) => void) | undefined;
    const stop = vi.fn();
    const gateway: Pick<TaskGateway, "list" | "subscribe"> = {
      subscribe: vi.fn(async (next) => {
        listener = next;
        return stop;
      }),
      list: vi.fn(async () => {
        listener?.(task(5, "succeeded"));
        return [task(4)];
      }),
    };
    const observed: TaskSnapshot["state"][] = [];

    const unsubscribe = await subscribeBeforeReadingTasks(gateway, (tasks) => {
      const state = tasks.get(7)?.state;
      if (state) observed.push(state);
    });

    expect(observed[observed.length - 1]).toBe("succeeded");
    unsubscribe();
    expect(stop).toHaveBeenCalledOnce();
  });
});
