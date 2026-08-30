import {
  calculateOverlayDockAnimationDuration,
  calculateOverlayDockPlacement,
  easeOverlayDockProgress,
  findOverlayDockEdge,
  OVERLAY_DOCK_REVEAL_SIZE,
  OVERLAY_DOCK_SNAP_DISTANCE,
} from "./overlayDocking";

function assertEqual(actual: unknown, expected: unknown, message: string) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`FAIL: ${message}\nexpected=${JSON.stringify(expected)}\nactual=${JSON.stringify(actual)}`);
  }
  console.log(`PASS: ${message}`);
}

export function runTests() {
  const workArea = { x: 0, y: 0, width: 2560, height: 1400 };
  const size = { width: 320, height: 180 };

  assertEqual(
    OVERLAY_DOCK_SNAP_DISTANCE,
    4.8,
    "overlay docking uses one fifth of the former 24-pixel snap distance",
  );
  assertEqual(
    findOverlayDockEdge({ x: 4.8, y: 300 }, size, workArea, OVERLAY_DOCK_SNAP_DISTANCE),
    "left",
    "nearest left edge is detected at the reduced snap-distance boundary",
  );
  assertEqual(
    findOverlayDockEdge({ x: 4.9, y: 300 }, size, workArea, OVERLAY_DOCK_SNAP_DISTANCE),
    null,
    "an overlay outside the reduced snap distance remains freely positioned",
  );
  assertEqual(
    findOverlayDockEdge({ x: 900, y: 700 }, size, workArea, 24),
    null,
    "a freely positioned overlay is not docked",
  );
  assertEqual(
    calculateOverlayDockPlacement("right", { x: 2230, y: 1300 }, size, workArea, OVERLAY_DOCK_REVEAL_SIZE),
    {
      edge: "right",
      visible: { x: 2240, y: 1220 },
      hidden: { x: 2552, y: 1220 },
    },
    "right docking clamps the overlay and leaves an eight-pixel reveal strip",
  );
  assertEqual(
    calculateOverlayDockPlacement("top", { x: -40, y: 2 }, size, workArea, OVERLAY_DOCK_REVEAL_SIZE),
    {
      edge: "top",
      visible: { x: 0, y: 0 },
      hidden: { x: 0, y: -172 },
    },
    "top docking clamps the perpendicular axis and hides by its height",
  );
  assertEqual(
    calculateOverlayDockPlacement("left", { x: 3, y: 420 }, size, workArea, OVERLAY_DOCK_REVEAL_SIZE),
    {
      edge: "left",
      visible: { x: 0, y: 420 },
      hidden: { x: -312, y: 420 },
    },
    "left docking leaves its reveal strip on the screen",
  );
  assertEqual(
    calculateOverlayDockPlacement("bottom", { x: 700, y: 1214 }, size, workArea, OVERLAY_DOCK_REVEAL_SIZE),
    {
      edge: "bottom",
      visible: { x: 700, y: 1220 },
      hidden: { x: 700, y: 1392 },
    },
    "bottom docking leaves its reveal strip above the work-area edge",
  );
  assertEqual(
    {
      shortReveal: calculateOverlayDockAnimationDuration(10, "reveal"),
      fullReveal: calculateOverlayDockAnimationDuration(320, "reveal"),
      shortHide: calculateOverlayDockAnimationDuration(10, "hide"),
      fullHide: calculateOverlayDockAnimationDuration(320, "hide"),
    },
    { shortReveal: 320, fullReveal: 460, shortHide: 280, fullHide: 398 },
    "dock motion duration scales with travel while reveal remains more deliberate than hide",
  );
  assertEqual(
    {
      revealStart: easeOverlayDockProgress(0, "reveal"),
      revealMiddle: easeOverlayDockProgress(0.5, "reveal"),
      revealEnd: easeOverlayDockProgress(1, "reveal"),
      hideStart: easeOverlayDockProgress(0, "hide"),
      hideMiddle: easeOverlayDockProgress(0.5, "hide"),
      hideEnd: easeOverlayDockProgress(1, "hide"),
    },
    {
      revealStart: 0,
      revealMiddle: 0.875,
      revealEnd: 1,
      hideStart: 0,
      hideMiddle: 0.5,
      hideEnd: 1,
    },
    "reveal decelerates without the old quint snap while hide eases smoothly at both ends",
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
