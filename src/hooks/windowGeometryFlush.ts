/**
 * Window geometry persistence is owned by the component that manages the window
 * layout (see `useMiniMode`). Consumers that need the freshest bounds — settings
 * saves and window teardown — flush the registered writers instead of
 * snapshotting the window themselves, so one layout authority stays in charge.
 */
const geometryFlushers = new Set<() => Promise<void>>();

export function registerWindowGeometryFlusher(flush: () => Promise<void>): () => void {
  geometryFlushers.add(flush);
  return () => { geometryFlushers.delete(flush); };
}

export async function flushWindowGeometrySaves(): Promise<void> {
  await Promise.all([...geometryFlushers].map((flush) => flush()));
}
