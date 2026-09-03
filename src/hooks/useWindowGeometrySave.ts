import { useEffect, useCallback, useRef } from "react";
import { invokeCommand, type TauriCommandName } from "../platform/tauri";
import { getCurrentWindow } from "@tauri-apps/api/window";

type GeometryPersistenceTarget = TauriCommandName | `localStorage:${string}`;

function isLocalStorageTarget(
  target: GeometryPersistenceTarget,
): target is `localStorage:${string}` {
  return target.startsWith("localStorage:");
}

export function useWindowGeometrySave(
  commandName: GeometryPersistenceTarget,
  minWidth: number = 100,
  minHeight: number = 100,
) {
  const saveTimeout = useRef<number | null>(null);

  const scheduleGeometrySave = useCallback(() => {
    if (saveTimeout.current !== null) {
      window.clearTimeout(saveTimeout.current);
    }
    saveTimeout.current = window.setTimeout(async () => {
      try {
        const win = getCurrentWindow();
        const minimized = await win.isMinimized();
        if (minimized) return;

        const pos = await win.outerPosition();
        if (pos.x <= -32000 || pos.y <= -32000) return;

        const size = await win.outerSize();
        const scale = await win.scaleFactor();
        const w = Math.round(size.width / scale);
        const h = Math.round(size.height / scale);
        if (w < minWidth || h < minHeight) return;

        const geometry = {
          x: Math.round(pos.x / scale),
          y: Math.round(pos.y / scale),
          width: w,
          height: h,
        };

        if (isLocalStorageTarget(commandName)) {
          const key = commandName.replace("localStorage:", "");
          localStorage.setItem(key, JSON.stringify(geometry));
        } else {
          await invokeCommand(commandName, { geometry });
        }
      } catch (err) {
        console.error(`Failed to save geometry using ${commandName}:`, err);
      }
    }, 500);
  }, [commandName, minWidth, minHeight]);

  useEffect(() => {
    let cancelled = false;
    let unlistenResize: (() => void) | undefined;
    let unlistenMove: (() => void) | undefined;

    (async () => {
      try {
        const win = getCurrentWindow();
        const stopResize = await win.onResized(() => scheduleGeometrySave());
        if (cancelled) {
          stopResize();
          return;
        }
        unlistenResize = stopResize;

        const stopMove = await win.onMoved(() => scheduleGeometrySave());
        if (cancelled) {
          stopMove();
        } else {
          unlistenMove = stopMove;
        }
      } catch (err) {
        console.error("Failed to listen for geometry changes:", err);
      }
    })();

    return () => {
      cancelled = true;
      unlistenResize?.();
      unlistenMove?.();
      if (saveTimeout.current !== null) {
        window.clearTimeout(saveTimeout.current);
      }
    };
  }, [scheduleGeometrySave]);
}
