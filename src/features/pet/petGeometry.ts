import type { PetFrame } from "./types";

// Original SVGs have different viewBoxes. Preserve their native coordinate
// units instead of fitting each image into the same box (which adds padding).
export const PET_CANVAS = { width: 208, height: 135 };
export const PET_FRAME_GEOMETRY: Record<PetFrame, {
  width: number; height: number; eyes: readonly [number, number, number, number];
}> = {
  up: { width: 208, height: 133, eyes: [74.4, 77.4, 122.8, 81.8] },
  left: { width: 195, height: 133, eyes: [71.7, 68.8, 120.2, 73.2] },
  right: { width: 201, height: 135, eyes: [70.8, 72.2, 119.2, 76.6] },
};
