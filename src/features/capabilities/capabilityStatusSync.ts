import { invokeCommand, listenEvent } from "../../platform/tauri";
import type { CapabilityStatusSnapshot } from "../../store/types";
import {
  subscribeBeforeReadingCapabilityStatuses,
  type CapabilityStatusSource,
} from "./capabilityStatus";

const tauriCapabilityStatusSource: CapabilityStatusSource = {
  subscribe: (listener) => listenEvent<CapabilityStatusSnapshot>(
    "capability-status-updated",
    ({ payload }) => listener(payload),
  ),
  readSnapshot: () => invokeCommand<CapabilityStatusSnapshot>("get_capability_statuses"),
};

/** Subscribe to backend commits before requesting the initial status snapshot. */
export function initCapabilityStatusSync(
  onSnapshot: (snapshot: CapabilityStatusSnapshot) => void,
  initialSnapshot: CapabilityStatusSnapshot | null = null,
): Promise<() => void> {
  return subscribeBeforeReadingCapabilityStatuses(
    tauriCapabilityStatusSource,
    onSnapshot,
    initialSnapshot,
  );
}
