import type { TaskGateway } from "./gateway";
import type { TaskSnapshot } from "./types";

export type TaskSnapshotMap = ReadonlyMap<number, TaskSnapshot>;

export function mergeTaskSnapshot(
  current: TaskSnapshotMap,
  incoming: TaskSnapshot,
): TaskSnapshotMap {
  const previous = current.get(incoming.task_id);
  if (previous && previous.revision >= incoming.revision) return current;
  const next = new Map(current);
  next.set(incoming.task_id, incoming);
  return next;
}

/** Subscribe before the initial read so a task commit cannot be lost in between. */
export async function subscribeBeforeReadingTasks(
  gateway: Pick<TaskGateway, "list" | "subscribe">,
  listener: (tasks: TaskSnapshotMap) => void,
): Promise<() => void> {
  let current: TaskSnapshotMap = new Map();
  let active = true;
  const stop = await gateway.subscribe((snapshot) => {
    if (!active) return;
    const next = mergeTaskSnapshot(current, snapshot);
    if (next !== current) {
      current = next;
      listener(current);
    }
  });

  try {
    const initial = await gateway.list();
    if (active) {
      for (const snapshot of initial) current = mergeTaskSnapshot(current, snapshot);
      listener(current);
    }
  } catch (error) {
    active = false;
    stop();
    throw error;
  }

  return () => {
    active = false;
    stop();
  };
}
