import { invokeCommand, listenEvent } from "../../platform/tauri";
import type {
  RoomAutomationConfig,
  RoomAutomationConfigSnapshot,
  RoomAutomationSaveOutcome,
  RoomAutomationWorkflowStatus,
  RoomChatBindingStatus,
} from "./types";

export interface RoomAutomationSyncHandlers {
  onConfig: (snapshot: RoomAutomationConfigSnapshot) => void;
  onStatus: (status: RoomAutomationWorkflowStatus) => void;
}

export interface RoomAutomationGateway {
  getConfig(): Promise<RoomAutomationConfigSnapshot>;
  saveConfig(expectedGeneration: number, config: RoomAutomationConfig): Promise<RoomAutomationSaveOutcome>;
  getStatus(): Promise<RoomAutomationWorkflowStatus>;
  startPrimary(): Promise<RoomAutomationWorkflowStatus>;
  startFollowers(): Promise<RoomAutomationWorkflowStatus>;
  retry(): Promise<RoomAutomationWorkflowStatus>;
  cancel(): Promise<RoomAutomationWorkflowStatus>;
  getChatBinding(): Promise<RoomChatBindingStatus>;
  installChatBinding(): Promise<RoomChatBindingStatus>;
  restoreChatBinding(): Promise<RoomChatBindingStatus>;
  startSync(handlers: RoomAutomationSyncHandlers): Promise<() => void>;
}

function acceptNewer<T extends { revision: number }>(
  currentRevision: { value: number },
  value: T,
  accept: (value: T) => void,
): void {
  if (value.revision < currentRevision.value) return;
  currentRevision.value = value.revision;
  accept(value);
}

function acceptNewerConfig(
  currentGeneration: { value: number },
  value: RoomAutomationConfigSnapshot,
  accept: (value: RoomAutomationConfigSnapshot) => void,
): void {
  if (value.generation < currentGeneration.value) return;
  currentGeneration.value = value.generation;
  accept(value);
}

export const roomAutomationGateway: RoomAutomationGateway = {
  getConfig: () => invokeCommand<RoomAutomationConfigSnapshot>("room_automation_get_config"),
  saveConfig: (expectedGeneration, config) => invokeCommand<RoomAutomationSaveOutcome>(
    "room_automation_save_config",
    { expectedGeneration, config },
  ),
  getStatus: () => invokeCommand<RoomAutomationWorkflowStatus>("room_automation_get_status"),
  startPrimary: () => invokeCommand<RoomAutomationWorkflowStatus>("room_automation_start_primary"),
  startFollowers: () => invokeCommand<RoomAutomationWorkflowStatus>("room_automation_start_followers"),
  retry: () => invokeCommand<RoomAutomationWorkflowStatus>("room_automation_retry"),
  cancel: () => invokeCommand<RoomAutomationWorkflowStatus>("room_automation_cancel"),
  getChatBinding: () => invokeCommand<RoomChatBindingStatus>("room_automation_get_chat_binding"),
  installChatBinding: () => invokeCommand<RoomChatBindingStatus>("room_automation_install_chat_binding"),
  restoreChatBinding: () => invokeCommand<RoomChatBindingStatus>("room_automation_restore_chat_binding"),
  startSync: async ({ onConfig, onStatus }) => {
    const statusRevision = { value: -1 };
    const configGeneration = { value: -1 };
    const stops: Array<() => void> = [];

    try {
      // Both subscriptions are established before either initial read begins.
      stops.push(await listenEvent<RoomAutomationWorkflowStatus>(
        "room-automation://status-changed",
        ({ payload }) => acceptNewer(statusRevision, payload, onStatus),
      ));
      stops.push(await listenEvent<RoomAutomationConfigSnapshot>(
        "room-automation://config-committed",
        ({ payload }) => acceptNewerConfig(configGeneration, payload, onConfig),
      ));

      const [config, status] = await Promise.all([
        roomAutomationGateway.getConfig(),
        roomAutomationGateway.getStatus(),
      ]);
      acceptNewerConfig(configGeneration, config, onConfig);
      acceptNewer(statusRevision, status, onStatus);
    } catch (error) {
      stops.forEach((stop) => stop());
      throw error;
    }

    return () => stops.forEach((stop) => stop());
  },
};
