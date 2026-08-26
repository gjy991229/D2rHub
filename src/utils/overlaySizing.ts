export interface OverlaySize {
  width: number;
  height: number;
}

export type MiniOverlayLayout = "single" | "stacked";

export const MINI_OVERLAY_MIN_WIDTH = 200;
export const MINI_OVERLAY_SINGLE_MIN_HEIGHT = 18;
export const MINI_OVERLAY_STACKED_MIN_HEIGHT = MINI_OVERLAY_SINGLE_MIN_HEIGHT * 2;
export const MINI_OVERLAY_LAYOUT_THRESHOLD = MINI_OVERLAY_STACKED_MIN_HEIGHT;

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

export function miniOverlayMinHeightForLayout(layout: MiniOverlayLayout): number {
  return layout === "single"
    ? MINI_OVERLAY_SINGLE_MIN_HEIGHT
    : MINI_OVERLAY_STACKED_MIN_HEIGHT;
}

export function initialMiniOverlayLayout(
  height: number,
  layoutAtThreshold: MiniOverlayLayout = "single",
): MiniOverlayLayout {
  if (height < MINI_OVERLAY_LAYOUT_THRESHOLD) return "single";
  if (height > MINI_OVERLAY_LAYOUT_THRESHOLD) return "stacked";
  return layoutAtThreshold;
}

export function resolveMiniOverlayLayoutAfterResize(
  currentLayout: MiniOverlayLayout,
  previousHeight: number,
  nextHeight: number,
): MiniOverlayLayout {
  // Windows rounds physical track sizes to whole pixels. At 125%-175% DPI a
  // logical 36px minimum can therefore arrive as 36.x CSS pixels. The one-pixel
  // tolerance lets a downward resize cross into the single row on those screens.
  const dpiRoundingTolerance = 1;
  if (
    nextHeight < previousHeight
    && nextHeight <= MINI_OVERLAY_LAYOUT_THRESHOLD + dpiRoundingTolerance
  ) {
    return "single";
  }
  if (
    nextHeight > previousHeight
    && nextHeight >= MINI_OVERLAY_LAYOUT_THRESHOLD
  ) {
    return "stacked";
  }
  return currentLayout;
}
