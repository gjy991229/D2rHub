import React, { useEffect, useState, useRef } from "react";
import { ChevronLeft, ChevronRight, Eye } from "lucide-react";
import { useAccounts } from "../store/accounts";
import { useTheme, syncThemeFromConfig } from "../store/theme";
import { useGlobalConfig, initConfigSync } from "../store/globalConfig";
import { useStats, isHighRune, getSessionRunKey, MANUAL_FINISH_SCENE } from "../store/stats";
import { invokeCommand, listenEvent } from "../platform/tauri";
import type { ItemAudioEvent, RuneAudioEvent, TrackingSnapshot } from "../store/types";
import {
  aggregateOverlayDrops,
  getAppendedOverlayDrops,
  getOverlayDropLabel,
} from "../utils/overlayDrops";
import {
  currentMonitor,
  getCurrentWindow,
  PhysicalPosition,
  LogicalSize,
} from "@tauri-apps/api/window";
import { surfaceOpacityVars } from "../styles/surfaceOpacity";
import { isEnglishLanguage } from "../i18n";
import { translateTerrorZoneAreaName } from "../data/terrorZoneAreaNames";
import {
  calculateMiniOverlaySize,
  calculateMiniOverlayResizeBounds,
  MINI_OVERLAY_MIN_HEIGHT,
  MINI_OVERLAY_MIN_WIDTH,
  normalizeMiniOverlaySize,
  type MiniOverlayResizeBounds,
  type MiniOverlayResizeEdge,
  type OverlaySize,
} from "../utils/overlaySizing";
import {
  calculateOverlayDockAnimationDuration,
  calculateOverlayDockPlacement,
  easeOverlayDockProgress,
  findOverlayDockEdge,
  OVERLAY_DOCK_REVEAL_SIZE,
  OVERLAY_DOCK_SNAP_DISTANCE,
  pointDistance,
  type OverlayDockEdge,
  type OverlayDockPlacement,
  type PhysicalPoint,
  type PhysicalRect,
  type PhysicalSize,
} from "../utils/overlayDocking";
import { restoreWindowPlacement } from "../utils/windowPlacement";

