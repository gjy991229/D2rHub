import { invokeCommand, listenEvent } from "../../platform/tauri";
import type { TaskRetryDescriptor, TaskSnapshot, TaskTimelineEntry } from "./types";

export interface TaskGateway {
  list(): Promise<TaskSnapshot[]>;
  get(taskId: number): Promise<TaskSnapshot>;
  timeline(taskId: number): Promise<TaskTimelineEntry[]>;
  cancel(taskId: number): Promise<TaskSnapshot>;
  retryDescriptor(taskId: number): Promise<TaskRetryDescriptor>;
  retry(taskId: number): Promise<void>;
  subscribe(listener: (snapshot: TaskSnapshot) => void): Promise<() => void>;
}

export const taskGateway: TaskGateway = {
  list: () => invokeCommand<TaskSnapshot[]>("get_tasks"),
  get: (taskId) => invokeCommand<TaskSnapshot>("get_task", { taskId }),
  timeline: (taskId) => invokeCommand<TaskTimelineEntry[]>("get_task_timeline", { taskId }),
  cancel: (taskId) => invokeCommand<TaskSnapshot>("cancel_task", { taskId }),
  retryDescriptor: (taskId) => invokeCommand<TaskRetryDescriptor>(
    "get_task_retry_descriptor",
    { taskId },
  ),
  retry: async (taskId) => {
    const descriptor = await invokeCommand<TaskRetryDescriptor>(
      "get_task_retry_descriptor",
      { taskId },
    );
    if (descriptor.kind === "account-initialize" && descriptor.subject) {
      await invokeCommand("initialize_bnet_account", { accountId: descriptor.subject });
      return;
    }
    if (descriptor.kind === "account-reinitialize" && descriptor.subject) {
      await invokeCommand("reinitialize_account", { accountId: descriptor.subject });
      return;
    }
    if (descriptor.kind === "room-automation") {
      await invokeCommand("room_automation_retry");
      return;
    }
    throw new Error(`Task kind ${descriptor.kind} must be retried from its feature panel`);
  },
  subscribe: (listener) => listenEvent<TaskSnapshot>(
    "task-status-updated",
    ({ payload }) => listener(payload),
  ),
};
