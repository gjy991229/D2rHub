import { useEffect, useCallback, useRef } from "react";
import { invokeCommand, type TauriCommandName } from "../platform/tauri";
import { getCurrentWindow } from "@tauri-apps/api/window";

type GeometryPersistenceTarget = TauriCommandName | `localStorage:${string}`;
const geometryFlushers = new Set<() => Promise<void>>();

export async function flushWindowGeometrySaves(): Promise<void> {
  await Promise.all([...geometryFlushers].map((flush) => flush()));
}

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
  const saveChain = useRef<Promise<void>>(Promise.resolve());

  const persistGeometry = useCallback(async () => {
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
  }, [commandName, minWidth, minHeight]);

  const flush = useCallback(() => {
    if (saveTimeout.current !== null) {
      window.clearTimeout(saveTimeout.current);
      saveTimeout.current = null;
    }
    // An older in-flight read/save must finish before the final fresh snapshot.
    const pending = saveChain.current.catch(() => undefined).then(persistGeometry);
    saveChain.current = pending;
    return pending;
  }, [persistGeometry]);

  const scheduleGeometrySave = useCallback(() => {
    if (saveTimeout.current !== null) window.clearTimeout(saveTimeout.current);
    saveTimeout.current = window.setTimeout(() => {
      void flush().catch((err) => console.error(`Failed to save geometry using ${commandName}:`, err));
    }, 500);
  }, [commandName, flush]);

  useEffect(() => {
    geometryFlushers.add(flush);
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
      geometryFlushers.delete(flush);
      cancelled = true;
      unlistenResize?.();
      unlistenMove?.();
      if (saveTimeout.current !== null) {
        window.clearTimeout(saveTimeout.current);
      }
    };
  }, [scheduleGeometrySave, flush]);
}