function overlayLogDetail(value: unknown): string {
  if (value instanceof Error) return value.stack || value.message;
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function reportOverlayIssue(
  level: "WARN" | "ERROR",
  message: string,
  detail?: unknown,
) {
  const fullMessage = detail === undefined
    ? `[Overlay] ${message}`
    : `[Overlay] ${message}: ${overlayLogDetail(detail)}`;
  if (level === "ERROR") console.error(fullMessage);
  else console.warn(fullMessage);
  void invokeCommand("write_log", { level, message: fullMessage }).catch(() => {});
}

interface TerrorZoneImmunity {
  code: string;
  label: string;
  color: string;
}

interface TerrorZoneForecast {
  start_time: number;
  end_time: number;
  display_time: string;
  location_name: string;
  location_detail: string;
  tier_exp: string;
  tier_loot: string;
  immunities: TerrorZoneImmunity[];
}

interface TerrorZoneSnapshot {
  current: TerrorZoneForecast | null;
  next: TerrorZoneForecast | null;
}

type TerrorZoneStatus = "loading" | "ready" | "empty" | "error";
type OverlayDisplayMode = "mini" | "expanded";
type DropScope = "current" | "previous" | "overview";

const OVERLAY_MODE_STORAGE_KEY = "d2rhub-information-overlay-mode";
const OVERLAY_MINI_SIZE_STORAGE_KEY = "d2rhub-information-overlay-mini-size";
const OVERLAY_MINI_SIZE_VERSION_STORAGE_KEY = "d2rhub-information-overlay-mini-size-version";
const OVERLAY_MINI_SIZE_VERSION = "2";
const OVERLAY_EXPANDED_SIZE_STORAGE_KEY = "d2rhub-information-overlay-expanded-size";
const STATS_OVERLAY_MODE_STORAGE_KEY = "d2rhub-statistics-overlay-mode";
const STATS_OVERLAY_MINI_SIZE_STORAGE_KEY = "d2rhub-statistics-overlay-mini-size";
const STATS_OVERLAY_EXPANDED_SIZE_STORAGE_KEY = "d2rhub-statistics-overlay-expanded-size";
const EXPANDED_OVERLAY_MIN_WIDTH = 200;
const EXPANDED_OVERLAY_MIN_HEIGHT = 180;
const DEFAULT_EXPANDED_OVERLAY_SIZE: OverlaySize = { width: 280, height: 250 };
const STATS_EXPANDED_OVERLAY_MIN_WIDTH = 220;
const STATS_EXPANDED_OVERLAY_MIN_HEIGHT = 180;
const DEFAULT_STATS_EXPANDED_OVERLAY_SIZE: OverlaySize = { width: 280, height: 300 };
const STATS_MINI_OVERLAY_MIN_WIDTH = 240;
const STATS_MINI_OVERLAY_MIN_HEIGHT = 48;
const STATS_MINI_OVERLAY_RESIZE_INSET = 6;
const DEFAULT_STATS_MINI_OVERLAY_SIZE: OverlaySize = { width: 320, height: 48 };
const OVERLAY_WINDOW_DRAG_THRESHOLD_PX = 4;
const OVERLAY_DOCK_SETTLE_DELAY_MS = 260;
const OVERLAY_DOCK_HIDE_DELAY_MS = 420;
const COLLAPSED_DROP_GROUP_LIMIT = 5;
const RECENT_DROP_HIGHLIGHT_DURATION_MS = 1600;

type OverlayDockPhase = "shown" | "hidden" | "moving";

interface OverlayDockState {
  placement: OverlayDockPlacement;
  workArea: PhysicalRect;
  phase: OverlayDockPhase;
}

interface RecentDropHighlight {
  id: number;
  key: string;
  expiresAt: number;
}

interface MiniOverlayResizeSession {
  active: boolean;
  cancelled: boolean;
  pointerId: number;
  edge: MiniOverlayResizeEdge;
  startScreenX: number;
  startScreenY: number;
  startSize: OverlaySize;
  startLeftPhysical: number;
  startTopPhysical: number;
  scaleFactor: number;
  pendingBounds: MiniOverlayResizeBounds | null;
  moveInFlight: boolean;
  flushPromise: Promise<void> | null;
}

const MINI_OVERLAY_RESIZE_EDGES: MiniOverlayResizeEdge[] = [
  "n", "s", "e", "w", "ne", "nw", "se", "sw",
];

function readStoredOverlayMode(storageKey: string): OverlayDisplayMode {
  try {
    return localStorage.getItem(storageKey) === "mini" ? "mini" : "expanded";
  } catch {
    return "expanded";
  }
}

function storeOverlayMode(storageKey: string, mode: OverlayDisplayMode) {
  try {
    localStorage.setItem(storageKey, mode);
  } catch {}
}

function normalizeExpandedOverlaySize(size: OverlaySize): OverlaySize {
  return {
    width: Math.max(EXPANDED_OVERLAY_MIN_WIDTH, Math.round(size.width)),
    height: Math.max(EXPANDED_OVERLAY_MIN_HEIGHT, Math.round(size.height)),
  };
}

function normalizeStatsExpandedOverlaySize(size: OverlaySize): OverlaySize {
  return {
    width: Math.max(STATS_EXPANDED_OVERLAY_MIN_WIDTH, Math.round(size.width)),
    height: Math.max(STATS_EXPANDED_OVERLAY_MIN_HEIGHT, Math.round(size.height)),
  };
}

function normalizeStatsMiniOverlaySize(size: OverlaySize): OverlaySize {
  return {
    width: Math.max(STATS_MINI_OVERLAY_MIN_WIDTH, Math.round(size.width)),
    height: Math.max(STATS_MINI_OVERLAY_MIN_HEIGHT, Math.round(size.height)),
  };
}

function readStoredOverlaySize(
  storageKey: string,
  normalize: (size: OverlaySize) => OverlaySize,
): OverlaySize | null {
  try {
    const parsed = JSON.parse(localStorage.getItem(storageKey) || "null");
    if (!parsed || !Number.isFinite(parsed.width) || !Number.isFinite(parsed.height)) return null;
    return normalize(parsed);
  } catch {
    return null;
  }
}

function readStoredMiniOverlaySize(): OverlaySize | null {
  const stored = readStoredOverlaySize(OVERLAY_MINI_SIZE_STORAGE_KEY, normalizeMiniOverlaySize);
  if (!stored) return null;

  try {
    if (localStorage.getItem(OVERLAY_MINI_SIZE_VERSION_STORAGE_KEY) !== OVERLAY_MINI_SIZE_VERSION) {
      return { ...stored, height: MINI_OVERLAY_MIN_HEIGHT };
    }
  } catch {}
  return stored;
}

function readStoredExpandedOverlaySize(): OverlaySize | null {
  return readStoredOverlaySize(OVERLAY_EXPANDED_SIZE_STORAGE_KEY, normalizeExpandedOverlaySize);
}

function readStoredStatsMiniOverlaySize(): OverlaySize | null {
  return readStoredOverlaySize(
    STATS_OVERLAY_MINI_SIZE_STORAGE_KEY,
    normalizeStatsMiniOverlaySize,
  );
}

function storeOverlaySize(storageKey: string, size: OverlaySize) {
  try {
    localStorage.setItem(storageKey, JSON.stringify(size));
  } catch {}
}

function storeMiniOverlaySize(size: OverlaySize) {
  storeOverlaySize(OVERLAY_MINI_SIZE_STORAGE_KEY, size);
  try {
    localStorage.setItem(OVERLAY_MINI_SIZE_VERSION_STORAGE_KEY, OVERLAY_MINI_SIZE_VERSION);
  } catch {}
}

function storeExpandedOverlaySize(size: OverlaySize) {
  storeOverlaySize(OVERLAY_EXPANDED_SIZE_STORAGE_KEY, size);
}

async function resolveDefaultMiniOverlaySize(): Promise<OverlaySize> {
  const monitor = await currentMonitor();
  if (monitor?.workArea?.size && monitor.scaleFactor > 0) {
    return calculateMiniOverlaySize({
      width: monitor.workArea.size.width / monitor.scaleFactor,
      height: monitor.workArea.size.height / monitor.scaleFactor,
    });
  }

  return calculateMiniOverlaySize({
    width: window.screen.availWidth || window.innerWidth,
    height: window.screen.availHeight || window.innerHeight,
  });
}

async function applyMiniOverlaySize(
  win: ReturnType<typeof getCurrentWindow>,
  miniSize: OverlaySize,
) {
  const logicalSize = new LogicalSize(miniSize.width, miniSize.height);

  // Native Windows resizing stops an undecorated window near 39px. Mini mode
  // uses in-app resize handles so its content-height minimum remains reachable.
  await win.setResizable(false);
  await win.setMinSize(null);
  await win.setMaxSize(null);
  await win.setSize(logicalSize);
  await win.setMinSize(
    new LogicalSize(MINI_OVERLAY_MIN_WIDTH, MINI_OVERLAY_MIN_HEIGHT),
  );
}

async function applyExpandedOverlaySize(
  win: ReturnType<typeof getCurrentWindow>,
  expandedSize: OverlaySize,
) {
  await win.setResizable(true);
  await win.setMinSize(null);
  await win.setMaxSize(null);
  await win.setSize(new LogicalSize(expandedSize.width, expandedSize.height));
  await win.setMinSize(
    new LogicalSize(EXPANDED_OVERLAY_MIN_WIDTH, EXPANDED_OVERLAY_MIN_HEIGHT),
  );
}

async function applyStatsMiniOverlaySize(
  win: ReturnType<typeof getCurrentWindow>,
  miniSize: OverlaySize,
) {
  await win.setResizable(false);
  await win.setMinSize(null);
  await win.setMaxSize(null);
  await win.setSize(new LogicalSize(miniSize.width, miniSize.height));
  await win.setMinSize(
    new LogicalSize(STATS_MINI_OVERLAY_MIN_WIDTH, STATS_MINI_OVERLAY_MIN_HEIGHT),
  );
}

async function applyStatsExpandedOverlaySize(
  win: ReturnType<typeof getCurrentWindow>,
  expandedSize: OverlaySize,
) {
  await win.setResizable(true);
  await win.setMinSize(null);
  await win.setMaxSize(null);
  await win.setSize(new LogicalSize(expandedSize.width, expandedSize.height));
  await win.setMinSize(
    new LogicalSize(STATS_EXPANDED_OVERLAY_MIN_WIDTH, STATS_EXPANDED_OVERLAY_MIN_HEIGHT),
  );
}

async function syncStatsMiniInputRegion(
  win: ReturnType<typeof getCurrentWindow>,
  enabled: boolean,
) {
  if (!enabled) {
    await invokeCommand("set_stats_overlay_mini_input_region", {
      enabled: false,
      x: 0,
      y: 0,
      width: 0,
      height: 0,
      minWidth: STATS_MINI_OVERLAY_MIN_WIDTH,
      minHeight: STATS_MINI_OVERLAY_MIN_HEIGHT,
      resizeInset: STATS_MINI_OVERLAY_RESIZE_INSET,
    });
    return;
  }

  const [position, size] = await Promise.all([win.outerPosition(), win.outerSize()]);
  await invokeCommand("set_stats_overlay_mini_input_region", {
    enabled: true,
    x: position.x,
    y: position.y,
    width: size.width,
    height: size.height,
    minWidth: STATS_MINI_OVERLAY_MIN_WIDTH,
    minHeight: STATS_MINI_OVERLAY_MIN_HEIGHT,
    resizeInset: STATS_MINI_OVERLAY_RESIZE_INSET,
  });
}
const IMMUNITY_EN_LABELS: Record<string, string> = {
  f: "F",
  c: "C",
  l: "L",
  p: "P",
  m: "M",
  ph: "Ph",
};
const IMMUNITY_EN_NAMES: Record<string, string> = {
  f: "Fire",
  c: "Cold",
  l: "Lightning",
  p: "Poison",
  m: "Magic",
  ph: "Physical",
};

function getImmunityTextColor(code: string) {
  return ["l", "p", "m", "ph"].includes(code) ? "#18181b" : "#ffffff";
}

function getImmunityLabel(immunity: TerrorZoneImmunity, useEnglish: boolean) {
  return useEnglish ? (IMMUNITY_EN_LABELS[immunity.code] ?? immunity.code.toUpperCase()) : immunity.label;
}

function getImmunityTitle(immunity: TerrorZoneImmunity, useEnglish: boolean) {
  if (useEnglish) {
    return `Monster is immune to ${IMMUNITY_EN_NAMES[immunity.code] ?? immunity.code.toUpperCase()}`;
  }

  const label = immunity.label;
  return `怪物${label}免疫`;
}

function translateOverlaySceneName(sceneName: string, useEnglish: boolean) {
  if (sceneName === MANUAL_FINISH_SCENE) return useEnglish ? "Run ended" : "已手动结束";
  if (!useEnglish) return sceneName;
  if (sceneName === "等待识别...") return "Waiting for detection...";
  return translateTerrorZoneAreaName(sceneName, true);
}

function TerrorZoneInfo({
  label,
  zone,
  useEnglish,
  current = false,
}: {
  label: string;
  zone: TerrorZoneForecast;
  useEnglish: boolean;
  current?: boolean;
}) {
  const locationName = translateTerrorZoneAreaName(zone.location_name, useEnglish);
  const locationDetail = translateTerrorZoneAreaName(zone.location_detail, useEnglish);
  const showLocationDetail = !!locationDetail && locationDetail !== locationName;
  const expTierLabel = useEnglish ? "EXP tier" : "经验等级";
  const lootTierLabel = useEnglish ? "Loot tier" : "财富等级";

  return (
    <section className="tz-forecast-section min-w-0 py-2 first:pt-0 last:pb-0" aria-label={label}>
      <div className="flex items-center justify-between gap-2">
        <span className={`text-xs font-semibold ${current ? "text-[var(--tz-accent)]" : "text-text-secondary"}`}>
          {label}
        </span>
        <span className="text-2xs font-mono font-semibold text-text-muted tabular-nums">
          {zone.display_time}
        </span>
      </div>

      <div
        className="mt-1 flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-1"
      >
        <span
          className={`max-w-full shrink-0 truncate text-md font-bold leading-tight ${current ? "text-[var(--tz-accent)]" : "text-text-primary"}`}
          title={locationDetail}
        >
          {locationName}
        </span>

        <div className="flex shrink-0 items-center gap-1">
          {zone.immunities.map((immunity) => (
            <span
              key={immunity.code}
              className="inline-flex h-4 w-4 items-center justify-center rounded-[5px] text-[9px] font-black leading-none"
              style={{
                backgroundColor: immunity.color,
                color: getImmunityTextColor(immunity.code),
                border: "1px solid rgba(0,0,0,0.14)",
              }}
              title={getImmunityTitle(immunity, useEnglish)}
            >
              {getImmunityLabel(immunity, useEnglish)}
            </span>
          ))}
        </div>
      </div>

      {showLocationDetail && (
        <p className="mt-1 truncate text-xs font-medium text-text-muted" title={locationDetail}>
          {locationDetail}
        </p>
      )}

      <div
        className="mt-1 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5 text-2xs font-semibold text-text-secondary"
      >
        <span className="whitespace-nowrap">
          {expTierLabel}:<span className="ml-0.5 text-text-primary">{zone.tier_exp}</span>
        </span>
        <span className="whitespace-nowrap">
          {lootTierLabel}:<span className="ml-0.5 text-text-primary">{zone.tier_loot}</span>
        </span>
      </div>
    </section>
  );
}

export function Overlay() {
  const overlayWindowLabel = getCurrentWindow().label;
  const isStatsOverlay = overlayWindowLabel === "stats-overlay";
  const supportsCompactMode = overlayWindowLabel === "overlay" || isStatsOverlay;
  const modeStorageKey = isStatsOverlay
    ? STATS_OVERLAY_MODE_STORAGE_KEY
    : OVERLAY_MODE_STORAGE_KEY;
  const expandedSizeStorageKey = isStatsOverlay
    ? STATS_OVERLAY_EXPANDED_SIZE_STORAGE_KEY
    : OVERLAY_EXPANDED_SIZE_STORAGE_KEY;
  const { config } = useGlobalConfig();
  const { theme } = useTheme();
  const { accounts, loadAccounts } = useAccounts();
  const stats = useStats();
  const useEnglish = isEnglishLanguage(config?.app_language);
  const [recentDropHighlights, setRecentDropHighlights] = useState<RecentDropHighlight[]>([]);
  const [recentDropAnnouncement, setRecentDropAnnouncement] = useState("");
  const [statsMiniHovered, setStatsMiniHovered] = useState(false);
  const [showAllDropGroups, setShowAllDropGroups] = useState(false);
  const [dropScope, setDropScope] = useState<DropScope>("current");
  const [dropSlideDirection, setDropSlideDirection] = useState<"previous" | "next">("next");
  const observedDropsRef = useRef(stats.currentDrops);
  const dropHighlightSequenceRef = useRef(0);
  const [displayMode, setDisplayMode] = useState<OverlayDisplayMode>(() =>
    supportsCompactMode ? readStoredOverlayMode(modeStorageKey) : "expanded",
  );
  const [isOverlayWindowVisible, setIsOverlayWindowVisible] = useState(false);
  const displayModeRef = useRef(displayMode);
  const miniSizeRef = useRef<OverlaySize | null>(
    isStatsOverlay ? readStoredStatsMiniOverlaySize() : readStoredMiniOverlaySize(),
  );
  const expandedSizeRef = useRef<OverlaySize>(
    isStatsOverlay
      ? readStoredOverlaySize(
          STATS_OVERLAY_EXPANDED_SIZE_STORAGE_KEY,
          normalizeStatsExpandedOverlaySize,
        ) ?? DEFAULT_STATS_EXPANDED_OVERLAY_SIZE
      : readStoredExpandedOverlaySize() ?? DEFAULT_EXPANDED_OVERLAY_SIZE,
  );
  const overlaySizeSaveTimerRef = useRef<number | null>(null);
  const modeTransitionRef = useRef(false);
  const accountScrollRef = useRef<HTMLDivElement | null>(null);
  const accountButtonRefs = useRef<Map<string, HTMLButtonElement>>(new Map());
  const accountDragStateRef = useRef<{
    pointerId: number;
    startX: number;
    startScrollLeft: number;
    dragged: boolean;
  } | null>(null);
  const overlayWindowDragStateRef = useRef<{
    pointerId: number;
    startX: number;
    startY: number;
  } | null>(null);
  const userWindowMovePendingRef = useRef(false);
  const miniResizeInitTokenRef = useRef(0);
  const miniResizeSessionRef = useRef<MiniOverlayResizeSession | null>(null);
  const suppressOverlayDoubleClickUntilRef = useRef(0);
  const [accountStripScrollable, setAccountStripScrollable] = useState(false);
  const dockStateRef = useRef<OverlayDockState | null>(null);
  const dockMoveTimerRef = useRef<number | null>(null);
  const dockHideTimerRef = useRef<number | null>(null);
  const dockAnimationTokenRef = useRef(0);
  const programmaticDockMoveRef = useRef(false);
  const pointerInsideDockRef = useRef(false);
  const [dockEdge, setDockEdge] = useState<OverlayDockEdge | null>(null);
  const [dockPhase, setDockPhase] = useState<OverlayDockPhase | null>(null);

  const isPollerActive = isStatsOverlay
    ? !!config?.enable_stats_overlay
    : !!config?.enable_tz_overlay;
  const isAudioTrackingActive = isStatsOverlay && !!config?.rune_audio_enabled;

  const startupCheckDoneRef = useRef(false);

  async function getOverlayWindowSize() {
    const win = getCurrentWindow();
    const size = await win.outerSize();
    const scale = await win.scaleFactor();

    return {
      win,
      width: Math.round(size.width / scale),
      height: Math.round(size.height / scale),
    };
  }

  function clearDockMoveTimer() {
    if (dockMoveTimerRef.current !== null) {
      window.clearTimeout(dockMoveTimerRef.current);
      dockMoveTimerRef.current = null;
    }
  }

  function clearDockHideTimer() {
    if (dockHideTimerRef.current !== null) {
      window.clearTimeout(dockHideTimerRef.current);
      dockHideTimerRef.current = null;
    }
  }

  function updateDockState(state: OverlayDockState | null) {
    dockStateRef.current = state;
    setDockEdge(state?.placement.edge ?? null);
    setDockPhase(state?.phase ?? null);
  }

  function cancelDockAnimation() {
    dockAnimationTokenRef.current += 1;
    programmaticDockMoveRef.current = false;
  }

  function clearDocking() {
    clearDockMoveTimer();
    clearDockHideTimer();
    cancelDockAnimation();
    updateDockState(null);
  }

  async function persistOverlayGeometry(
    positionOverride?: PhysicalPoint,
    userInitiated = false,
    dockEdgeOverride?: OverlayDockEdge | null,
  ) {
    try {
      const win = getCurrentWindow();
      const [position, size, scale] = await Promise.all([
        positionOverride ? Promise.resolve(positionOverride) : win.outerPosition(),
        win.outerSize(),
        win.scaleFactor(),
      ]);
      const legacyGeometry = {
        x: Math.round(position.x / scale),
        y: Math.round(position.y / scale),
        width: Math.round(size.width / scale),
        height: Math.round(size.height / scale),
      };
      await Promise.all([
        invokeCommand("save_window_placement", {
          label: isStatsOverlay ? "stats-overlay" : "overlay",
          positionOverride: position,
          dockEdge: dockEdgeOverride ?? dockStateRef.current?.placement.edge ?? null,
          userInitiated,
        }),
        invokeCommand(isStatsOverlay ? "save_stats_overlay_geometry" : "save_overlay_geometry", {
          geometry: legacyGeometry,
        }),
      ]);
    } catch (err) {
      reportOverlayIssue("WARN", "persist geometry failed", err);
    }
  }

  async function animateOverlayPosition(target: PhysicalPoint, finalPhase: OverlayDockPhase) {
    const state = dockStateRef.current;
    if (!state) return;

    const win = getCurrentWindow();
    const start = await win.outerPosition();
    if (pointDistance(start, target) < 1) {
      const settled = { ...state, phase: finalPhase };
      updateDockState(settled);
      return;
    }

    const token = dockAnimationTokenRef.current + 1;
    dockAnimationTokenRef.current = token;
    programmaticDockMoveRef.current = true;
    updateDockState({ ...state, phase: "moving" });

    const reduceMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    if (reduceMotion) {
      try {
        await win.setPosition(new PhysicalPosition(target.x, target.y));
      } catch (err) {
        reportOverlayIssue("WARN", "reduced-motion dock move failed", err);
        programmaticDockMoveRef.current = false;
        return;
      }
    } else {
      const motion = finalPhase === "hidden" ? "hide" : "reveal";
      const duration = calculateOverlayDockAnimationDuration(pointDistance(start, target), motion);
      let animationFailed = false;

      await new Promise<void>((resolve) => {
        let startedAt: number | null = null;
        let pendingPosition: PhysicalPoint | null = null;
        let moveInFlight = false;
        let animationFinished = false;
        let resolved = false;
        let lastRequestedPosition: PhysicalPoint | null = null;

        const finish = () => {
          if (resolved) return;
          resolved = true;
          resolve();
        };

        const flushLatestPosition = () => {
          if (resolved || moveInFlight || !pendingPosition) return;
          const nextPosition = pendingPosition;
          pendingPosition = null;
          moveInFlight = true;

          void win.setPosition(new PhysicalPosition(nextPosition.x, nextPosition.y)).then(() => {
            moveInFlight = false;
            if (dockAnimationTokenRef.current !== token) {
              pendingPosition = null;
              finish();
              return;
            }
            flushLatestPosition();
            if (animationFinished && !moveInFlight && !pendingPosition) finish();
          }).catch((err) => {
            animationFailed = true;
            moveInFlight = false;
            pendingPosition = null;
            reportOverlayIssue("WARN", "dock animation move failed", err);
            finish();
          });
        };

        const queueLatestPosition = (position: PhysicalPoint) => {
          if (
            lastRequestedPosition
            && lastRequestedPosition.x === position.x
            && lastRequestedPosition.y === position.y
          ) {
            return;
          }
          lastRequestedPosition = position;
          pendingPosition = position;
          flushLatestPosition();
        };

        const step = (now: number) => {
          if (resolved) return;
          if (dockAnimationTokenRef.current !== token) {
            pendingPosition = null;
            if (!moveInFlight) finish();
            return;
          }
          if (startedAt === null) startedAt = now;
          const progress = Math.min(1, (now - startedAt) / duration);
          const eased = easeOverlayDockProgress(progress, motion);
          queueLatestPosition({
            x: Math.round(start.x + (target.x - start.x) * eased),
            y: Math.round(start.y + (target.y - start.y) * eased),
          });

          if (progress >= 1) {
            animationFinished = true;
            queueLatestPosition({ x: target.x, y: target.y });
            if (!moveInFlight && !pendingPosition) finish();
          } else {
            window.requestAnimationFrame(step);
          }
        };

        window.requestAnimationFrame(step);
      });

      if (animationFailed) {
        programmaticDockMoveRef.current = false;
        return;
      }
    }

    if (dockAnimationTokenRef.current !== token) return;
    const latest = dockStateRef.current;
    if (latest) updateDockState({ ...latest, phase: finalPhase });
    window.setTimeout(() => {
      if (dockAnimationTokenRef.current === token) {
        programmaticDockMoveRef.current = false;
      }
    }, 32);
  }

  function scheduleDockHide(delay = OVERLAY_DOCK_HIDE_DELAY_MS) {
    clearDockHideTimer();
    dockHideTimerRef.current = window.setTimeout(() => {
      dockHideTimerRef.current = null;
      const state = dockStateRef.current;
      if (!state || pointerInsideDockRef.current) return;
      void animateOverlayPosition(state.placement.hidden, "hidden");
    }, delay);
  }

  async function revealDockedOverlay() {
    clearDockHideTimer();
    const state = dockStateRef.current;
    if (!state || state.phase === "shown") return;
    await animateOverlayPosition(state.placement.visible, "shown");
  }

  async function evaluateOverlayDocking(userInitiated = false) {
    if (
      programmaticDockMoveRef.current
      || modeTransitionRef.current
      || miniResizeSessionRef.current
      || (isStatsOverlay && displayModeRef.current === "mini")
    ) {
      return;
    }
    try {
      const win = getCurrentWindow();
      const [position, size, monitor, scale] = await Promise.all([
        win.outerPosition(),
        win.outerSize(),
        currentMonitor(),
        win.scaleFactor(),
      ]);
      if (!monitor) {
        return;
      }
      const workArea: PhysicalRect = {
        x: monitor.workArea.position.x,
        y: monitor.workArea.position.y,
        width: monitor.workArea.size.width,
        height: monitor.workArea.size.height,
      };
      const physicalSize: PhysicalSize = { width: size.width, height: size.height };
      const edge = findOverlayDockEdge(
        { x: position.x, y: position.y },
        physicalSize,
        workArea,
        OVERLAY_DOCK_SNAP_DISTANCE * scale,
      );
      if (!edge) {
        updateDockState(null);
        await persistOverlayGeometry({ x: position.x, y: position.y }, userInitiated, null);
        return;
      }

      const placement = calculateOverlayDockPlacement(
        edge,
        { x: position.x, y: position.y },
        physicalSize,
        workArea,
        OVERLAY_DOCK_REVEAL_SIZE * scale,
      );
      updateDockState({ placement, workArea, phase: "moving" });
      await persistOverlayGeometry(placement.visible, userInitiated, edge);
      await animateOverlayPosition(placement.visible, "shown");
      scheduleDockHide();
    } catch (err) {
      reportOverlayIssue("WARN", "evaluate edge docking failed", err);
    }
  }

  async function refreshDockPlacementAfterResize(userInitiated = false) {
    const state = dockStateRef.current;
    if (!state) return;
    try {
      const win = getCurrentWindow();
      const [size, scale] = await Promise.all([win.outerSize(), win.scaleFactor()]);
      const placement = calculateOverlayDockPlacement(
        state.placement.edge,
        state.placement.visible,
        { width: size.width, height: size.height },
        state.workArea,
        OVERLAY_DOCK_REVEAL_SIZE * scale,
      );
      const next = { ...state, placement, phase: "shown" as const };
      updateDockState(next);
      await persistOverlayGeometry(
        placement.visible,
        userInitiated,
        state.placement.edge,
      );
      await animateOverlayPosition(placement.visible, "shown");
    } catch (err) {
      reportOverlayIssue("WARN", "refresh dock placement failed", err);
    }
  }

  async function toggleOverlayDisplayMode() {
    if (!supportsCompactMode) return;
    if (modeTransitionRef.current) return;
    modeTransitionRef.current = true;
    const pendingMiniResize = cancelMiniOverlayResize();

    try {
      // Let an already-issued native resize settle before applying the other
      // mode's geometry, so a stale callback cannot overwrite the new size.
      await pendingMiniResize;
      if (dockStateRef.current) {
        await revealDockedOverlay();
      }
      const { win, width, height } = await getOverlayWindowSize();

      if (displayModeRef.current === "expanded") {
        const expandedSize = isStatsOverlay
          ? normalizeStatsExpandedOverlaySize({ width, height })
          : normalizeExpandedOverlaySize({ width, height });
        expandedSizeRef.current = expandedSize;
        storeOverlaySize(expandedSizeStorageKey, expandedSize);

        displayModeRef.current = "mini";
        setStatsMiniHovered(false);
        setDisplayMode("mini");
        storeOverlayMode(modeStorageKey, "mini");

        if (isStatsOverlay) {
          clearDocking();
          const miniSize =
            miniSizeRef.current ??
            readStoredStatsMiniOverlaySize() ??
            DEFAULT_STATS_MINI_OVERLAY_SIZE;
          miniSizeRef.current = miniSize;
          storeOverlaySize(STATS_OVERLAY_MINI_SIZE_STORAGE_KEY, miniSize);
          await applyStatsMiniOverlaySize(win, miniSize);
          await win.setIgnoreCursorEvents(true);
          await syncStatsMiniInputRegion(win, true);
        } else {
          const miniSize =
            miniSizeRef.current ??
            readStoredMiniOverlaySize() ??
            await resolveDefaultMiniOverlaySize();
          miniSizeRef.current = miniSize;
          storeMiniOverlaySize(miniSize);
          await applyMiniOverlaySize(win, miniSize);
        }
      } else {
        if (isStatsOverlay) {
          const miniSize = normalizeStatsMiniOverlaySize({ width, height });
          miniSizeRef.current = miniSize;
          storeOverlaySize(STATS_OVERLAY_MINI_SIZE_STORAGE_KEY, miniSize);
          await syncStatsMiniInputRegion(win, false);
          await win.setIgnoreCursorEvents(false);
        } else {
          const miniSize = normalizeMiniOverlaySize({ width, height });
          miniSizeRef.current = miniSize;
          storeMiniOverlaySize(miniSize);
        }

        const expandedSize = expandedSizeRef.current;
        displayModeRef.current = "expanded";
        setStatsMiniHovered(false);
        setDisplayMode("expanded");
        storeOverlayMode(modeStorageKey, "expanded");

        if (isStatsOverlay) {
          await applyStatsExpandedOverlaySize(win, expandedSize);
        } else {
          await applyExpandedOverlaySize(win, expandedSize);
        }
      }
      if (dockStateRef.current && !(isStatsOverlay && displayModeRef.current === "mini")) {
        await refreshDockPlacementAfterResize();
      } else if (isStatsOverlay && displayModeRef.current === "expanded") {
        await evaluateOverlayDocking();
      }
    } catch (err) {
      reportOverlayIssue("WARN", "toggle information overlay mode failed", err);
    } finally {
      modeTransitionRef.current = false;
    }
  }

  function handleOverlayDoubleClick(event: React.MouseEvent<HTMLDivElement>) {
    if (!supportsCompactMode) return;
    const target = event.target;
    if (
      isStatsOverlay
      && target instanceof Element
      && target.closest('[data-overlay-interactive="true"]')
    ) {
      return;
    }
    if (performance.now() < suppressOverlayDoubleClickUntilRef.current) {
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    void toggleOverlayDisplayMode();
  }

  useEffect(() => {
    if (!supportsCompactMode) return;

    const handleOverlayWindowKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Enter" || event.repeat) return;
      const target = event.target;
      if (
        isStatsOverlay
        && target instanceof Element
        && target.closest('[data-overlay-interactive="true"]')
      ) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      void toggleOverlayDisplayMode();
    };

    // Listen at the native window level so Enter works immediately after the
    // overlay is shown, even before any element inside the webview is focused.
    window.addEventListener("keydown", handleOverlayWindowKeyDown, true);
    return () => window.removeEventListener("keydown", handleOverlayWindowKeyDown, true);
  }, [isStatsOverlay, supportsCompactMode]);

  useEffect(() => {
    if (!isStatsOverlay) return;
    const unlisten = listenEvent<string>("global-input-event", (event) => {
      if (event.payload === "StatsOverlayMiniHoverEnter") {
        if (displayModeRef.current === "mini") setStatsMiniHovered(true);
        return;
      }
      if (event.payload === "StatsOverlayMiniHoverLeave") {
        setStatsMiniHovered(false);
        return;
      }
      if (event.payload !== "StatsOverlayMiniToggle") return;
      if (displayModeRef.current !== "mini") return;
      void toggleOverlayDisplayMode();
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, [isStatsOverlay]);

  function finishCurrentTimer() {
    if (!useStats.getState().isTiming) return;
    void useStats.getState().finishRunAsTown();
  }

  function handleTimerDoubleClick(event: React.MouseEvent<HTMLDivElement>) {
    event.preventDefault();
    event.stopPropagation();
    finishCurrentTimer();
  }

  function handleTimerKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    event.stopPropagation();
    finishCurrentTimer();
  }

  function handleOverlayWindowPointerDownCapture(event: React.PointerEvent<HTMLDivElement>) {
    if (isStatsOverlay && displayModeRef.current === "mini") return;
    if (event.button !== 0 || event.detail > 1) return;
    const target = event.target;
    if (!(target instanceof Element)) return;
    if (
      target.closest(
        'button, input, select, textarea, a, [data-overlay-interactive="true"], .overlay-account-scroll.is-scrollable',
      )
    ) {
      return;
    }

    overlayWindowDragStateRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  function handleOverlayWindowPointerMoveCapture(event: React.PointerEvent<HTMLDivElement>) {
    if (isStatsOverlay && displayModeRef.current === "mini") return;
    const drag = overlayWindowDragStateRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;

    const distance = Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY);
    if (distance < OVERLAY_WINDOW_DRAG_THRESHOLD_PX) return;

    overlayWindowDragStateRef.current = null;
    suppressOverlayDoubleClickUntilRef.current = performance.now() + 350;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    event.preventDefault();
    event.stopPropagation();
    clearDocking();
    userWindowMovePendingRef.current = true;
    void getCurrentWindow().startDragging().catch((err) => {
      userWindowMovePendingRef.current = false;
      reportOverlayIssue("WARN", "start window dragging failed", err);
    });
  }

  function finishOverlayWindowPointerGesture(event: React.PointerEvent<HTMLDivElement>) {
    const drag = overlayWindowDragStateRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;

    overlayWindowDragStateRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  }

  async function flushMiniOverlayResize(session: MiniOverlayResizeSession) {
    if (
      session.cancelled
      || displayModeRef.current !== "mini"
      || session.moveInFlight
      || !session.pendingBounds
    ) {
      return;
    }

    const bounds = session.pendingBounds;
    session.pendingBounds = null;
    session.moveInFlight = true;
    const win = getCurrentWindow();

    const operation = (async () => {
      try {
        const operations: Promise<void>[] = [
          win.setSize(new LogicalSize(bounds.width, bounds.height)),
        ];
        if (session.edge.includes("w") || session.edge.includes("n")) {
          operations.push(
            win.setPosition(
              new PhysicalPosition(
                session.startLeftPhysical + Math.round(bounds.offsetX * session.scaleFactor),
                session.startTopPhysical + Math.round(bounds.offsetY * session.scaleFactor),
              ),
            ),
          );
        }
        await Promise.all(operations);
      } catch (err) {
        reportOverlayIssue("WARN", "custom mini resize failed", err);
      }
    })();
    session.flushPromise = operation;

    try {
      await operation;
    } finally {
      session.moveInFlight = false;
      session.flushPromise = null;
      if (miniResizeSessionRef.current !== session) return;
      if (!session.cancelled && session.pendingBounds) {
        void flushMiniOverlayResize(session);
      } else if (session.cancelled || !session.active) {
        miniResizeSessionRef.current = null;
      }
    }
  }

  function cancelMiniOverlayResize() {
    miniResizeInitTokenRef.current += 1;
    const session = miniResizeSessionRef.current;
    if (!session) return Promise.resolve();

    session.active = false;
    session.cancelled = true;
    session.pendingBounds = null;
    if (!session.moveInFlight) {
      miniResizeSessionRef.current = null;
    }
    return session.flushPromise ?? Promise.resolve();
  }

  function handleMiniOverlayResizePointerDown(
    event: React.PointerEvent<HTMLDivElement>,
    edge: MiniOverlayResizeEdge,
  ) {
    if (
      event.button !== 0
      || displayModeRef.current !== "mini"
      || miniResizeSessionRef.current?.active
    ) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    suppressOverlayDoubleClickUntilRef.current = performance.now() + 350;
    clearDocking();

    const handle = event.currentTarget;
    const pointerId = event.pointerId;
    const startScreenX = event.screenX;
    const startScreenY = event.screenY;
    const initToken = miniResizeInitTokenRef.current + 1;
    miniResizeInitTokenRef.current = initToken;
    handle.setPointerCapture(pointerId);

    void (async () => {
      const win = getCurrentWindow();
      const [size, position, scaleFactor] = await Promise.all([
        win.outerSize(),
        win.outerPosition(),
        win.scaleFactor(),
      ]);
      if (
        miniResizeInitTokenRef.current !== initToken
        || !handle.hasPointerCapture(pointerId)
      ) {
        return;
      }

      miniResizeSessionRef.current = {
        active: true,
        cancelled: false,
        pointerId,
        edge,
        startScreenX,
        startScreenY,
        startSize: {
          width: Math.round(size.width / scaleFactor),
          height: Math.round(size.height / scaleFactor),
        },
        startLeftPhysical: position.x,
        startTopPhysical: position.y,
        scaleFactor,
        pendingBounds: null,
        moveInFlight: false,
        flushPromise: null,
      };
    })().catch((err) => {
      reportOverlayIssue("WARN", "initialize custom mini resize failed", err);
    });
  }

  function handleMiniOverlayResizePointerMove(event: React.PointerEvent<HTMLDivElement>) {
    const session = miniResizeSessionRef.current;
    if (!session?.active || session.pointerId !== event.pointerId) return;

    event.preventDefault();
    event.stopPropagation();
    session.pendingBounds = calculateMiniOverlayResizeBounds(
      session.startSize,
      session.edge,
      event.screenX - session.startScreenX,
      event.screenY - session.startScreenY,
    );
    void flushMiniOverlayResize(session);
  }

  function finishMiniOverlayResize(event: React.PointerEvent<HTMLDivElement>) {
    miniResizeInitTokenRef.current += 1;
    const session = miniResizeSessionRef.current;
    if (session?.pointerId === event.pointerId) {
      session.active = false;
      if (!session.moveInFlight && !session.pendingBounds) {
        miniResizeSessionRef.current = null;
      } else {
        void flushMiniOverlayResize(session);
      }
    }
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    event.preventDefault();
    event.stopPropagation();
  }

  function cancelMiniOverlayResizePointerGesture(event: React.PointerEvent<HTMLDivElement>) {
    const session = miniResizeSessionRef.current;
    if (session?.active && session.pointerId === event.pointerId) {
      void cancelMiniOverlayResize();
    }
    event.preventDefault();
    event.stopPropagation();
  }

  function handleMiniOverlayResizeLostPointerCapture(event: React.PointerEvent<HTMLDivElement>) {
    const session = miniResizeSessionRef.current;
    // A normal pointer-up marks the session inactive before releasing capture.
    // Only an unexpected capture loss should cancel queued native writes.
    if (session?.active && session.pointerId === event.pointerId) {
      void cancelMiniOverlayResize();
    }
  }

  async function focusAccountWindow(displayName: string) {
    try {
      const ok = await invokeCommand<boolean>("bring_window_by_title_to_front", {
        windowTitle: displayName,
      });
      if (!ok) {
        reportOverlayIssue("WARN", "bring_window_by_title_to_front returned false", displayName);
      } else {
        // Optimistically reflect the requested focus so its pill becomes visible immediately.
        setForegroundTitle(displayName);
      }
    } catch (err) {
      reportOverlayIssue("ERROR", "bring_window_by_title_to_front failed", err);
    }
  }

  function handleAccountPointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    const container = event.currentTarget;
    if (container.scrollWidth <= container.clientWidth) return;

    accountDragStateRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startScrollLeft: container.scrollLeft,
      dragged: false,
    };
  }

  function handleAccountPointerMove(event: React.PointerEvent<HTMLDivElement>) {
    const drag = accountDragStateRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;

    const deltaX = event.clientX - drag.startX;
    if (!drag.dragged && Math.abs(deltaX) < 4) return;
    if (!drag.dragged) {
      drag.dragged = true;
      event.currentTarget.setPointerCapture(event.pointerId);
    }
    event.currentTarget.dataset.dragging = "true";
    event.currentTarget.scrollLeft = drag.startScrollLeft - deltaX;
    event.preventDefault();
  }

  function finishAccountPointerDrag(event: React.PointerEvent<HTMLDivElement>) {
    const drag = accountDragStateRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;

    if (drag.dragged) {
      suppressOverlayDoubleClickUntilRef.current = performance.now() + 350;
      event.preventDefault();
    }
    delete event.currentTarget.dataset.dragging;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    accountDragStateRef.current = null;
  }

  // Apply font scale on startup from localStorage, then sync from config
  useEffect(() => {
    try {
      const saved = localStorage.getItem("d2rhub-font-scale");
      if (saved && ["small","default","large"].includes(saved)) {
        document.documentElement.dataset.fontScale = saved;
      } else {
        document.documentElement.dataset.fontScale = "default";
      }
    } catch {
      document.documentElement.dataset.fontScale = "default";
    }
  }, []);

  // Sync font scale from config when it loads/changes
  useEffect(() => {
    if (!config?.font_scale) return;
    if (["small","default","large"].includes(config.font_scale)) {
      document.documentElement.dataset.fontScale = config.font_scale;
      try { localStorage.setItem("d2rhub-font-scale", config.font_scale); } catch {}
    }
  }, [config?.font_scale]);

  useEffect(() => {
    const container = accountScrollRef.current;
    if (!container) return;

    const handleWheel = (event: WheelEvent) => {
      if (container.scrollWidth <= container.clientWidth) return;
      const delta = Math.abs(event.deltaX) > Math.abs(event.deltaY)
        ? event.deltaX
        : event.deltaY;
      if (delta === 0) return;

      event.preventDefault();
      container.scrollLeft += delta;
    };

    container.addEventListener("wheel", handleWheel, { passive: false });
    return () => container.removeEventListener("wheel", handleWheel);
  }, []);

  const [foregroundTitle, setForegroundTitle] = useState("");
  const [terrorZones, setTerrorZones] = useState<TerrorZoneSnapshot>({ current: null, next: null });
  const [terrorZoneStatus, setTerrorZoneStatus] = useState<TerrorZoneStatus>("loading");

  // Sync theme on startup / changes
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    initConfigSync().then((stopListening) => {
      if (cancelled) stopListening();
      else unlisten = stopListening;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Sync theme from global config (config as source of truth)
  useEffect(() => {
    if (!config?.theme_overlay) return;
    syncThemeFromConfig(config.theme_overlay);
  }, [config?.theme_overlay]);

  // Page visibility follows native hide/show in WebView2. Re-check the native
  // window on visibility and focus transitions so presentation-only ticking
  // never runs while this always-on-top window is hidden.
  useEffect(() => {
    let cancelled = false;
    let unlistenFocus: (() => void) | undefined;
    const win = getCurrentWindow();

    const syncWindowVisibility = async () => {
      if (document.visibilityState !== "visible") {
        if (!cancelled) setIsOverlayWindowVisible(false);
        return;
      }

      try {
        const visible = await win.isVisible();
        if (!cancelled) setIsOverlayWindowVisible(visible);
      } catch {
        if (!cancelled) setIsOverlayWindowVisible(document.visibilityState === "visible");
      }
    };

    const handleVisibilityChange = () => {
      void syncWindowVisibility();
    };

    document.addEventListener("visibilitychange", handleVisibilityChange);
    void syncWindowVisibility();
    win.onFocusChanged(handleVisibilityChange)
      .then((unlisten) => {
        if (cancelled) unlisten();
        else unlistenFocus = unlisten;
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      document.removeEventListener("visibilitychange", handleVisibilityChange);
      unlistenFocus?.();
    };
  }, []);

  useEffect(() => {
    return () => {
      void cancelMiniOverlayResize();
    };
  }, []);

  useEffect(() => {
    if (isStatsOverlay || !isPollerActive) return;

    let cancelled = false;
    let refreshTimer: number | undefined;

    function queueNextLoad(snapshot: TerrorZoneSnapshot | null, fallbackDelayMs = 5 * 60 * 1000) {
      if (refreshTimer !== undefined) {
        window.clearTimeout(refreshTimer);
      }

      const refreshAt = snapshot?.next?.start_time ?? snapshot?.current?.end_time;
      const delayMs = refreshAt
        ? Math.max(
            15 * 1000,
            Math.min(10 * 60 * 1000, refreshAt * 1000 - Date.now() + 60 * 1000),
          )
        : fallbackDelayMs;

      refreshTimer = window.setTimeout(loadNextTerrorZone, delayMs);
    }

    async function loadNextTerrorZone() {
      try {
        const snapshot = await invokeCommand<TerrorZoneSnapshot>("get_terror_zone_snapshot");
        if (cancelled) return;
        setTerrorZones(snapshot);
        setTerrorZoneStatus(snapshot.current || snapshot.next ? "ready" : "empty");
        queueNextLoad(snapshot, 60 * 1000);
      } catch (err) {
        reportOverlayIssue("WARN", "get_terror_zone_snapshot failed", err);
        if (!cancelled) {
          setTerrorZones({ current: null, next: null });
          setTerrorZoneStatus("error");
          queueNextLoad(null);
        }
      }
    }

    loadNextTerrorZone();
    return () => {
      cancelled = true;
      if (refreshTimer !== undefined) {
        window.clearTimeout(refreshTimer);
      }
    };
  }, [isPollerActive]);

  // Set character name from monitored account
  useEffect(() => {
    if (!isStatsOverlay || (!isPollerActive && !isAudioTrackingActive)) return;
    if (config?.rune_audio_target_account) {
      const target = accounts.find((a) => a.id === config.rune_audio_target_account);
      if (target) {
        stats.setCharacterName(target.display_name || target.id);
      }
    }
  }, [config?.rune_audio_target_account, accounts, stats.setCharacterName, isAudioTrackingActive, isPollerActive, isStatsOverlay]);

  // Restore position and each mode's independent preferred size. The geometry
  // file is only a migration fallback for whichever mode was active last.
  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        const saved = await invokeCommand<any>(
          isStatsOverlay ? "load_stats_overlay_geometry" : "load_overlay_geometry",
        );
        if (cancelled) return;

        const win = getCurrentWindow();

        if (isStatsOverlay) {
          const savedExpandedSize =
            displayModeRef.current === "expanded"
            && saved
            && saved.width >= STATS_EXPANDED_OVERLAY_MIN_WIDTH
            && saved.height >= STATS_EXPANDED_OVERLAY_MIN_HEIGHT
              ? normalizeStatsExpandedOverlaySize({ width: saved.width, height: saved.height })
              : null;
          const expandedSize =
            readStoredOverlaySize(
              STATS_OVERLAY_EXPANDED_SIZE_STORAGE_KEY,
              normalizeStatsExpandedOverlaySize,
            ) ?? savedExpandedSize ?? DEFAULT_STATS_EXPANDED_OVERLAY_SIZE;
          const savedMiniSize =
            displayModeRef.current === "mini"
            && saved
            && saved.width >= STATS_MINI_OVERLAY_MIN_WIDTH
            && saved.height >= STATS_MINI_OVERLAY_MIN_HEIGHT
            && saved.height < STATS_EXPANDED_OVERLAY_MIN_HEIGHT
              ? normalizeStatsMiniOverlaySize({ width: saved.width, height: saved.height })
              : null;
          const miniSize =
            readStoredStatsMiniOverlaySize()
            ?? savedMiniSize
            ?? DEFAULT_STATS_MINI_OVERLAY_SIZE;
          miniSizeRef.current = miniSize;
          expandedSizeRef.current = expandedSize;
          storeOverlaySize(STATS_OVERLAY_MINI_SIZE_STORAGE_KEY, miniSize);
          storeOverlaySize(STATS_OVERLAY_EXPANDED_SIZE_STORAGE_KEY, expandedSize);

          if (displayModeRef.current === "mini") {
            await applyStatsMiniOverlaySize(win, miniSize);
            await win.setIgnoreCursorEvents(true);
          } else {
            await syncStatsMiniInputRegion(win, false);
            await win.setIgnoreCursorEvents(false);
            await applyStatsExpandedOverlaySize(win, expandedSize);
          }
          await restoreWindowPlacement("stats-overlay", saved);
          if (displayModeRef.current === "mini") {
            await syncStatsMiniInputRegion(win, true);
          } else {
            window.setTimeout(() => {
              if (!cancelled) void evaluateOverlayDocking();
            }, OVERLAY_DOCK_SETTLE_DELAY_MS);
          }
          return;
        }

        const savedExpandedSize =
          displayModeRef.current === "expanded"
          && saved
          && saved.width >= EXPANDED_OVERLAY_MIN_WIDTH
          && saved.height >= EXPANDED_OVERLAY_MIN_HEIGHT
            ? normalizeExpandedOverlaySize({ width: saved.width, height: saved.height })
            : null;
        const savedMiniSize =
          displayModeRef.current === "mini"
          && saved
          && saved.width > 0
          && saved.height > 0
            ? normalizeMiniOverlaySize({ width: saved.width, height: saved.height })
            : null;
        const miniSize =
          readStoredMiniOverlaySize() ??
          savedMiniSize ??
          await resolveDefaultMiniOverlaySize();
        const expandedSize =
          readStoredExpandedOverlaySize() ??
          savedExpandedSize ??
          DEFAULT_EXPANDED_OVERLAY_SIZE;
        if (cancelled) return;

        miniSizeRef.current = miniSize;
        expandedSizeRef.current = expandedSize;
        storeMiniOverlaySize(miniSize);
        storeExpandedOverlaySize(expandedSize);

        if (displayModeRef.current === "mini") {
          await applyMiniOverlaySize(win, miniSize);
        } else {
          await applyExpandedOverlaySize(win, expandedSize);
        }
        await restoreWindowPlacement("overlay", saved);
        window.setTimeout(() => {
          if (!cancelled && supportsCompactMode) void evaluateOverlayDocking();
        }, OVERLAY_DOCK_SETTLE_DELAY_MS);
      } catch (err) {
        reportOverlayIssue("WARN", "restore information overlay geometry failed", err);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  // Persist the user-visible size for the active mode. Each mode owns its own
  // record, so resizing mini never overwrites the preferred expanded layout.
  useEffect(() => {
    let cancelled = false;
    let unlistenResize: (() => void) | undefined;

    (async () => {
      try {
        const win = getCurrentWindow();
        const stopListening = await win.onResized(() => {
          const resizedMode = displayModeRef.current;
          if (overlaySizeSaveTimerRef.current !== null) {
            window.clearTimeout(overlaySizeSaveTimerRef.current);
          }
          overlaySizeSaveTimerRef.current = window.setTimeout(async () => {
            overlaySizeSaveTimerRef.current = null;
            if (modeTransitionRef.current || displayModeRef.current !== resizedMode) return;
            try {
              if (isStatsOverlay) {
                const { width, height } = await getOverlayWindowSize();
                if (resizedMode === "mini") {
                  const miniSize = normalizeStatsMiniOverlaySize({ width, height });
                  miniSizeRef.current = miniSize;
                  storeOverlaySize(STATS_OVERLAY_MINI_SIZE_STORAGE_KEY, miniSize);
                  await syncStatsMiniInputRegion(getCurrentWindow(), true);
                  await persistOverlayGeometry(undefined, true);
                  return;
                }
                const expandedSize = normalizeStatsExpandedOverlaySize({ width, height });
                expandedSizeRef.current = expandedSize;
                storeOverlaySize(STATS_OVERLAY_EXPANDED_SIZE_STORAGE_KEY, expandedSize);
                if (dockStateRef.current) {
                  await refreshDockPlacementAfterResize(true);
                } else {
                  await persistOverlayGeometry(undefined, true);
                }
                return;
              }
              const { width, height } = await getOverlayWindowSize();
              if (resizedMode === "mini") {
                const miniSize = normalizeMiniOverlaySize({ width, height });
                miniSizeRef.current = miniSize;
                storeMiniOverlaySize(miniSize);
              } else {
                const expandedSize = normalizeExpandedOverlaySize({ width, height });
                expandedSizeRef.current = expandedSize;
                storeExpandedOverlaySize(expandedSize);
              }
              if (dockStateRef.current) {
                await refreshDockPlacementAfterResize(true);
              } else {
                await persistOverlayGeometry(undefined, true);
              }
            } catch (err) {
              reportOverlayIssue("WARN", `persist ${resizedMode} size failed`, err);
            }
          }, 250);
        });
        if (cancelled) stopListening();
        else unlistenResize = stopListening;
      } catch (err) {
        reportOverlayIssue("WARN", "listen for preferred size changes failed", err);
      }
    })();

    return () => {
      cancelled = true;
      unlistenResize?.();
      if (overlaySizeSaveTimerRef.current !== null) {
        window.clearTimeout(overlaySizeSaveTimerRef.current);
      }
    };
  }, []);

  // Native dragging leaves the webview pointer stream, so movement is observed
  // at the window level and settled after the final OS move event.
  useEffect(() => {
    let cancelled = false;
    let unlistenMove: (() => void) | undefined;

    (async () => {
      try {
        const win = getCurrentWindow();
        const stopListening = await win.onMoved(() => {
          if (
            cancelled
            || programmaticDockMoveRef.current
            || modeTransitionRef.current
            || miniResizeSessionRef.current
          ) {
            return;
          }
          if (isStatsOverlay && displayModeRef.current === "mini") {
            clearDockMoveTimer();
            dockMoveTimerRef.current = window.setTimeout(() => {
              dockMoveTimerRef.current = null;
              if (!cancelled && displayModeRef.current === "mini") {
                void (async () => {
                  try {
                    await syncStatsMiniInputRegion(win, true);
                    await persistOverlayGeometry(undefined, true);
                  } catch (err) {
                    reportOverlayIssue("WARN", "persist moved stats mini overlay failed", err);
                  }
                })();
              }
            }, OVERLAY_DOCK_SETTLE_DELAY_MS);
            return;
          }
          clearDockMoveTimer();
          const userInitiated = userWindowMovePendingRef.current;
          dockMoveTimerRef.current = window.setTimeout(() => {
            dockMoveTimerRef.current = null;
            if (!cancelled) {
              void evaluateOverlayDocking(userInitiated).finally(() => {
                if (userInitiated) userWindowMovePendingRef.current = false;
              });
            }
          }, OVERLAY_DOCK_SETTLE_DELAY_MS);
        });
        if (cancelled) stopListening();
        else unlistenMove = stopListening;
      } catch (err) {
        reportOverlayIssue("WARN", "listen for edge docking failed", err);
      }
    })();

    return () => {
      cancelled = true;
      unlistenMove?.();
      clearDockMoveTimer();
      clearDockHideTimer();
      cancelDockAnimation();
    };
  }, []);

  // Config is updated in real-time via the Tauri event listener in globalConfig.ts

  // Load accounts
  useEffect(() => {
    if (!isStatsOverlay || (!isPollerActive && !isAudioTrackingActive)) return;
    let cancelled = false;
    let timer: number | undefined;

    const poll = async () => {
      await loadAccounts();
      if (!cancelled && isOverlayWindowVisible && isPollerActive) {
        timer = window.setTimeout(poll, 5000);
      }
    };

    void poll();
    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [isAudioTrackingActive, loadAccounts, isOverlayWindowVisible, isPollerActive, isStatsOverlay]);

  // 启动时检查已运行的 D2R 窗口，立即更新悬浮窗状态（仅执行一次）
  useEffect(() => {
    if (startupCheckDoneRef.current) return;
    if (!isStatsOverlay || (!isPollerActive && !isAudioTrackingActive)) return;
    if (!config?.rune_audio_target_account) return;
    (async () => {
      try {
        startupCheckDoneRef.current = true;

        // 刷新账号运行状态（扫描 D2R 窗口匹配账号昵称 → 更新 active_games）
        const matchedIds: string[] = await invokeCommand("refresh_account_running_state");
        if (matchedIds.length > 0) {
          // 等待账号列表加载完成
          await loadAccounts();
        }

        // 如果任一 D2R 窗口标题包含被监控账号的昵称，直接设置为前台标题
        const titles: string[] = await invokeCommand("get_d2r_window_titles");
        if (titles.length > 0 && config?.rune_audio_target_account) {
          // 使用 getState() 读取最新账号列表，避免将 accounts 加入依赖造成循环
          const latestAccounts = useAccounts.getState().accounts;
          const target = latestAccounts.find((a) => a.id === config.rune_audio_target_account);
          if (target) {
            const displayName = target.display_name || target.id;
            const match = titles.find((t) =>
              t.toLowerCase().includes(displayName.toLowerCase())
            );
            if (match) {
              setForegroundTitle(match);
            }
          }
        }
      } catch {}
    })();
  }, [config?.rune_audio_target_account, isAudioTrackingActive, isPollerActive, isStatsOverlay, loadAccounts]);

  // 前台窗口标题轮询
  useEffect(() => {
    if (!isStatsOverlay || !isPollerActive || !isOverlayWindowVisible) return;
    let cancelled = false;
    let timer: number | undefined;

    const poll = async () => {
      try {
        const title = await invokeCommand<string>("get_foreground_window_title");
        if (!cancelled) setForegroundTitle(title);
      } catch {}
      if (!cancelled) timer = window.setTimeout(poll, 1000);
    };

    void poll();
    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [isOverlayWindowVisible, isPollerActive]);

  // ── 音频遥测事件（后端实时捕获并推送）──
  useEffect(() => {
    if (!isAudioTrackingActive) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listenEvent<RuneAudioEvent>("rune-audio-detected", (event) => {
      if (cancelled || event.payload.account_id !== config.rune_audio_target_account) return;
      stats.processRuneDrop({
        rune_number: event.payload.rune_number,
        rune_name: event.payload.rune_name,
        rune_name_en: event.payload.rune_name_en,
      });
    }).then((stop) => {
      if (cancelled) stop();
      else unlisten = stop;
    }).catch((error) => reportOverlayIssue("ERROR", "监听符文声纹事件失败", error));
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [config?.rune_audio_target_account, isAudioTrackingActive, stats.processRuneDrop]);

  useEffect(() => {
    if (!isAudioTrackingActive) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listenEvent<ItemAudioEvent>("item-audio-detected", (event) => {
      if (cancelled || event.payload.account_id !== config.rune_audio_target_account) return;
      stats.processItemDrop(event.payload);
    }).then((stop) => {
      if (cancelled) stop();
      else unlisten = stop;
    }).catch((error) => reportOverlayIssue("ERROR", "监听物品声纹事件失败", error));
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [config?.rune_audio_target_account, isAudioTrackingActive, stats.processItemDrop]);

  useEffect(() => {
    if (!isAudioTrackingActive) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listenEvent<TrackingSnapshot>("audio-tracking-state", (event) => {
      if (cancelled || event.payload.account_id !== config.rune_audio_target_account) return;
      useStats.getState().applyTrackingSnapshot(event.payload);
    }).then((stop) => {
      if (cancelled) stop();
      else unlisten = stop;
    }).catch((error) => reportOverlayIssue("ERROR", "监听自动刷图统计状态失败", error));
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [config?.rune_audio_target_account, isAudioTrackingActive]);

  useEffect(() => {
    if (!isAudioTrackingActive || !config?.rune_audio_target_account) return;
    const target = accounts.find((account) => account.id === config.rune_audio_target_account);
    if (!target?.is_running) return;
    let cancelled = false;
    void invokeCommand<{ running: boolean }>("get_rune_audio_status").then((status) => {
      if (!cancelled && !status.running) return invokeCommand("start_rune_audio_monitor");
    }).catch((error) => reportOverlayIssue("WARN", "启动符文声纹监控失败", error));
    return () => { cancelled = true; };
  }, [accounts, config?.rune_audio_target_account, isAudioTrackingActive]);

  // ── 计时器 tick (100ms → 0.1s 精度) ──
  useEffect(() => {
    if (!isStatsOverlay || !isPollerActive || !isOverlayWindowVisible) return;
    const tick = () => useStats.getState().tick();
    tick();
    const interval = setInterval(tick, 100);
    return () => clearInterval(interval);
  }, [displayMode, isOverlayWindowVisible, isPollerActive]);

  useEffect(() => {
    const previousDrops = observedDropsRef.current;
    const currentDrops = stats.currentDrops;
    observedDropsRef.current = currentDrops;
    if (!isStatsOverlay) return;

    const addedDrops = getAppendedOverlayDrops(previousDrops, currentDrops);
    if (addedDrops.length === 0) return;
    const now = Date.now();
    const addedGroups = aggregateOverlayDrops(addedDrops);
    const addedKeys = new Set(addedGroups.map((group) => group.key));
    const currentGroups = new Map(
      aggregateOverlayDrops(currentDrops).map((group) => [group.key, group]),
    );

    setRecentDropAnnouncement(addedGroups.map(({ key, drop }) => {
      const total = currentGroups.get(key)?.count ?? 1;
      const label = getOverlayDropLabel(drop, useEnglish);
      return useEnglish ? `${label}, ${total} total` : `${label}，共 ${total} 个`;
    }).join(useEnglish ? "; " : "；"));

    setRecentDropHighlights((current) => {
      const active = current.filter((highlight) => highlight.expiresAt > now);
      const addedHighlights = addedGroups.map(({ key }) => ({
        id: ++dropHighlightSequenceRef.current,
        key,
        expiresAt: now + RECENT_DROP_HIGHLIGHT_DURATION_MS,
      }));
      return [
        ...addedHighlights,
        ...active.filter((highlight) => !addedKeys.has(highlight.key)),
      ];
    });
  }, [isStatsOverlay, stats.currentDrops, useEnglish]);

  useEffect(() => {
    if (recentDropHighlights.length === 0) return;
    const nextExpiration = Math.min(...recentDropHighlights.map((highlight) => highlight.expiresAt));
    const timeout = window.setTimeout(() => {
      const now = Date.now();
      setRecentDropHighlights((current) => current.filter((highlight) => highlight.expiresAt > now));
    }, Math.max(0, nextExpiration - Date.now()) + 20);
    return () => window.clearTimeout(timeout);
  }, [recentDropHighlights]);

  // ── 派生数据 ──
  const activeAccounts = accounts.filter((a) => a.is_running);
  const scopedDrops = dropScope === "current"
    ? stats.currentRunDrops
    : dropScope === "previous"
      ? stats.previousRunDrops
      : stats.currentDrops;
  const aggregatedDrops = React.useMemo(
    () => aggregateOverlayDrops(scopedDrops),
    [scopedDrops],
  );
  const displayedDropGroups = showAllDropGroups
    ? aggregatedDrops
    : aggregatedDrops.slice(0, COLLAPSED_DROP_GROUP_LIMIT);
  const hiddenDropGroupCount = Math.max(0, aggregatedDrops.length - COLLAPSED_DROP_GROUP_LIMIT);
  const recentDropHighlightIds = React.useMemo(
    () => new Map(recentDropHighlights.map((highlight) => [highlight.key, highlight.id])),
    [recentDropHighlights],
  );

  useEffect(() => {
    if (aggregatedDrops.length <= COLLAPSED_DROP_GROUP_LIMIT) {
      setShowAllDropGroups(false);
    }
  }, [aggregatedDrops.length]);
  const activeAccountIdsKey = activeAccounts.map((account) => account.id).join("|");
  const foregroundTitleLower = foregroundTitle.toLowerCase();
  const focusedAccountId = activeAccounts.find((account) => {
    const displayName = account.display_name || account.id;
    return displayName.length > 0 && foregroundTitleLower.includes(displayName.toLowerCase());
  })?.id;

  useEffect(() => {
    if (!isStatsOverlay) return;
    const container = accountScrollRef.current;
    if (!container) return;

    const syncScrollableState = () => {
      setAccountStripScrollable(container.scrollWidth > container.clientWidth + 1);
    };
    syncScrollableState();

    const observer = new ResizeObserver(syncScrollableState);
    observer.observe(container);
    return () => observer.disconnect();
  }, [activeAccountIdsKey, displayMode]);

  useEffect(() => {
    if (!isStatsOverlay || !focusedAccountId) return;
    const frame = window.requestAnimationFrame(() => {
      const container = accountScrollRef.current;
      const pill = accountButtonRefs.current.get(focusedAccountId);
      if (!container || !pill) return;

      const centeredLeft = pill.offsetLeft - (container.clientWidth - pill.offsetWidth) / 2;
      const maxScrollLeft = Math.max(0, container.scrollWidth - container.clientWidth);
      const left = Math.max(0, Math.min(maxScrollLeft, centeredLeft));
      const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      container.scrollTo({ left, behavior: reduceMotion ? "auto" : "smooth" });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [activeAccountIdsKey, displayMode, focusedAccountId]);

  const avgTime = stats.dbAvgTime;
  const currentSessionRuns = stats.sessionRuns[
    getSessionRunKey(stats.currentRunName || stats.currentScene, stats.currentTz)
  ] || 0;
  const elapsedDisplay = stats.isTiming
    ? (stats.elapsedMs / 1000).toFixed(1)
    : "0.0";
  const currentTerrorZone = terrorZones.current;
  const nextTerrorZone = terrorZones.next;
  const hasTerrorZoneData = !!(currentTerrorZone || nextTerrorZone);
  const currentTerrorZoneLabel = useEnglish ? "Current" : "当前";
  const nextTerrorZoneLabel = useEnglish ? "Next" : "下一个";
  const miniNextTerrorZoneLabel = useEnglish ? "Next TZ" : "下一个 TZ";
  const miniNextTerrorZoneName = nextTerrorZone
    ? translateTerrorZoneAreaName(nextTerrorZone.location_name, useEnglish)
    : terrorZoneStatus === "error"
      ? (useEnglish ? "Unavailable" : "暂不可用")
      : terrorZoneStatus === "empty"
        ? (useEnglish ? "Awaiting forecast" : "等待预报")
        : (useEnglish ? "Syncing" : "同步中");
  const overlayRegionLabel = isStatsOverlay
    ? displayMode === "mini"
      ? (useEnglish
          ? "Statistics overlay, resizable mini click-through mode"
          : "统计悬浮窗，可缩放的迷你穿透模式")
      : (useEnglish ? "Statistics overlay" : "统计悬浮窗")
    : (useEnglish ? "Terror Zone broadcast overlay" : "TZ 播报悬浮窗");
  const overlayModeTitle = displayMode === "mini"
    ? isStatsOverlay
      ? (useEnglish
          ? "Middle-drag to move · Drag an edge to resize · Double-click or press Enter for normal mode"
          : "中键拖动位置 · 拖拽边缘调节大小 · 双击或按 Enter 返回正常模式")
      : (useEnglish ? "Double-click anywhere or press Enter to expand" : "双击任意位置或按 Enter 展开")
    : isStatsOverlay
      ? (useEnglish
          ? "Double-click an empty area or press Enter for mini click-through mode"
          : "双击空白区域或按 Enter 切换迷你穿透模式")
      : (useEnglish ? "Double-click anywhere or press Enter for mini mode" : "双击任意位置或按 Enter 切换迷你模式");
  const timerActionTitle = stats.isTiming
    ? (useEnglish ? "Double-click to end and save this run" : "双击结束当前计时并保存")
    : (useEnglish ? "No active timer" : "当前没有正在进行的计时");

  const noActiveAccountLabel = useEnglish ? "No active accounts" : "无活动账号";
  const averageTimeLabel = useEnglish ? "Average" : "平均";
  const totalRunsLabel = useEnglish ? "Total" : "总计";
  const currentSessionRunsLabel = useEnglish ? "This session" : "本次";
  const runUnitLabel = useEnglish ? "runs" : "场";
  const dropsLabel = useEnglish ? "Drops" : "掉落";
  const emptyDropsLabel = useEnglish ? "None" : "暂无";
  const uniqueDropCountLabel = useEnglish
    ? `${aggregatedDrops.length} types`
    : `${aggregatedDrops.length} 种`;
  const showMoreDropsLabel = useEnglish
    ? `Show ${hiddenDropGroupCount} more`
    : `展开其余 ${hiddenDropGroupCount} 种`;
  const collapseDropsLabel = useEnglish ? "Show latest 5" : "收起至最近 5 种";
  const deleteDropTitle = useEnglish ? "Remove the latest occurrence" : "移除最近一次掉落";
  const dropScopeLabels: Record<DropScope, string> = useEnglish
    ? { current: "Current", previous: "Previous", overview: "Overview" }
    : { current: "当前", previous: "上一把", overview: "总览" };
  const terrorZoneTitle = useEnglish ? "Terror Zone" : "邪恶区域";
  const terrorZoneEmptyMessage = terrorZoneStatus === "error"
    ? (useEnglish ? "Forecast unavailable" : "预报暂不可用")
    : terrorZoneStatus === "empty"
      ? (useEnglish ? "Waiting for next forecast" : "等待下一条预报")
      : (useEnglish ? "Syncing forecast" : "正在同步预报");

  function handleDockPointerEnter() {
    if (isStatsOverlay && displayModeRef.current === "mini") return;
    pointerInsideDockRef.current = true;
    if (dockStateRef.current) void revealDockedOverlay();
  }

  function cycleDropScope(direction: "previous" | "next") {
    const scopes: DropScope[] = ["current", "previous", "overview"];
    const offset = direction === "next" ? 1 : -1;
    const nextIndex = (scopes.indexOf(dropScope) + offset + scopes.length) % scopes.length;
    setDropSlideDirection(direction);
    setDropScope(scopes[nextIndex]);
    setShowAllDropGroups(false);
  }

  function handleDockPointerLeave() {
    if (isStatsOverlay && displayModeRef.current === "mini") return;
    pointerInsideDockRef.current = false;
    if (dockStateRef.current) scheduleDockHide();
  }

  return (
    <div
      className="relative h-screen w-screen overflow-hidden select-none focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-[-3px]"
      style={{
        ...surfaceOpacityVars(config?.overlay_opacity, theme),
      }}
      data-overlay-kind={isStatsOverlay ? "stats" : "tz"}
      data-overlay-mode={displayMode}
      data-dock-edge={dockEdge ?? undefined}
      data-dock-phase={dockPhase ?? undefined}
      role="region"
      tabIndex={supportsCompactMode ? 0 : undefined}
      aria-label={overlayRegionLabel}
      aria-keyshortcuts={supportsCompactMode ? "Enter" : undefined}
      title={supportsCompactMode ? overlayModeTitle : undefined}
      onDoubleClickCapture={supportsCompactMode ? handleOverlayDoubleClick : undefined}
      onPointerDownCapture={handleOverlayWindowPointerDownCapture}
      onPointerMoveCapture={handleOverlayWindowPointerMoveCapture}
      onPointerUpCapture={finishOverlayWindowPointerGesture}
      onPointerCancelCapture={finishOverlayWindowPointerGesture}
      onPointerEnter={handleDockPointerEnter}
      onPointerLeave={handleDockPointerLeave}
    >
      {dockEdge && <div className="overlay-dock-handle" aria-hidden="true" />}
      {!isStatsOverlay && displayMode === "mini" && MINI_OVERLAY_RESIZE_EDGES.map((edge) => (
        <div
          key={edge}
          className="overlay-mini-resize-handle"
          data-mini-resize-edge={edge}
          data-overlay-interactive="true"
          aria-hidden="true"
          onPointerDown={(event) => handleMiniOverlayResizePointerDown(event, edge)}
          onPointerMove={handleMiniOverlayResizePointerMove}
          onPointerUp={finishMiniOverlayResize}
          onPointerCancel={cancelMiniOverlayResizePointerGesture}
          onLostPointerCapture={handleMiniOverlayResizeLostPointerCapture}
        />
      ))}
      <div
        className={`overlay-window flex h-full w-full overflow-hidden ${
          displayMode === "mini"
            ? "overlay-window-mini"
            : "flex-col rounded-xl p-2.5"
        }`}
        style={{
          border: "1px solid var(--border-default)",
          boxShadow: "none",
        }}
      >
      {/* ═══════════════════════════════════════════
          1. 账号胶囊（现有）
          ═══════════════════════════════════════════ */}
      {isStatsOverlay && displayMode === "expanded" && <div
        ref={accountScrollRef}
        className={`overlay-account-scroll flex min-h-0 w-full flex-none flex-nowrap items-center gap-1.5 overflow-x-auto ${
          accountStripScrollable ? "is-scrollable" : ""
        }`}
        data-overlay-account-scroll="true"
        tabIndex={0}
        role="toolbar"
        aria-label={useEnglish ? "Active game accounts" : "运行中的游戏账号"}
        onPointerDown={handleAccountPointerDown}
        onPointerMove={handleAccountPointerMove}
        onPointerUp={finishAccountPointerDrag}
        onPointerCancel={finishAccountPointerDrag}
      >
        {activeAccounts.length > 0 ? (
          activeAccounts.map((a) => {
            const isMonitored =
              config?.rune_audio_enabled && config?.rune_audio_target_account === a.id;
            const displayName = a.display_name || a.id;
            const isFocused = a.id === focusedAccountId;

            const bg = isFocused
              ? "rgba(52,211,153,0.12)"
              : isMonitored
                ? "rgba(200,168,78,0.15)"
                : "var(--surface-hover)";
            const border = isFocused
              ? "1px solid rgba(52,211,153,0.45)"
              : isMonitored
                ? "1px solid rgba(200,168,78,0.3)"
                : "1px solid var(--border-default)";
            const textColor = isFocused
              ? "var(--success)"
              : isMonitored
                ? "var(--accent)"
                : "var(--text-secondary)";
            const dotBg = isFocused
              ? "var(--success)"
              : isMonitored
                ? "var(--accent)"
                : "var(--success)";
            const dotShadow = "none";

            let tooltip = "";
            if (useEnglish) {
              if (isFocused && isMonitored) tooltip = "Monitoring window · Focused · Double-click to switch";
              else if (isFocused) tooltip = "Focused · Double-click to switch";
              else if (isMonitored) tooltip = "Monitoring window · Double-click to focus";
              else tooltip = "Double-click to focus window";
            } else {
              if (isFocused && isMonitored) tooltip = "正在监测窗口 · 当前聚焦 · 双击切换";
              else if (isFocused) tooltip = "当前聚焦 · 双击切换";
              else if (isMonitored) tooltip = "正在监测窗口 · 双击聚焦";
              else tooltip = "双击聚焦窗口";
            }

            return (
              <button
                type="button"
                key={a.id}
                ref={(node) => {
                  if (node) accountButtonRefs.current.set(a.id, node);
                  else accountButtonRefs.current.delete(a.id);
                }}
                className={`overlay-account-pill flex shrink-0 items-center gap-1 whitespace-nowrap rounded-full px-2 text-2xs font-medium
                  cursor-pointer hover:brightness-110 active:scale-95 transition-all duration-150 select-none
                  focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-1`}
                title={tooltip}
                aria-label={useEnglish ? `${displayName}, focus window` : `${displayName}，聚焦窗口`}
                aria-keyshortcuts="Enter Space"
                data-overlay-interactive="true"
                style={
                  {
                    background: bg,
                    color: textColor,
                    border,
                    WebkitAppRegion: "no-drag",
                  } as React.CSSProperties
                }
                onKeyDown={(e) => {
                  if (e.key !== "Enter" && e.key !== " ") return;
                  e.stopPropagation();
                  e.preventDefault();
                  void focusAccountWindow(displayName);
                }}
                onDoubleClick={(e) => {
                  e.stopPropagation();
                  e.preventDefault();
                  void focusAccountWindow(displayName);
                }}
              >
                {isMonitored && (
                  <Eye
                    size={10}
                    className="shrink-0"
                    style={{ opacity: isFocused ? 1 : 0.85 }}
                  />
                )}
                <span
                  className="w-1.5 h-1.5 rounded-full shrink-0"
                  style={{ background: dotBg, boxShadow: dotShadow }}
                />
                {displayName}
              </button>
            );
          })
        ) : (
          <div className="whitespace-nowrap text-xs italic text-text-muted">{noActiveAccountLabel}</div>
        )}
      </div>}


      {isStatsOverlay && displayMode === "mini" ? (
        <div
          className="relative flex h-full min-w-0 flex-1 items-center gap-2"
          aria-label={useEnglish
            ? `${translateOverlaySceneName(stats.currentScene, true)}, ${elapsedDisplay} seconds, ${currentSessionRuns} runs this session`
            : `${translateOverlaySceneName(stats.currentScene, false)}，${elapsedDisplay} 秒，本次 ${currentSessionRuns} 场`}
        >
          <div className="flex min-w-0 flex-1 flex-col justify-center leading-none">
            <span className="text-2xs font-medium text-text-muted">
              {useEnglish ? "Detected scene" : "识别场景"}
            </span>
            <span
              className={`mt-1 truncate text-xs font-semibold ${
                stats.currentTz ? "text-[var(--tz-accent)]" : "text-text-primary"
              }`}
            >
              {translateOverlaySceneName(stats.currentScene, useEnglish)}
            </span>
          </div>
          <span className="h-6 w-px shrink-0 bg-border-default" aria-hidden="true" />
          <div className="flex shrink-0 flex-col items-end justify-center font-mono leading-none tabular-nums">
            <span
              className="text-sm font-bold"
              style={{ color: stats.isTiming ? "var(--success)" : "var(--text-muted)" }}
            >
              {elapsedDisplay}s
            </span>
            <span className="mt-1 text-2xs font-medium text-text-muted">
              {useEnglish ? "Timer" : "计时"}
            </span>
          </div>
          <span className="h-6 w-px shrink-0 bg-border-default" aria-hidden="true" />
          <div className="flex shrink-0 flex-col items-end justify-center leading-none">
            <span className="text-sm font-bold tabular-nums text-accent">{currentSessionRuns}</span>
            <span className="mt-1 text-2xs font-medium text-text-muted">
              {useEnglish ? "Runs" : "场次"}
            </span>
          </div>
          {statsMiniHovered && (
            <div
              className="absolute inset-0 flex flex-col items-center justify-center rounded-md bg-surface-glass px-2 text-center"
              aria-hidden="true"
            >
              <span className="text-xs font-semibold text-text-primary">
                {useEnglish ? "Middle-drag to move · Drag an edge to resize" : "中键拖动位置 · 拖拽边缘调节大小"}
              </span>
              <span className="mt-1 text-2xs font-medium text-text-muted">
                {useEnglish ? "Double-click for normal mode · Click-through stays on" : "双击返回正常模式 · 鼠标穿透保持开启"}
              </span>
            </div>
          )}
        </div>
      ) : !isStatsOverlay && displayMode === "mini" ? (
          <div
            className="overlay-mini-terror-zone flex min-h-0 items-center gap-1.5"
            title={`${miniNextTerrorZoneLabel}: ${miniNextTerrorZoneName}`}
          >
            <span className="overlay-mini-terror-zone-text shrink-0 text-2xs font-semibold text-text-muted">
              {miniNextTerrorZoneLabel}
            </span>
            <span
              className="overlay-mini-terror-zone-text min-w-0 truncate text-xs font-semibold text-[var(--tz-accent)]"
            >
              {miniNextTerrorZoneName}
            </span>
          </div>
      ) : (
      <div className={`flex min-h-0 flex-1 flex-col gap-2 ${isStatsOverlay ? "mt-2.5" : ""}`}>

        {isStatsOverlay && (
          <>
        {/* 场景名称 — 右上角小字 */}
        <div className="flex justify-end px-1">
          <span
            className={`text-sm font-medium truncate max-w-[180px] text-right ${
              stats.currentTz ? "text-[var(--tz-accent)]" : "text-text-secondary"
            }`}
          >
            {translateOverlaySceneName(stats.currentScene, useEnglish)}
          </span>
        </div>

        {/* 计时器 — 大字居中，无背景容器 */}
        <div
          className={`overlay-timer-action flex flex-col items-center rounded-lg py-1.5 focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-1 ${
            stats.isTiming ? "cursor-pointer" : "cursor-default"
          }`}
          data-overlay-interactive="true"
          role="button"
          tabIndex={0}
          aria-disabled={!stats.isTiming}
          aria-label={timerActionTitle}
          aria-keyshortcuts="Enter Space"
          title={timerActionTitle}
          onDoubleClick={handleTimerDoubleClick}
          onKeyDown={handleTimerKeyDown}
        >
          <span
            className="text-xl font-mono font-bold tabular-nums leading-none select-none"
            style={{
              color: stats.isTiming ? "var(--success)" : "var(--text-muted)",
              transition: "color 180ms ease-out",
            }}
          >
            {elapsedDisplay}
          </span>
          <span
            className="text-xs font-mono mt-0.5 select-none"
            style={{
              color: stats.isTiming ? "var(--success)" : "var(--text-muted)",
            }}
          >
            SEC
          </span>
          {avgTime !== null && (
            <div className="flex flex-col items-center mt-1.5 gap-0.5">
              <span
                className="text-xs font-medium select-none"
                style={{ color: "var(--accent)" }}
              >
                {averageTimeLabel} {avgTime.toFixed(1)}s
              </span>
              <span
                className="text-2xs font-medium select-none"
                style={{ color: "var(--text-secondary)", opacity: 0.8 }}
              >
                {totalRunsLabel} {stats.dbTotalRuns} {runUnitLabel} · {currentSessionRunsLabel} {currentSessionRuns} {runUnitLabel}
              </span>
            </div>
          )}
        </div>

        {/* 分隔线 */}
        <div
          className="w-full shrink-0"
          style={{ height: 1, background: "var(--border-default)" }}
        />

        {/* 掉落分组；新识别结果在原胶囊上短时强调 */}
        <div className="flex flex-col gap-1 flex-1 min-h-0">
          <div className="overlay-drop-scope-header flex items-center justify-between gap-2 px-1">
            <div className="overlay-drop-scope-controls" role="group" aria-label={useEnglish ? "Drop range" : "掉落范围"}>
              <button
                type="button"
                data-overlay-interactive="true"
                aria-label={useEnglish ? "Previous drop range" : "查看上一种掉落范围"}
                onClick={(event) => { event.stopPropagation(); cycleDropScope("previous"); }}
              ><ChevronLeft size={11} /></button>
              <span>
                {dropsLabel} · {dropScopeLabels[dropScope]}
                {scopedDrops.length > 0 && <em>({scopedDrops.length})</em>}
              </span>
              <button
                type="button"
                data-overlay-interactive="true"
                aria-label={useEnglish ? "Next drop range" : "查看下一种掉落范围"}
                onClick={(event) => { event.stopPropagation(); cycleDropScope("next"); }}
              ><ChevronRight size={11} /></button>
            </div>
            {aggregatedDrops.length > 0 && (
              <span className="text-2xs text-text-muted tabular-nums">
                {uniqueDropCountLabel}
              </span>
            )}
          </div>

          <span className="sr-only" role="status" aria-live="polite" aria-atomic="true">
            {recentDropAnnouncement}
          </span>

          <div
            key={dropScope}
            className="overlay-drop-scope-content flex flex-wrap gap-1 pr-1 overflow-y-auto content-start"
            data-direction={dropSlideDirection}
            style={{ flex: 1, scrollbarWidth: "thin" }}
          >
            {displayedDropGroups.length > 0 ? (
              <>
                {displayedDropGroups.map(({ key, drop, count, latestIndex }) => {
                  const high = drop.runeNumber !== null && isHighRune(drop.runeNumber);
                  const highlightId = dropScope === "previous" ? undefined : recentDropHighlightIds.get(key);
                  return (
                    <span
                      key={key}
                      className="overlay-drop-pill relative inline-flex items-center overflow-hidden pl-1.5 pr-4 py-0.5 rounded-md text-2xs font-medium
                        transition-all duration-200 hover:brightness-110 group"
                      style={{
                        background: high ? "rgba(255,119,0,0.18)" : "var(--accent-glow)",
                        color: high ? "#ffaa00" : "var(--accent)",
                        border: high
                          ? "1px solid rgba(255,119,0,0.5)"
                          : "1px solid var(--border-strong)",
                        WebkitAppRegion: "no-drag",
                      } as React.CSSProperties}
                    >
                      {highlightId && (
                        <span key={highlightId} className="overlay-drop-pill-flash" aria-hidden="true" />
                      )}
                      <span className="relative z-[1]">{getOverlayDropLabel(drop, useEnglish)}{count > 1 ? ` ×${count}` : ""}</span>
                      {dropScope === "overview" && <button
                        className="absolute right-0.5 top-0 bottom-0 z-[1] flex items-center justify-center w-3 text-gray-400 hover:text-red-500 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 transition-opacity"
                        onClick={(e) => {
                          e.stopPropagation();
                          stats.removeCurrentDrop(latestIndex);
                        }}
                        title={deleteDropTitle}
                      >
                        ×
                      </button>}
                    </span>
                  );
                })}
                {hiddenDropGroupCount > 0 && (
                  <button
                    type="button"
                    className="rounded-md px-1.5 py-0.5 text-2xs font-medium text-text-secondary transition-colors hover:text-text-primary focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                    style={{
                      background: "var(--surface-hover)",
                      border: "1px solid var(--border-default)",
                      WebkitAppRegion: "no-drag",
                    } as React.CSSProperties}
                    onClick={(event) => {
                      event.stopPropagation();
                      setShowAllDropGroups((current) => !current);
                    }}
                    aria-expanded={showAllDropGroups}
                    title={showAllDropGroups ? collapseDropsLabel : showMoreDropsLabel}
                  >
                    {showAllDropGroups ? collapseDropsLabel : showMoreDropsLabel}
                  </button>
                )}
              </>
            ) : (
              <span className="text-2xs text-text-muted italic px-1">
                {emptyDropsLabel}
              </span>
            )}
          </div>
        </div>

          </>
        )}
        {!isStatsOverlay && (
          <section className="tz-expanded-layout flex min-h-0 flex-1 flex-col px-0.5" aria-labelledby="tz-overlay-heading">
            <header className="shrink-0 pb-2">
              <h2 id="tz-overlay-heading" className="text-sm font-bold text-text-primary">
                {terrorZoneTitle}
              </h2>
            </header>

            {hasTerrorZoneData ? (
              <div className="tz-forecast-list min-h-0 flex-1 overflow-y-auto pr-1">
                {currentTerrorZone && (
                  <TerrorZoneInfo
                    current
                    label={currentTerrorZoneLabel}
                    zone={currentTerrorZone}
                    useEnglish={useEnglish}
                  />
                )}
                {currentTerrorZone && nextTerrorZone && (
                  <div className="tz-forecast-divider" aria-hidden="true" />
                )}
                {nextTerrorZone && (
                  <TerrorZoneInfo
                    label={nextTerrorZoneLabel}
                    zone={nextTerrorZone}
                    useEnglish={useEnglish}
                  />
                )}
              </div>
            ) : (
              <div className="flex min-h-0 flex-1 items-center justify-center px-3 text-center">
                <span className="text-xs font-medium text-text-muted">
                  {terrorZoneEmptyMessage}
                </span>
              </div>
            )}
          </section>
        )}
      </div>
      )}
      </div>
    </div>
  );
}
