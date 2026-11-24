import type { LayoutMonitor, LayoutRect, LayoutSlot, WindowLayout } from "../../store/types";

export const MIN_LAYOUT_WIDTH = 800;
export const MIN_LAYOUT_HEIGHT = 600;
export const MAX_LAYOUT_WIDTH = 7680;
export const MAX_LAYOUT_HEIGHT = 4320;
export const MAX_LAYOUT_WINDOWS = 32;
// Keep a recoverable strip visible while allowing borders to sit offscreen.
export const MIN_VISIBLE_PIXELS = 64;
export type PresetResolution = "1080p" | "2k" | "4k";
export type ResizeHandle = "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "nw";

const clamp = (value: number, min: number, max: number) => Math.round(Math.min(Math.max(value, min), max));

export function primaryMonitor(monitors: readonly LayoutMonitor[]): LayoutMonitor | undefined {
  return monitors.find(monitor => monitor.primary) ?? monitors[0];
}

export function defaultPresetResolution(monitor?: LayoutMonitor): PresetResolution {
  return (monitor?.bounds.width ?? 0) >= 3840 ? "4k" : (monitor?.bounds.width ?? 0) >= 2560 ? "2k" : "1080p";
}

export function visibleInsets(monitor?: LayoutMonitor, fullscreen = false) {
  return (!fullscreen && monitor?.frame?.visible) || { left: 0, top: 0, right: 0, bottom: 0 };
}

export function visibleSize(slot: Pick<LayoutSlot, "width" | "height" | "fullscreen">, monitor?: LayoutMonitor) {
  if (slot.fullscreen && monitor) return { width: monitor.bounds.width, height: monitor.bounds.height };
  const edge = visibleInsets(monitor);
  return { width: slot.width + edge.left + edge.right, height: slot.height + edge.top + edge.bottom };
}

export function visibleSlotRect(slot: LayoutSlot, monitors: readonly LayoutMonitor[]): LayoutRect {
  const monitor = monitors.find(candidate => candidate.id === slot.monitor_id) ?? primaryMonitor(monitors);
  return { ...slotRect(slot, monitors), ...visibleSize(slot, monitor) };
}

export function constrainSlot(slot: LayoutSlot, monitor: LayoutMonitor): LayoutSlot {
  if (slot.fullscreen) return { ...slot, monitor_id: monitor.id };
  const work = monitor.work_area;
  const width = clamp(slot.width, MIN_LAYOUT_WIDTH, MAX_LAYOUT_WIDTH);
  const height = clamp(slot.height, MIN_LAYOUT_HEIGHT, MAX_LAYOUT_HEIGHT);
  const visible = visibleSize({ width, height }, monitor);
  return { ...slot, monitor_id: monitor.id, width, height,
    x: clamp(slot.x, MIN_VISIBLE_PIXELS - visible.width, Math.max(0, work.width - visible.width)),
    y: clamp(slot.y, MIN_VISIBLE_PIXELS - visible.height, Math.max(0, work.height - visible.height)) };
}

export function slotRect(slot: LayoutSlot, monitors: readonly LayoutMonitor[]): LayoutRect {
  const monitor = monitors.find(monitor => monitor.id === slot.monitor_id) ?? primaryMonitor(monitors);
  if (slot.fullscreen && monitor) return monitor.bounds;
  return { x: slot.x + (monitor?.work_area.x ?? 0), y: slot.y + (monitor?.work_area.y ?? 0), width: slot.width, height: slot.height };
}

function adaptAxis(offset: number, size: number, previous: number, nextSize: number, next: number): number {
  if (offset < 0) return Math.max(MIN_VISIBLE_PIXELS - nextSize, offset);
  if (previous === next && size === nextSize) return offset;
  const span = previous - size;
  const anchor = span <= 0 ? 0.5 : offset / span;
  return Math.round(Math.min(1, Math.max(0, anchor)) * Math.max(0, next - nextSize));
}

