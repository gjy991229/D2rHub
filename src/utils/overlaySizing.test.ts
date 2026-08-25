import {
  calculateMiniOverlaySize,
  initialMiniOverlayLayout,
  MINI_OVERLAY_MIN_HEIGHT,
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
    MINI_OVERLAY_MIN_HEIGHT * 2,
    "the two-row threshold is exactly twice the new minimum height",
  );
  assertEqual(
    normalizeMiniOverlaySize({ width: 120, height: 4 }),
    { width: 200, height: 18 },
    "mini size respects the new half-height minimum",
  );
  assertEqual(
    initialMiniOverlayLayout(MINI_OVERLAY_STACKED_HEIGHT),
    "single",
    "a restored mini window at the old minimum starts in one row",
  );
  assertEqual(
    initialMiniOverlayLayout(MINI_OVERLAY_STACKED_HEIGHT + 1),
    "stacked",
    "a restored mini window above the threshold starts in two rows",
  );
  assertEqual(
    initialMiniOverlayLayout(MINI_OVERLAY_STACKED_HEIGHT, "stacked"),
    "stacked",
    "a persisted upward resize preserves the two-row threshold state",
  );
  assertEqual(
    resolveMiniOverlayLayoutAfterResize("stacked", 37, 36),
    "single",
    "dragging down to the old minimum switches to one row",
  );
  assertEqual(
    resolveMiniOverlayLayoutAfterResize("single", 36, 36),
    "single",
    "duplicate resize events at the threshold do not flicker",
  );
  assertEqual(
    resolveMiniOverlayLayoutAfterResize("single", 18, 36),
    "stacked",
    "dragging up to twice the new minimum restores two rows",
  );
  assertEqual(
    resolveMiniOverlayLayoutAfterResize("stacked", 36, 36),
    "stacked",
    "duplicate two-row resize events at the threshold remain stable",
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
