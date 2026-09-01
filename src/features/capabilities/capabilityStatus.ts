import type {
  CapabilityRuntimeState,
  CapabilityStatus,
  CapabilityStatusSnapshot,
} from "../../store/types";

/**
 * Highest-severity state wins when one settings contribution represents more
 * than one independently supervised capability.
 */
const CAPABILITY_STATE_PRIORITY: Readonly<Record<CapabilityRuntimeState, number>> = {
  disabled: 0,
  stopped: 1,
  running: 2,
  starting: 3,
  degraded: 4,
  failed: 5,
};

export interface CapabilityStatusAggregate {
  requested_enabled: boolean;
  state: CapabilityRuntimeState;
  reason_code: string | null;
}

export interface CapabilityStatusSource {
  /** Register the commit listener and resolve only after it is active. */
  subscribe: (
    listener: (snapshot: CapabilityStatusSnapshot) => void,
  ) => Promise<() => void>;
  readSnapshot: () => Promise<CapabilityStatusSnapshot>;
}

/**
 * Applies only a strictly newer backend snapshot. Equal revisions are treated
 * as the same immutable commit, while a delayed command result cannot replace
 * an event that crossed the request.
 */
export function applyCapabilityStatusSnapshot(
  current: CapabilityStatusSnapshot | null,
  incoming: CapabilityStatusSnapshot,
): CapabilityStatusSnapshot {
  return current === null || incoming.revision > current.revision ? incoming : current;
}

export function indexCapabilityStatuses(
  snapshot: CapabilityStatusSnapshot,
): ReadonlyMap<string, CapabilityStatus> {
  return new Map(snapshot.capabilities.map((status) => [status.id, status]));
}

/**
 * Combines backend-reported states for a single settings contribution.
 *
 * Missing statuses return `null`; callers must show loading/unavailable state
 * instead of falling back to configuration-derived guesses.
 */
export function aggregateCapabilityStatuses(
  snapshot: CapabilityStatusSnapshot | null,
  capabilityIds: readonly string[],
): CapabilityStatusAggregate | null {
  if (snapshot === null || capabilityIds.length === 0) return null;

  const statusesById = indexCapabilityStatuses(snapshot);
  const statuses: CapabilityStatus[] = [];
  for (const id of capabilityIds) {
    const status = statusesById.get(id);
    if (!status) return null;
    statuses.push(status);
  }

  let highestPriority = -1;
  let aggregateState: CapabilityRuntimeState = "disabled";
  let reasonCode: string | null = null;

  for (const status of statuses) {
    const priority = CAPABILITY_STATE_PRIORITY[status.state];
    if (priority > highestPriority) {
      highestPriority = priority;
      aggregateState = status.state;
      reasonCode = status.reason_code;
    } else if (priority === highestPriority && reasonCode === null) {
      reasonCode = status.reason_code;
    }
  }

  return {
    requested_enabled: statuses.some((status) => status.requested_enabled),
    state: aggregateState,
    reason_code: reasonCode,
  };
}

/**
 * Closes the bootstrap race by registering the event listener before reading
 * the initial command snapshot. Both paths share the same revision gate.
 */
export async function subscribeBeforeReadingCapabilityStatuses(
  source: CapabilityStatusSource,
  onSnapshot: (snapshot: CapabilityStatusSnapshot) => void,
  initialSnapshot: CapabilityStatusSnapshot | null = null,
): Promise<() => void> {
  let active = true;
  let current = initialSnapshot;

  const accept = (incoming: CapabilityStatusSnapshot) => {
    if (!active) return;
    const next = applyCapabilityStatusSnapshot(current, incoming);
    if (next !== current) {
      current = next;
      onSnapshot(next);
    }
  };

  const unsubscribe = await source.subscribe(accept);
  try {
    accept(await source.readSnapshot());
  } catch (error) {
    active = false;
    unsubscribe();
    throw error;
  }

  return () => {
    active = false;
    unsubscribe();
  };
}
