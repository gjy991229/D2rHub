import { useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface Options {
  label: "bongo-cat";
  legacyStorageKey?: string;
  minWidth?: number;
  minHeight?: number;
}

/**
 * Persists native physical placement while continuing to update the legacy
 * logical-pixel record for downgrade compatibility. Call markUserInteraction
 * from the draggable surface before native dragging begins.
 */
export function useWindowPlacementSave({
  label,
  legacyStorageKey,
  minWidth = 50,
  minHeight = 50,
}: Options) {
  const saveTimeout = useRef<number | null>(null);
  const userInteractionPending = useRef(false);

  const markUserInteraction = useCallback(() => {
    userInteractionPending.current = true;
  }, []);

  const scheduleSave = useCallback(() => {
    if (saveTimeout.current !== null) window.clearTimeout(saveTimeout.current);
    saveTimeout.current = window.setTimeout(async () => {
      saveTimeout.current = null;
      try {
        const win = getCurrentWindow();
        if (await win.isMinimized()) return;
        const [position, size, scaleFactor] = await Promise.all([
          win.outerPosition(),
          win.outerSize(),
          win.scaleFactor(),
        ]);
        if (position.x <= -32000 || position.y <= -32000) return;
        const logicalWidth = Math.round(size.width / scaleFactor);
        const logicalHeight = Math.round(size.height / scaleFactor);
        if (logicalWidth < minWidth || logicalHeight < minHeight) return;

        await invoke("save_window_placement", {
          label,
          positionOverride: null,
          dockEdge: null,
          userInitiated: userInteractionPending.current,
        });

        if (legacyStorageKey) {
          localStorage.setItem(legacyStorageKey, JSON.stringify({
            x: Math.round(position.x / scaleFactor),
            y: Math.round(position.y / scaleFactor),
            width: logicalWidth,
            height: logicalHeight,
          }));
        }
        userInteractionPending.current = false;
      } catch (error) {
        console.error(`[WindowPlacement] failed to save ${label}:`, error);
      }
    }, 500);
  }, [label, legacyStorageKey, minHeight, minWidth]);

  useEffect(() => {
    let cancelled = false;
    let unlistenResize: (() => void) | undefined;
    let unlistenMove: (() => void) | undefined;

    (async () => {
      try {
        const win = getCurrentWindow();
        const stopResize = await win.onResized(scheduleSave);
        if (cancelled) {
          stopResize();
          return;
        }
        unlistenResize = stopResize;

        const stopMove = await win.onMoved(scheduleSave);
        if (cancelled) stopMove();
        else unlistenMove = stopMove;
      } catch (error) {
        console.error(`[WindowPlacement] failed to listen for ${label}:`, error);
      }
    })();

    return () => {
      cancelled = true;
      unlistenResize?.();
      unlistenMove?.();
      if (saveTimeout.current !== null) window.clearTimeout(saveTimeout.current);
    };
  }, [label, scheduleSave]);

  return markUserInteraction;
}