/** Mirrors the backend's edge/center anchoring and disconnected-screen fallback. */
export function adaptLayout(layout: WindowLayout, monitors: LayoutMonitor[]): WindowLayout {
  const primary = primaryMonitor(monitors);
  if (!primary) return layout;
  return { ...layout, monitors, windows: layout.windows.map(slot => {
    const saved = layout.monitors.find(monitor => monitor.id === slot.monitor_id) ?? primary;
    const legacy = saved.id.startsWith("\\\\.\\DISPLAY");
    const current = monitors.find(monitor => monitor.id === slot.monitor_id)
      ?? (legacy ? monitors.find(monitor => monitor.bounds.x === saved.bounds.x && monitor.bounds.y === saved.bounds.y
        && monitor.bounds.width === saved.bounds.width && monitor.bounds.height === saved.bounds.height)
        ?? monitors.find(monitor => monitor.name === saved.name) : undefined) ?? primary;
    if (slot.fullscreen) return { ...slot, monitor_id: current.id };
    const { width, height } = slot;
    const before = visibleSize(slot, saved), after = visibleSize(slot, current);
    return constrainSlot({ ...slot, monitor_id: current.id, width, height,
      x: adaptAxis(slot.x, before.width, saved.work_area.width, after.width, current.work_area.width),
      y: adaptAxis(slot.y, before.height, saved.work_area.height, after.height, current.work_area.height) }, current);
  }) };
}

/** Presets fill a custom-layout draft, exclusively on the current primary. */
export function presetSlots(monitors: readonly LayoutMonitor[], count: number, resolution: PresetResolution): LayoutSlot[] {
  const monitor = primaryMonitor(monitors);
  if (!monitor) return [];
  const work = monitor.work_area;
  const total = clamp(count, 1, MAX_LAYOUT_WINDOWS);
  const mainSize = resolution === "4k" ? [1920, 1080] : resolution === "2k" ? [1600, 900] : [1280, 720];
  const followerSize = resolution === "4k" && total <= 4 ? [1600, 900] : [1280, 720];
  const create = (size: number[], anchorX: number, anchorY: number): LayoutSlot => {
    const [width, height] = size;
    const visible = visibleSize({ width, height }, monitor);
    return { monitor_id: monitor.id, width, height, x: Math.round(Math.max(0, work.width - visible.width) * anchorX), y: Math.round(Math.max(0, work.height - visible.height) * anchorY) };
  };
  // Balanced corners first, then the middle of the outer edges. The larger
  // primary stays centered instead of being squeezed into a tiny grid cell.
  const anchors = [[0, 0], [1, 1], [1, 0], [0, 1], [0.5, 0], [0.5, 1], [0, 0.5], [1, 0.5]];
  const slots = [create(mainSize, 0.5, 0.5)];
  for (let index = 1; index < total; index += 1) {
    const anchor = anchors[(index - 1) % anchors.length];
    const inset = Math.floor((index - 1) / anchors.length) * 0.08;
    slots.push(create(followerSize, anchor[0] * (1 - 2 * inset) + inset, anchor[1] * (1 - 2 * inset) + inset));
  }
  return slots;
}

export function changeSlotCount(layout: WindowLayout, count: number, resolution: PresetResolution): WindowLayout {
  const total = clamp(count, 1, MAX_LAYOUT_WINDOWS);
  const extra = presetSlots(layout.monitors, total, resolution);
  return { ...layout, windows: Array.from({ length: total }, (_, index) => layout.windows[index] ?? extra[index]) };
}

function closestDelta(edges: number[], targets: number[], tolerance: number): number {
  let result = 0;
  let distance = tolerance + 0.001;
  for (const edge of edges) for (const target of targets) {
    const delta = target - edge;
    if (Math.abs(delta) < distance) { result = delta; distance = Math.abs(delta); }
  }
  return result;
}

function snapTargets(layout: WindowLayout, index: number, monitor: LayoutMonitor): { xs: number[]; ys: number[] } {
  const xs = [0, monitor.work_area.width];
  const ys = [0, monitor.work_area.height];
  layout.windows.forEach((slot, candidate) => {
    if (candidate === index || slot.fullscreen || slot.monitor_id !== monitor.id) return;
    const visible = visibleSize(slot, monitor);
    xs.push(slot.x, slot.x + visible.width);
    ys.push(slot.y, slot.y + visible.height);
  });
  return { xs, ys };
}

