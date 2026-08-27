export interface OverlaySize {
  width: number;
  height: number;
}

export type MiniOverlayResizeEdge = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

export interface MiniOverlayResizeBounds extends OverlaySize {
  offsetX: number;
  offsetY: number;
}

export const MINI_OVERLAY_MIN_WIDTH = 220;
export const MINI_OVERLAY_MIN_HEIGHT = 28;

const MINI_OVERLAY_WIDTH_RATIO = 0.18;

export function calculateMiniOverlaySize(workArea: OverlaySize): OverlaySize {
  return {
    width: Math.max(MINI_OVERLAY_MIN_WIDTH, Math.round(workArea.width * MINI_OVERLAY_WIDTH_RATIO)),
    height: MINI_OVERLAY_MIN_HEIGHT,
  };
}

export function normalizeMiniOverlaySize(size: OverlaySize): OverlaySize {
  return {
    width: Math.max(MINI_OVERLAY_MIN_WIDTH, Math.round(size.width)),
    height: Math.max(MINI_OVERLAY_MIN_HEIGHT, Math.round(size.height)),
  };
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
