export type TaskState = "running" | "succeeded" | "failed" | "cancelled";

export interface TaskSnapshot {
  revision: number;
  task_id: number;
  kind: string;
  subject: string | null;
  conflict_key: string | null;
  state: TaskState;
  progress: number;
  step: string;
  message: string;
  error_code: string | null;
  cancel_requested: boolean;
  retryable: boolean;
  retry_of: number | null;
  started_at_ms: number;
  finished_at_ms: number | null;
}

export interface TaskTimelineEntry {
  revision: number;
  timestamp_ms: number;
  state: TaskState;
  progress: number;
  step: string;
  message: string;
  error_code: string | null;
  cancel_requested: boolean;
}

export interface TaskRetryDescriptor {
  kind: string;
  subject: string | null;
  retry_of: number;
}
