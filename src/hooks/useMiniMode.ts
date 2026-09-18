import { useCallback, useEffect, useRef, useState } from "react";
import { availableMonitors, currentMonitor, getCurrentWindow, LogicalSize, PhysicalPosition } from "@tauri-apps/api/window";
import { invokeCommand } from "../platform/tauri";
import { showToast } from "../components/ui/Toast";
import { registerWindowGeometryFlusher } from "./windowGeometryFlush";

// Physical screen position paired with a logical (DPI-independent) size; see
// `snapshot` for why the two units deliberately differ.
type Bounds = { x: number; y: number; width: number; height: number };
type LayoutMode = "full" | "mini";
const PREFIX = "d2rhub-main-layout-v1";
// Minimum logical size per mode. `full` mirrors the main window bounds in
// tauri.conf.json so JS placement never fights the native minimum.
const MIN_SIZE: Record<LayoutMode, { width: number; height: number }> = {
  full: { width: 430, height: 360 },
  mini: { width: 360, height: 280 },
};
function read(key: string): string | null {
  try { return localStorage.getItem(`${PREFIX}:${key}`); } catch { return null; }
}
function write(key: string, value: string) {
  try { localStorage.setItem(`${PREFIX}:${key}`, value); } catch { /* Session still works without storage. */ }
}
function readBounds(key: string): Bounds | null {
  try {
    const value = JSON.parse(read(key) || "null") as Bounds | null;
    return value && [value.x, value.y, value.width, value.height].every(Number.isFinite)
      && value.width >= 100 && value.height >= 100 ? value : null;
  } catch { return null; }
}
async function snapshot(): Promise<Bounds> {
  const win = getCurrentWindow();
  const position = await win.outerPosition();
  const size = await win.innerSize();
  const scale = await win.scaleFactor();
  // Keep positions physical: dividing negative monitor coordinates by the
  // current window's DPI loses the monitor origin on mixed-DPI desktops.
  return { x: position.x, y: position.y, width: size.width / scale, height: size.height / scale };
}
async function place(bounds: Bounds, mini: boolean) {
  const win = getCurrentWindow();
  const monitors = await availableMonitors();
  const monitor = monitors.find(item => {
    const { position, size } = item.workArea;
    return bounds.x >= position.x && bounds.x < position.x + size.width
      && bounds.y >= position.y && bounds.y < position.y + size.height;
  }) || await currentMonitor() || monitors[0];
  const min = MIN_SIZE[mini ? "mini" : "full"];
  const scale = monitor?.scaleFactor || await win.scaleFactor();
  const area = monitor?.workArea;
  const width = Math.max(min.width, Math.min(bounds.width, area ? area.size.width / scale : bounds.width));
  const height = Math.max(min.height, Math.min(bounds.height, area ? area.size.height / scale : bounds.height));
  const x = area ? Math.max(area.position.x, Math.min(bounds.x, area.position.x + area.size.width - width * scale)) : bounds.x;
  const y = area ? Math.max(area.position.y, Math.min(bounds.y, area.position.y + area.size.height - height * scale)) : bounds.y;
  await win.setMinSize(new LogicalSize(min.width, min.height));
  await win.setPosition(new PhysicalPosition(Math.round(x), Math.round(y)));
  await win.setSize(new LogicalSize(width, height));
}

