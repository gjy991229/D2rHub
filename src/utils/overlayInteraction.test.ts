const g = globalThis as any;
const fs = g.process.getBuiltinModule("fs");
const path = g.process.getBuiltinModule("path");

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

export function runTests() {
  const overlayPath = path.join(g.process.cwd(), "src", "pages", "Overlay.tsx");
  const statsPath = path.join(g.process.cwd(), "src", "store", "stats.ts");
  const sizingPath = path.join(g.process.cwd(), "src", "utils", "overlaySizing.ts");
  const overlaySource = (fs.readFileSync(overlayPath, "utf8") as string).replace(/\r\n/g, "\n");
  const statsSource = (fs.readFileSync(statsPath, "utf8") as string).replace(/\r\n/g, "\n");
  const sizingSource = (fs.readFileSync(sizingPath, "utf8") as string).replace(/\r\n/g, "\n");

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
    overlaySource.includes('data-overlay-account-scroll="true"')
      && overlaySource.includes('overlayWindowLabel === "stats-overlay"'),
    "statistics overlay owns the account strip and is selected by its native window label",
  );
  assert(
    overlaySource.includes('data-overlay-kind={isStatsOverlay ? "stats" : "tz"}')
      && overlaySource.includes("onDoubleClickCapture={!isStatsOverlay ? handleTzOverlayDoubleClick : undefined}")
      && overlaySource.includes('window.addEventListener("keydown", handleTzOverlayWindowKeyDown, true)')
      && overlaySource.includes('if (event.key !== "Enter" || event.repeat) return;'),
    "TZ overlay toggles mini mode from any double-click or Enter while statistics does not",
  );
  assert(
    overlaySource.includes('className="tz-expanded-layout')
      && overlaySource.includes('className="tz-forecast-divider"')
      && !overlaySource.includes("terrorZoneExpanded")
      && !overlaySource.includes("toggleTerrorZoneDrawer"),
    "expanded TZ uses a direct forecast layout without a drawer or nested card interaction",
  );
  assert(
    !overlaySource.includes("data-mini-layout")
      && !sizingSource.includes("LAYOUT_THRESHOLD")
      && !sizingSource.includes("MiniOverlayLayout"),
    "mini TZ stays in one row without a height-based layout threshold",
  );
  assert(
    overlaySource.includes("if (isStatsOverlay) {")
      && overlaySource.includes('isStatsOverlay ? "load_stats_overlay_geometry" : "load_overlay_geometry"')
      && overlaySource.includes("await persistOverlayGeometry();\n      return;"),
    "statistics overlay has independent geometry and bypasses mini and edge-docking behavior",
  );
  assert(
    overlaySource.includes("onDoubleClick={handleTimerDoubleClick}")
      && overlaySource.includes("useStats.getState().finishRunAsTown()")
      && statsSource.includes("await get().finishRunAsTown(normalized)"),
    "timer double-click and audio-tracked town detection share the same run-finishing transition",
  );
  assert(
    overlaySource.includes("aggregateOverlayDrops(stats.currentDrops)")
      && overlaySource.includes("displayedDropGroups.map(({ key, drop, count, latestIndex })")
      && overlaySource.includes("stats.removeCurrentDrop(latestIndex)"),
    "statistics overlay groups repeated drops and removes the latest occurrence from a group",
  );
  assert(
    overlaySource.includes('aria-live="polite"')
      && overlaySource.includes("RECENT_DROP_NOTICE_LIMIT = 3")
      && overlaySource.includes("RECENT_DROP_NOTICE_DURATION_MS = 6000"),
    "drop feedback announces at most three recent detections and expires them promptly",
  );
  assert(
    overlaySource.includes("const isAudioTrackingActive = isStatsOverlay && !!config?.rune_audio_enabled")
      && overlaySource.includes("if (!isAudioTrackingActive) return;")
      && overlaySource.includes("(!isPollerActive && !isAudioTrackingActive)"),
    "audio tracking remains active when the statistics window is hidden",
  );
  assert(
    overlaySource.includes("stats.sessionRuns[stats.currentRunKey || stats.currentRunName || stats.currentScene]"),
    "session run totals use the stable audio-tracking run key before display-name fallbacks",
  );
  assert(
    overlaySource.includes("COLLAPSED_DROP_GROUP_LIMIT = 5")
      && overlaySource.includes("setShowAllDropGroups((current) => !current)")
      && overlaySource.includes("aria-expanded={showAllDropGroups}"),
    "drop groups stay compact by default and expose an accessible expansion control",
  );
  assert(
    overlaySource.includes('OVERLAY_MINI_SIZE_STORAGE_KEY = "d2rhub-information-overlay-mini-size"'),
    "mini mode owns a dedicated persisted size preference",
  );
  assert(
    overlaySource.includes('OVERLAY_MINI_SIZE_VERSION = "2"')
      && overlaySource.includes("return { ...stored, height: MINI_OVERLAY_MIN_HEIGHT }"),
    "legacy mini preferences migrate once to the new content-height layout while keeping width",
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
