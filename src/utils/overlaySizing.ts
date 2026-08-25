export interface OverlaySize {
  width: number;
  height: number;
}

export type MiniOverlayLayout = "single" | "stacked";

export const MINI_OVERLAY_MIN_WIDTH = 200;
export const MINI_OVERLAY_MIN_HEIGHT = 18;
export const MINI_OVERLAY_STACKED_HEIGHT = MINI_OVERLAY_MIN_HEIGHT * 2;

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
  layoutAtThreshold: MiniOverlayLayout = "single",
): MiniOverlayLayout {
  if (height < MINI_OVERLAY_STACKED_HEIGHT) return "single";
  if (height > MINI_OVERLAY_STACKED_HEIGHT) return "stacked";
  return layoutAtThreshold;
}

export function resolveMiniOverlayLayoutAfterResize(
  currentLayout: MiniOverlayLayout,
  previousHeight: number,
  nextHeight: number,
): MiniOverlayLayout {
  if (nextHeight < previousHeight && nextHeight <= MINI_OVERLAY_STACKED_HEIGHT) {
    return "single";
  }
  if (nextHeight > previousHeight && nextHeight >= MINI_OVERLAY_STACKED_HEIGHT) {
    return "stacked";
  }
  return currentLayout;
}