export function useMiniMode(ready: boolean, forceFull: boolean) {
  const [mini, setMini] = useState(false);
  const [busy, setBusy] = useState(true);
  const [pinned, setPinned] = useState(() => read("pinned") === "true");
  const current = useRef(false);
  const changing = useRef(true);
  const initialized = useRef(false);
  const restored = useRef(false);
  const expansionRequested = useRef(false);
  const startupMini = useRef(read("mode") === "mini");
  const full = useRef<Bounds | null>(readBounds("full"));
  const compact = useRef<Bounds | null>(readBounds("mini"));
  const chain = useRef<Promise<void>>(Promise.resolve());

  const save = useCallback(async () => {
    if (!initialized.current || changing.current) return;
    const win = getCurrentWindow();
    if (await win.isMinimized()) return;
    const mini = current.current;
    const bounds = await snapshot();
    if (changing.current || mini !== current.current || bounds.x <= -32000 || bounds.y <= -32000) return;
    // Never persist a transient layout left behind by a failed placement.
    const min = MIN_SIZE[mini ? "mini" : "full"];
    if (bounds.width < min.width || bounds.height < min.height) return;
    (mini ? compact : full).current = bounds;
    write(mini ? "mini" : "full", JSON.stringify(bounds));
    // The full layout is the authoritative one for the native window; compact
    // bounds stay local so a collapsed window never rewrites it.
    if (!mini) {
      const scale = await win.scaleFactor();
      await invokeCommand("save_window_geometry", { geometry: {
        x: Math.round(bounds.x / scale), y: Math.round(bounds.y / scale),
        width: Math.round(bounds.width), height: Math.round(bounds.height),
      } });
    }
  }, []);
  const flush = useCallback(() => {
    const pending = chain.current.catch(() => undefined).then(save);
    chain.current = pending;
    return pending;
  }, [save]);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const stops: Array<() => void> = [];
    const schedule = () => {
      clearTimeout(timer);
      timer = setTimeout(() => { void flush().catch(console.error); }, 350);
    };
    const boot = async () => {
      if (cancelled) return;
      try {
        const bounds = full.current || await snapshot();
        full.current = bounds;
        await place(bounds, false);
        if (cancelled) return;
        const win = getCurrentWindow();
        for (const listen of [() => win.onMoved(schedule), () => win.onResized(schedule)]) {
          const stop = await listen();
          if (cancelled) { stop(); return; }
          stops.push(stop);
        }
      } catch (error) {
        if (!cancelled) showToast("error", `恢复窗口失败：${error}`);
      } finally {
        // Geometry persistence must survive a failed placement: the user still
        // moves and resizes the window afterwards.
        if (!cancelled) { initialized.current = true; changing.current = false; setBusy(false); }
      }
    };
    const pending = chain.current.catch(() => undefined).then(boot);
    chain.current = pending;
    const unregister = registerWindowGeometryFlusher(flush);
    return () => { cancelled = true; clearTimeout(timer); stops.forEach(stop => stop()); unregister(); };
  }, [flush]);

  const switchMode = useCallback(async (next: boolean): Promise<boolean> => {
    if (changing.current) return false;
    if (current.current === next) return true;
    changing.current = true;
    setBusy(true);
    const previous = current.current;
    let before: Bounds | undefined;
    try {
      await chain.current.catch(() => undefined);
      before = await snapshot();
      (previous ? compact : full).current = before;
      write(previous ? "mini" : "full", JSON.stringify(before));
      const target = next ? compact.current || { ...before, width: 380, height: 440 } : full.current || before;
      await place(target, next);
      await getCurrentWindow().setAlwaysOnTop(next && pinned);
      current.current = next;
      setMini(next);
      write("mode", next ? "mini" : "full");
      return true;
    } catch (error) {
      if (before) await place(before, previous).catch(console.error);
      showToast("error", `切换窗口模式失败：${error}`);
      return false;
    } finally { changing.current = false; setBusy(false); }
  }, [pinned]);

  useEffect(() => {
    if (busy || !ready || restored.current) return;
    restored.current = true;
    if (startupMini.current && !forceFull) void switchMode(true);
  }, [busy, ready, forceFull, switchMode]);
  useEffect(() => {
    if (!forceFull) expansionRequested.current = false;
    if (!busy && mini && forceFull && !expansionRequested.current) {
      expansionRequested.current = true;
      void switchMode(false);
    }
  }, [busy, mini, forceFull, switchMode]);

  const togglePin = async () => {
    if (changing.current) return;
    changing.current = true;
    setBusy(true);
    try {
      await getCurrentWindow().setAlwaysOnTop(!pinned);
      write("pinned", String(!pinned));
      setPinned(value => !value);
    } catch (error) { showToast("error", `设置置顶失败：${error}`); }
    finally { changing.current = false; setBusy(false); }
  };
  return { mini, busy, pinned, switchMode, togglePin };
}
