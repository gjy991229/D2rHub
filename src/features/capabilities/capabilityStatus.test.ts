import type { CapabilityStatusSnapshot } from "../../store/types";
import {
  aggregateCapabilityStatuses,
  applyCapabilityStatusSnapshot,
  subscribeBeforeReadingCapabilityStatuses,
  type CapabilityStatusSource,
} from "./capabilityStatus";

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

function snapshot(
  revision: number,
  capabilities: CapabilityStatusSnapshot["capabilities"],
): CapabilityStatusSnapshot {
  return { revision, capabilities };
}

const running = snapshot(3, [{
  id: "audio-telemetry",
  requested_enabled: true,
  state: "running",
  reason_code: null,
}]);
const stale = snapshot(2, [{
  id: "audio-telemetry",
  requested_enabled: true,
  state: "failed",
  reason_code: "stale-error",
}]);

assert(
  applyCapabilityStatusSnapshot(running, stale) === running,
  "an older capability snapshot cannot replace a newer backend revision",
);
assert(
  applyCapabilityStatusSnapshot(running, snapshot(3, [])) === running,
  "equal revisions are the same immutable backend commit",
);

const aggregate = aggregateCapabilityStatuses(snapshot(7, [
  {
    id: "terror-zone-overlay",
    requested_enabled: true,
    state: "running",
    reason_code: null,
  },
  {
    id: "stats-overlay",
    requested_enabled: true,
    state: "degraded",
    reason_code: "window-unavailable",
  },
]), ["terror-zone-overlay", "stats-overlay"]);

assert(
  aggregate?.state === "degraded" && aggregate.reason_code === "window-unavailable",
  "aggregate status uses failed/degraded/starting/running/stopped/disabled priority",
);
assert(
  aggregate?.requested_enabled === true,
  "aggregate status preserves the backend's requested-enabled intent",
);

const statesByAscendingPriority = [
  "disabled",
  "stopped",
  "running",
  "starting",
  "degraded",
  "failed",
] as const;
for (let index = 1; index < statesByAscendingPriority.length; index += 1) {
  const lower = statesByAscendingPriority[index - 1];
  const higher = statesByAscendingPriority[index];
  const prioritized = aggregateCapabilityStatuses(snapshot(8 + index, [
    {
      id: "lower-priority",
      requested_enabled: lower !== "disabled",
      state: lower,
      reason_code: `reason-${lower}`,
    },
    {
      id: "higher-priority",
      requested_enabled: higher !== "disabled",
      state: higher,
      reason_code: `reason-${higher}`,
    },
  ]), ["lower-priority", "higher-priority"]);
  assert(
    prioritized?.state === higher && prioritized.reason_code === `reason-${higher}`,
    `${higher} takes precedence over ${lower} when capability states are aggregated`,
  );
}

assert(
  aggregateCapabilityStatuses(snapshot(8, [{
    id: "terror-zone-overlay",
    requested_enabled: true,
    state: "running",
    reason_code: null,
  }]), ["terror-zone-overlay", "stats-overlay"]) === null,
  "missing backend capability state stays unknown instead of using config inference",
);

async function testSubscribeBeforeReadOrdering() {
  const eventSnapshot = snapshot(11, [{
    id: "desktop-pet",
    requested_enabled: true,
    state: "running",
    reason_code: null,
  }]);
  const commandSnapshot = snapshot(10, [{
    id: "desktop-pet",
    requested_enabled: true,
    state: "starting",
    reason_code: null,
  }]);
  const seen: number[] = [];
  let subscribed = false;
  let listener: ((next: CapabilityStatusSnapshot) => void) | null = null;
  let unsubscribeCount = 0;

  const source: CapabilityStatusSource = {
    subscribe: async (nextListener) => {
      subscribed = true;
      listener = nextListener;
      return () => { unsubscribeCount += 1; };
    },
    readSnapshot: async () => {
      assert(subscribed, "the commit listener is active before the initial command read");
      const emit = listener as ((next: CapabilityStatusSnapshot) => void) | null;
      if (!emit) throw new Error("capability listener was not registered");
      emit(eventSnapshot);
      return commandSnapshot;
    },
  };

  const unsubscribe = await subscribeBeforeReadingCapabilityStatuses(
    source,
    (next) => seen.push(next.revision),
  );

  assert(
    seen.length === 1 && seen[0] === eventSnapshot.revision,
    "a stale initial command response cannot overwrite an intervening status event",
  );
  unsubscribe();
  assert(unsubscribeCount === 1, "capability status synchronization releases its listener");
}

async function testReadFailureCleanup() {
  let unsubscribeCount = 0;
  const source: CapabilityStatusSource = {
    subscribe: async () => () => { unsubscribeCount += 1; },
    readSnapshot: async () => { throw new Error("backend unavailable"); },
  };

  let rejected = false;
  try {
    await subscribeBeforeReadingCapabilityStatuses(source, () => {});
  } catch {
    rejected = true;
  }

  assert(rejected, "an initial capability snapshot failure is reported to the caller");
  assert(unsubscribeCount === 1, "a failed initial snapshot releases the registered listener");
}

async function runAsyncTests() {
  await testSubscribeBeforeReadOrdering();
  await testReadFailureCleanup();
}

void runAsyncTests();
