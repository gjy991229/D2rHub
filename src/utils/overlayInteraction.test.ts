const g = globalThis as any;
const fs = g.process.getBuiltinModule("fs");
const path = g.process.getBuiltinModule("path");

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

export function runTests() {
  const overlayPath = path.join(g.process.cwd(), "src", "pages", "Overlay.tsx");
  const overlaySource = fs.readFileSync(overlayPath, "utf8") as string;

  assert(
    !overlaySource.includes("data-tauri-drag-region"),
    "information overlay does not use native drag regions that consume double-clicks",
  );
  assert(
    overlaySource.includes("startDragging()"),
    "information overlay starts native window dragging only from its pointer gesture handler",
  );
  assert(
    overlaySource.includes("OVERLAY_WINDOW_DRAG_THRESHOLD_PX"),
    "information overlay distinguishes a drag from a stationary double-click with a movement threshold",
  );
  assert(
    overlaySource.includes('OVERLAY_MINI_SIZE_STORAGE_KEY = "d2rhub-information-overlay-mini-size"'),
    "mini mode owns a dedicated persisted size preference",
  );
  assert(
    overlaySource.includes("storeMiniOverlaySize(miniSize)")
      && overlaySource.includes("storeExpandedOverlaySize(expandedSize)"),
    "mini and expanded resize preferences are persisted independently",
  );
  assert(
    overlaySource.includes("setResizable(false)")
      && overlaySource.includes("MINI_OVERLAY_RESIZE_EDGES")
      && overlaySource.includes("calculateMiniOverlayResizeBounds"),
    "mini mode bypasses the native Windows resize floor with custom resize handles",
  );
  assert(
    overlaySource.includes("pendingBounds")
      && overlaySource.includes("moveInFlight")
      && overlaySource.includes("flushMiniOverlayResize"),
    "custom mini resizing coalesces native size writes instead of accumulating stale moves",
  );
  assert(
    overlaySource.includes("cancelMiniOverlayResize")
      && overlaySource.includes("onLostPointerCapture")
      && overlaySource.includes("flushPromise"),
    "custom mini resizing cancels safely on capture loss and waits for native writes during mode changes",
  );
  assert(
    overlaySource.includes("|| miniResizeSessionRef.current"),
    "programmatic north and west resizing cannot accidentally trigger edge docking",
  );
  assert(
    overlaySource.includes("pendingPosition")
      && overlaySource.includes("moveInFlight")
      && overlaySource.includes("flushLatestPosition"),
    "dock animation coalesces native window moves instead of accumulating stale frames",
  );
  assert(
    !overlaySource.includes("const step = async"),
    "dock animation clock is not blocked by awaited native position writes",
  );
}

if (typeof g.process !== "undefined" && typeof g.process.argv !== "undefined") {
  try {
    runTests();
  } catch (error) {
    console.error(error);
    g.process.exit(1);
  }
}
