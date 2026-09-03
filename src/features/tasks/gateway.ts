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
    await invokeCommand("retry_task", { taskId });
  },
  subscribe: (listener) => listenEvent<TaskSnapshot>(
    "task-status-updated",
    ({ payload }) => listener(payload),
  ),
};
