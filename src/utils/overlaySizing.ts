export interface OverlaySize {
  width: number;
  height: number;
}

export type MiniOverlayLayout = "single" | "stacked";
export type MiniOverlayResizeEdge = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

export interface MiniOverlayResizeBounds extends OverlaySize {
  offsetX: number;
  offsetY: number;
}

export const MINI_OVERLAY_MIN_WIDTH = 200;
export const MINI_OVERLAY_SINGLE_MIN_HEIGHT = 20;
export const MINI_OVERLAY_LAYOUT_THRESHOLD = 40;

// Kept as compatibility aliases for the window configuration and older callers.
export const MINI_OVERLAY_MIN_HEIGHT = MINI_OVERLAY_SINGLE_MIN_HEIGHT;
export const MINI_OVERLAY_STACKED_HEIGHT = MINI_OVERLAY_LAYOUT_THRESHOLD;

const MINI_OVERLAY_WIDTH_RATIO = 0.18;
const MINI_OVERLAY_HEIGHT_RATIO = 0.08;

export function calculateMiniOverlaySize(workArea: OverlaySize): OverlaySize {
  return {
    width: Math.max(1, Math.round(workArea.width * MINI_OVERLAY_WIDTH_RATIO)),
    height: Math.max(1, Math.round(workArea.height * MINI_OVERLAY_HEIGHT_RATIO)),
  };
}

export function normalizeMiniOverlaySize(size: OverlaySize): OverlaySize {
  return {
    width: Math.max(MINI_OVERLAY_MIN_WIDTH, Math.round(size.width)),
    height: Math.max(MINI_OVERLAY_MIN_HEIGHT, Math.round(size.height)),
  };
}

export function initialMiniOverlayLayout(
  height: number,
): MiniOverlayLayout {
  return height <= MINI_OVERLAY_LAYOUT_THRESHOLD ? "single" : "stacked";
}

export function resolveMiniOverlayLayoutAfterResize(
  nextHeight: number,
): MiniOverlayLayout {
  return initialMiniOverlayLayout(nextHeight);
}

export function calculateMiniOverlayResizeBounds(
  startSize: OverlaySize,
  edge: MiniOverlayResizeEdge,
  deltaX: number,
  deltaY: number,
): MiniOverlayResizeBounds {
  const resizeWest = edge.includes("w");
  const resizeEast = edge.includes("e");
  const resizeNorth = edge.includes("n");
  const resizeSouth = edge.includes("s");

  const requestedWidth = resizeWest
    ? startSize.width - deltaX
    : resizeEast
      ? startSize.width + deltaX
      : startSize.width;
  const requestedHeight = resizeNorth
    ? startSize.height - deltaY
    : resizeSouth
      ? startSize.height + deltaY
      : startSize.height;
  const normalized = normalizeMiniOverlaySize({
    width: requestedWidth,
    height: requestedHeight,
  });

  return {
    ...normalized,
    offsetX: resizeWest ? startSize.width - normalized.width : 0,
    offsetY: resizeNorth ? startSize.height - normalized.height : 0,
  };
}
