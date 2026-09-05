export interface RoomFlowStrategy {
  step_delay_ms: number;
  character_delay_ms: number;
  /** Legacy snapshots default to a 50 ms key hold. */
  key_hold_ms?: number;
}

export type FollowerJoinMode = "simultaneous" | "interval";

export interface RoomAutomationConfig {
  enabled: boolean;
  chat_f13_auto_patch_enabled: boolean;
  /** Legacy snapshots and new configurations default to Pause. */
  chat_key?: "pause" | "f13";
  primary_account_id: string;
  follower_account_ids: string[];
  auto_followers_enabled: boolean;
  auto_followers_delay_secs: number;
  /** Absent only on a legacy snapshot; the UI treats it as `simultaneous`. */
  follower_join_mode?: FollowerJoinMode;
  /** Absent only on a legacy snapshot; the UI treats it as 3 seconds. */
  follower_join_interval_secs?: number;
  shortcut: string;
  join_shortcut: string;
  name_prefix: string;
  password: string;
  next_sequence: number;
  sequence_width: number;
  background_text_strategy: "post_keys" | "send_keys";
  strategy_version: number;
  flow: RoomFlowStrategy;
}

export interface RoomAutomationNormalizationReport {
  source_strategy_version: number;
  target_strategy_version: number;
  changed: boolean;
  requires_chat_binding_consent: boolean;
}

export interface RoomAutomationConsentNotice {
  source: string;
  original_strategy_version: number;
  requires_user_reauthorization: boolean;
}

export interface RoomAutomationConfigSnapshot {
  schema_version: number;
  generation: number;
  config: RoomAutomationConfig;
  normalization: RoomAutomationNormalizationReport;
  consent_notice: RoomAutomationConsentNotice | null;
}

export interface RoomAutomationSaveOutcome {
  snapshot: RoomAutomationConfigSnapshot;
  apply_warning: string | null;
}

export type RoomAutomationPhase =
  | "idle"
  | "primary"
  | "waiting"
  | "followers"
  | "complete"
  | "cancelled"
  | "error";

export type RoomAutomationWaitingMode =
  | { mode: "manual" }
  | { mode: "automatic"; delay_secs: number };

export type RoomAutomationRecoveryAction = "retry_primary" | "resume_followers";

export interface RoomAutomationWorkflowStatus {
  revision: number;
  task_id: number | null;
  running: boolean;
  phase: RoomAutomationPhase;
  recovery_action: RoomAutomationRecoveryAction | null;
  waiting_mode: RoomAutomationWaitingMode | null;
  room_name: string | null;
  room_sequence: number | null;
  attempt: number;
  primary_account_id: string | null;
  follower_account_ids: string[];
  completed_follower_account_ids: string[];
  undelivered_follower_account_ids?: string[];
  started_at: string | null;
  last_error: string | null;
}

export interface RoomChatBindingStatus {
  ready: boolean;
  totalFiles: number;
  installedFiles: number;
  eligibleFiles: number;
  conflictedFiles: number;
  backupFiles: number;
  orphanBackupFiles: number;
  transactionArtifacts: number;
  d2rRunning: boolean;
  consentGranted: boolean;
  watcherRunning: boolean;
  autoPatchEnabled: boolean;
  directories: string[];
  lastWatcherError: string | null;
  message: string;
}
