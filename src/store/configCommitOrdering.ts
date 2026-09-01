/**
 * Backend commit events are the ordered source of truth. A command response is
 * only a fallback for windows that have not observed any event while the IPC
 * request was in flight.
 */
export function shouldApplyConfigCommandResponse<T>(
  snapshotBeforeRequest: T,
  currentSnapshot: T,
): boolean {
  return Object.is(snapshotBeforeRequest, currentSnapshot);
}
