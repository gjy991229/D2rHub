const listeners = new Map<symbol, () => void>();

/** Invalidate mounted account pickers when the Mod workspace changes. No polling. */
export function notifyModCatalogChanged(source?: symbol) {
  for (const [owner, listener] of listeners) if (owner !== source) listener();
}

export function subscribeModCatalogChanges(listener: () => void, owner = Symbol()) {
  listeners.set(owner, listener);
  return () => { listeners.delete(owner); };
}
