import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { TaskGateway } from "./gateway";
import { TaskRuntimePanel } from "./TaskRuntimePanel";
import type { TaskSnapshot, TaskTimelineEntry } from "./types";

afterEach(cleanup);

function snapshot(overrides: Partial<TaskSnapshot> = {}): TaskSnapshot {
  return {
    revision: 1,
    task_id: 7,
    kind: "audio-mod-prepare",
    subject: "account-a",
    conflict_key: "audio-mod-build",
    state: "running",
    progress: 42,
    step: "generating",
    message: "Generating Mod files",
    error_code: null,
    cancel_requested: false,
    retryable: false,
    retry_of: null,
    started_at_ms: Date.now(),
    finished_at_ms: null,
    ...overrides,
  };
}

function gateway(tasks: TaskSnapshot[]): TaskGateway {
  const timeline: TaskTimelineEntry[] = [{
    revision: 1,
    timestamp_ms: Date.now(),
    state: tasks[0]?.state ?? "running",
    progress: tasks[0]?.progress ?? 0,
    step: "preflight",
    message: "Environment ready",
    error_code: null,
    cancel_requested: false,
  }];
  return {
    list: vi.fn(async () => tasks),
    get: vi.fn(async () => tasks[0]),
    timeline: vi.fn(async () => timeline),
    cancel: vi.fn(async (taskId) => snapshot({ task_id: taskId, cancel_requested: true })),
    retryDescriptor: vi.fn(async (taskId) => ({
      kind: "account-reinitialize",
      subject: "account-a",
      retry_of: taskId,
    })),
    retry: vi.fn(async () => undefined),
    subscribe: vi.fn(async () => () => undefined),
  };
}

describe("TaskRuntimePanel", () => {
  it("teaches the empty state without presenting inactive controls", async () => {
    render(<TaskRuntimePanel language="en-US" gateway={gateway([])} />);

    expect(await screen.findByText("No background tasks yet")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
  });

  it("exposes progress, cancellation, and an expandable diagnostic timeline", async () => {
    const user = userEvent.setup();
    const source = gateway([snapshot()]);
    render(<TaskRuntimePanel language="en-US" gateway={source} />);

    expect(await screen.findByRole("progressbar", { name: "Mod processing 42%" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(source.cancel).toHaveBeenCalledWith(7);

    await user.click(screen.getByRole("button", { name: "Timeline" }));
    expect(await screen.findByText("Environment ready")).toBeTruthy();
    expect(source.timeline).toHaveBeenCalledWith(7);
  });

  it("offers retry only for failed tasks declared retryable by the backend", async () => {
    const user = userEvent.setup();
    const source = gateway([snapshot({
      kind: "account-reinitialize",
      state: "failed",
      progress: 20,
      retryable: true,
      error_code: "account-initialization-failed",
      finished_at_ms: Date.now(),
    })]);
    render(<TaskRuntimePanel language="en-US" gateway={source} />);

    await user.click(await screen.findByRole("button", { name: "Retry" }));
    await waitFor(() => expect(source.retry).toHaveBeenCalledWith(7));
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
  });
});