function monitorForRect(monitors: LayoutMonitor[], rect: LayoutRect): LayoutMonitor | undefined {
  const center = { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  return monitors.filter(monitor => monitor.work_area.width >= MIN_LAYOUT_WIDTH && monitor.work_area.height >= MIN_LAYOUT_HEIGHT).reduce<LayoutMonitor | undefined>((nearest, monitor) => {
    const distance = (candidate: LayoutMonitor) => {
      const work = candidate.work_area;
      return Math.hypot(Math.max(work.x - center.x, 0, center.x - work.x - work.width), Math.max(work.y - center.y, 0, center.y - work.y - work.height));
    };
    return !nearest || distance(monitor) < distance(nearest) ? monitor : nearest;
  }, undefined);
}

export function moveSlot(layout: WindowLayout, index: number, rect: LayoutRect, tolerance = 0): LayoutSlot {
  if (layout.windows[index].fullscreen) return layout.windows[index];
  const monitor = monitorForRect(layout.monitors, rect) ?? primaryMonitor(layout.monitors)!;
  let slot = constrainSlot({ ...layout.windows[index], ...rect, monitor_id: monitor.id, x: rect.x - monitor.work_area.x, y: rect.y - monitor.work_area.y }, monitor);
  const { xs, ys } = snapTargets(layout, index, monitor);
  const visible = visibleSize(slot, monitor);
  slot = { ...slot, x: slot.x + closestDelta([slot.x, slot.x + visible.width], xs, tolerance),
    y: slot.y + closestDelta([slot.y, slot.y + visible.height], ys, tolerance) };
  return constrainSlot(slot, monitor);
}

export function resizeSlot(layout: WindowLayout, index: number, start: LayoutSlot, dx: number, dy: number, handle: ResizeHandle, tolerance = 0): LayoutSlot {
  if (start.fullscreen) return start;
  const monitor = layout.monitors.find(monitor => monitor.id === start.monitor_id)!;
  const { xs, ys } = snapTargets(layout, index, monitor);
  const edge = visibleInsets(monitor);
  const extraWidth = edge.left + edge.right, extraHeight = edge.top + edge.bottom;
  const minWidth = MIN_LAYOUT_WIDTH + extraWidth, maxWidth = MAX_LAYOUT_WIDTH + extraWidth;
  const minHeight = MIN_LAYOUT_HEIGHT + extraHeight, maxHeight = MAX_LAYOUT_HEIGHT + extraHeight;
  let left = start.x, right = start.x + start.width + extraWidth, top = start.y, bottom = start.y + start.height + extraHeight;
  if (handle.includes("w")) left = clamp(left + dx, right - maxWidth, right - minWidth);
  if (handle.includes("e")) right = clamp(right + dx, left + minWidth, left + maxWidth);
  if (handle.includes("n")) top = clamp(top + dy, bottom - maxHeight, bottom - minHeight);
  if (handle.includes("s")) bottom = clamp(bottom + dy, top + minHeight, top + maxHeight);
  if (handle.includes("w")) left += closestDelta([left], xs, tolerance);
  if (handle.includes("e")) right += closestDelta([right], xs, tolerance);
  if (handle.includes("n")) top += closestDelta([top], ys, tolerance);
  if (handle.includes("s")) bottom += closestDelta([bottom], ys, tolerance);
  left = clamp(left, right - maxWidth, right - minWidth);
  right = clamp(right, left + minWidth, left + maxWidth);
  top = clamp(top, bottom - maxHeight, bottom - minHeight);
  bottom = clamp(bottom, top + minHeight, top + maxHeight);
  return constrainSlot({ ...start, monitor_id: monitor.id, x: left, y: top, width: right - left - extraWidth, height: bottom - top - extraHeight }, monitor);
}


export function validateLayout(layout: WindowLayout): string | null {
  if (!layout.name.trim() || layout.name.trim().length > 40) return "请输入布局名称（最多 40 字）";
  if (layout.windows.length < 1 || layout.windows.length > MAX_LAYOUT_WINDOWS) return "窗口数量须在 1 到 32 之间";
  const titles = new Set<string>();
  for (const slot of layout.windows) {
    const title = slot.window_title?.trim().toLowerCase();
    if (title && titles.has(title)) return "窗口名称不能重复";
    if (title) titles.add(title);
    const monitor = layout.monitors.find(monitor => monitor.id === slot.monitor_id);
    if (!monitor || ![slot.x, slot.y, slot.width, slot.height].every(Number.isInteger)) return "窗口坐标和尺寸必须为整数";
    if (slot.fullscreen) continue;
    if (slot.width < MIN_LAYOUT_WIDTH || slot.height < MIN_LAYOUT_HEIGHT || slot.width > MAX_LAYOUT_WIDTH || slot.height > MAX_LAYOUT_HEIGHT) return "游戏分辨率须在 800 × 600 到 7680 × 4320 之间";
    const visible = visibleSize(slot, monitor);
    if (slot.x < MIN_VISIBLE_PIXELS - visible.width || slot.y < MIN_VISIBLE_PIXELS - visible.height || slot.x > Math.max(0, monitor.work_area.width - visible.width) || slot.y > Math.max(0, monitor.work_area.height - visible.height)) return "窗口坐标超出显示器工作区";
  }
  return null;
}
