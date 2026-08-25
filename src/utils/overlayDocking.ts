export type OverlayDockEdge = "left" | "right" | "top" | "bottom";

export interface PhysicalPoint {
  x: number;
  y: number;
}

export interface PhysicalSize {
  width: number;
  height: number;
}

export interface PhysicalRect extends PhysicalPoint, PhysicalSize {}

export interface OverlayDockPlacement {
  edge: OverlayDockEdge;
  visible: PhysicalPoint;
  hidden: PhysicalPoint;
}

export const OVERLAY_DOCK_SNAP_DISTANCE = 24;
export const OVERLAY_DOCK_REVEAL_SIZE = 8;

export type OverlayDockMotion = "reveal" | "hide";

const OVERLAY_DOCK_REVEAL_MIN_MS = 320;
const OVERLAY_DOCK_REVEAL_MAX_MS = 480;
const OVERLAY_DOCK_HIDE_MIN_MS = 280;
const OVERLAY_DOCK_HIDE_MAX_MS = 410;

function clamp(value: number, minimum: number, maximum: number) {
  if (maximum < minimum) return minimum;
  return Math.min(maximum, Math.max(minimum, value));
}

export function calculateOverlayDockAnimationDuration(
  distance: number,
  motion: OverlayDockMotion,
): number {
  const safeDistance = Math.max(0, Number.isFinite(distance) ? distance : 0);
  if (motion === "hide") {
    return Math.round(clamp(270 + safeDistance * 0.4, OVERLAY_DOCK_HIDE_MIN_MS, OVERLAY_DOCK_HIDE_MAX_MS));
  }
  return Math.round(clamp(300 + safeDistance * 0.5, OVERLAY_DOCK_REVEAL_MIN_MS, OVERLAY_DOCK_REVEAL_MAX_MS));
}

export function easeOverlayDockProgress(
  progress: number,
  motion: OverlayDockMotion,
): number {
  const t = clamp(progress, 0, 1);
  if (motion === "hide") {
    return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
  }
  return 1 - Math.pow(1 - t, 3);
}

export function findOverlayDockEdge(
  position: PhysicalPoint,
  size: PhysicalSize,
  workArea: PhysicalRect,
  snapDistance: number,
): OverlayDockEdge | null {
  const right = workArea.x + workArea.width;
  const bottom = workArea.y + workArea.height;
  const candidates: Array<{ edge: OverlayDockEdge; distance: number }> = [
    { edge: "left", distance: Math.abs(position.x - workArea.x) },
    { edge: "right", distance: Math.abs(position.x + size.width - right) },
    { edge: "top", distance: Math.abs(position.y - workArea.y) },
    { edge: "bottom", distance: Math.abs(position.y + size.height - bottom) },
  ];

  candidates.sort((a, b) => a.distance - b.distance);
  return candidates[0].distance <= snapDistance ? candidates[0].edge : null;
}

export function calculateOverlayDockPlacement(
  edge: OverlayDockEdge,
  position: PhysicalPoint,
  size: PhysicalSize,
  workArea: PhysicalRect,
  revealSize: number,
): OverlayDockPlacement {
  const right = workArea.x + workArea.width;
  const bottom = workArea.y + workArea.height;
  const visible: PhysicalPoint = {
    x: clamp(position.x, workArea.x, right - size.width),
    y: clamp(position.y, workArea.y, bottom - size.height),
  };

  if (edge === "left") visible.x = workArea.x;
  if (edge === "right") visible.x = right - size.width;
  if (edge === "top") visible.y = workArea.y;
  if (edge === "bottom") visible.y = bottom - size.height;

  const revealed = Math.max(1, Math.min(revealSize, edge === "left" || edge === "right" ? size.width : size.height));
  const hidden = { ...visible };
  if (edge === "left") hidden.x = workArea.x - size.width + revealed;
  if (edge === "right") hidden.x = right - revealed;
  if (edge === "top") hidden.y = workArea.y - size.height + revealed;
  if (edge === "bottom") hidden.y = bottom - revealed;

  return { edge, visible, hidden };
}

export function pointDistance(a: PhysicalPoint, b: PhysicalPoint): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}
