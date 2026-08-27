import {
  calculateMiniOverlayResizeBounds,
  calculateMiniOverlaySize,
  MINI_OVERLAY_MIN_HEIGHT,
  MINI_OVERLAY_MIN_WIDTH,
  normalizeMiniOverlaySize,
} from "./overlaySizing";

function assertEqual(actual: unknown, expected: unknown, message: string) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`FAIL: ${message}\nexpected=${JSON.stringify(expected)}\nactual=${JSON.stringify(actual)}`);
  }
  console.log(`PASS: ${message}`);
}

export function runTests() {
  assertEqual(
    calculateMiniOverlaySize({ width: 1920, height: 1040 }),
    { width: 346, height: 28 },
    "mini defaults to one content-height row",
  );
  assertEqual(
    calculateMiniOverlaySize({ width: 2560, height: 1400 }),
    { width: 461, height: 28 },
    "mini width scales while its content-height minimum remains stable",
  );
  assertEqual(MINI_OVERLAY_MIN_WIDTH, 220, "mini mode keeps a practical horizontal minimum");
  assertEqual(MINI_OVERLAY_MIN_HEIGHT, 28, "mini mode keeps its text fully visible");
  assertEqual(
    normalizeMiniOverlaySize({ width: 120, height: 4 }),
    { width: 220, height: 28 },
    "mini size clamps directly to its content and width minimums",
  );
  assertEqual(
    calculateMiniOverlayResizeBounds({ width: 240, height: 48 }, "s", 0, -10),
    { width: 240, height: 38, offsetX: 0, offsetY: 0 },
    "custom south resizing remains continuous without a layout threshold",
  );
  assertEqual(
    calculateMiniOverlayResizeBounds({ width: 240, height: 48 }, "s", 0, -30),
    { width: 240, height: 28, offsetX: 0, offsetY: 0 },
    "custom south resizing clamps at the content-height minimum",
  );
  assertEqual(
    calculateMiniOverlayResizeBounds({ width: 240, height: 39 }, "n", 0, 20),
    { width: 240, height: 28, offsetX: 0, offsetY: 11 },
    "custom north resizing keeps the bottom edge anchored",
  );
  assertEqual(
    calculateMiniOverlayResizeBounds({ width: 240, height: 39 }, "w", 50, 0),
    { width: 220, height: 39, offsetX: 20, offsetY: 0 },
    "custom west resizing clamps width and keeps the right edge anchored",
  );
  assertEqual(
    calculateMiniOverlayResizeBounds({ width: 240, height: 39 }, "se", 30, 5),
    { width: 270, height: 44, offsetX: 0, offsetY: 0 },
    "custom corner resizing updates both dimensions",
  );
}

const g = globalThis as any;
if (typeof g.process !== "undefined" && typeof g.process.argv !== "undefined") {
  try {
    runTests();
  } catch (error) {
    console.error(error);
    g.process.exit(1);
  }
}
