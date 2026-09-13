import { create } from "zustand";
import { invokeCommand, listenEvent } from "../../platform/tauri";
import type { PetAction, PetOutcome, PetSnapshot } from "./types";

interface WardrobeStore { snapshot: PetSnapshot | null; error: string | null }
export const usePetWardrobe = create<WardrobeStore>(() => ({ snapshot: null, error: null }));
function accept(snapshot: PetSnapshot) {
  const current = usePetWardrobe.getState().snapshot;
  if (!current || snapshot.revision >= current.revision) {
    // Progress commits must not rebuild the chatter picker or reset its history.
    if (current && JSON.stringify(current.wardrobe.equipped) === JSON.stringify(snapshot.wardrobe.equipped)) {
      snapshot = { ...snapshot, wardrobe: { ...snapshot.wardrobe, equipped: current.wardrobe.equipped } };
    }
    usePetWardrobe.setState({ snapshot, error: null });
  }
}
let users = 0;
let connection: Promise<() => void> | null = null;
async function ensureConnection(): Promise<void> {
  if (!connection) {
    const pending = listenEvent<PetSnapshot>("pet-wardrobe-updated", event => accept(event.payload));
    connection = pending;
    void pending.catch(() => { if (connection === pending) connection = null; });
  }
  await connection;
}
/** One subscription per webview, shared by previews and controls. */
export function connectWardrobe(): () => void {
  users += 1;
  void reloadWardrobe();
  let released = false;
  return () => {
    if (released) return;
    released = true;
    users -= 1;
    if (users === 0 && connection) {
      void connection.then(unlisten => unlisten()).catch(() => {});
      connection = null;
    }
  };
}
export async function reloadWardrobe() {
  try {
    if (users > 0) await ensureConnection();
    const outcome = await invokeCommand<PetOutcome>("pet_get_wardrobe");
    // Process-local revision orders recovery events and delayed responses even
    // when restoring a backup rolls the on-disk generation backwards.
    accept(outcome.snapshot);
  }
  catch (error) { usePetWardrobe.setState({ error: String(error) }); }
}
let queue: Promise<unknown> = Promise.resolve();
export function changeWardrobe(action: PetAction): Promise<void> {
  const run = async () => {
    try {
      const outcome = await invokeCommand<PetOutcome>("pet_wardrobe_action", { action });
      accept(outcome.snapshot);
    } catch (error) {
      await reloadWardrobe();
      usePetWardrobe.setState({ error: String(error) });
      throw error;
    }
  };
  const pending = queue.then(run, run);
  queue = pending.catch(() => {});
  return pending;
}
export async function settlePetActivity(): Promise<PetOutcome> {
  try {
    const outcome = await invokeCommand<PetOutcome>("pet_settle_activity");
    accept(outcome.snapshot);
    return outcome;
  } catch (error) {
    await reloadWardrobe();
    usePetWardrobe.setState({ error: String(error) });
    throw error;
  }
}
