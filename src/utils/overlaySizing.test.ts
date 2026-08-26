import {
  calculateMiniOverlayResizeBounds,
  calculateMiniOverlaySize,
  initialMiniOverlayLayout,
  MINI_OVERLAY_SINGLE_MIN_HEIGHT,
  MINI_OVERLAY_STACKED_HEIGHT,
  normalizeMiniOverlaySize,
  resolveMiniOverlayLayoutAfterResize,
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
    { width: 346, height: 83 },
    "mini size matches the requested compact two-row proportions",
  );
  assertEqual(
    calculateMiniOverlaySize({ width: 2560, height: 1400 }),
    { width: 461, height: 112 },
    "mini size scales proportionally on a larger logical work area",
  );
  assertEqual(
    MINI_OVERLAY_STACKED_HEIGHT,
    40,
    "the layout threshold clears the Windows native resize floor",
  );
  assertEqual(MINI_OVERLAY_SINGLE_MIN_HEIGHT, 20, "single-row mode owns the 20px minimum");
  assertEqual(
    normalizeMiniOverlaySize({ width: 120, height: 4 }),
    { width: 200, height: 20 },
    "mini size respects the 20px single-row minimum",
  );
  assertEqual(
    initialMiniOverlayLayout(MINI_OVERLAY_STACKED_HEIGHT),
    "single",
    "a restored mini window at 40px starts in one row",
  );
  assertEqual(
    initialMiniOverlayLayout(MINI_OVERLAY_STACKED_HEIGHT + 1),
    "stacked",
    "a restored mini window above the threshold starts in two rows",
  );
  assertEqual(
    resolveMiniOverlayLayoutAfterResize(39),
    "single",
    "a height below 40px uses one row",
  );
  assertEqual(
    resolveMiniOverlayLayoutAfterResize(40),
    "single",
    "exactly 40px uses one row without changing height",
  );
  assertEqual(
    resolveMiniOverlayLayoutAfterResize(40.01),
    "stacked",
    "any height above 40px uses two rows",
  );
  assertEqual(
    calculateMiniOverlayResizeBounds({ width: 240, height: 39 }, "s", 0, -10),
    { width: 240, height: 29, offsetX: 0, offsetY: 0 },
    "custom south resizing can move continuously below the native 39px floor",
  );
  assertEqual(
    calculateMiniOverlayResizeBounds({ width: 240, height: 39 }, "s", 0, -30),
    { width: 240, height: 20, offsetX: 0, offsetY: 0 },
    "custom south resizing clamps at the 20px minimum",
  );
  assertEqual(
    calculateMiniOverlayResizeBounds({ width: 240, height: 39 }, "n", 0, 9),
    { width: 240, height: 30, offsetX: 0, offsetY: 9 },
    "custom north resizing keeps the bottom edge anchored",
  );
  assertEqual(
    calculateMiniOverlayResizeBounds({ width: 240, height: 39 }, "w", 50, 0),
    { width: 200, height: 39, offsetX: 40, offsetY: 0 },
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
