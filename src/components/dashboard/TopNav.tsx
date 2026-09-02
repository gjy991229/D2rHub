
import {
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { Settings, Info, Minus, BookOpen, BarChart3, CalendarDays, CalendarRange, Check, Share2, Sun } from "lucide-react";

import type { BattleReportQuickRange } from "../../utils/battleReport";

const SHARE_MENU_WIDTH = 224;
const SHARE_MENU_HEIGHT = 174;
const VIEWPORT_PADDING = 8;

const SHARE_RANGES: Array<{
  range: BattleReportQuickRange;
  label: string;
  detail: string;
  icon: typeof Sun;
}> = [
  { range: "today", label: "每日战报", detail: "今天 00:00 至现在", icon: Sun },
  { range: "week", label: "本周战报", detail: "本周一至现在", icon: CalendarDays },
  { range: "month", label: "本月战报", detail: "本月 1 日至现在", icon: CalendarRange },
];

export function TopNav({
  onAbout, onExit, onOpenConfig, onHelp, onStats, statsModuleInstalled = false,
  onShareReport, sharingReport,
}: {
  onAbout: () => void;
  onExit: () => void;
  onOpenConfig: () => void;
  onHelp: () => void;
  onStats: () => void;
  statsModuleInstalled?: boolean;
  onShareReport: (range: BattleReportQuickRange) => void;
  sharingReport: boolean;
}) {
  const [shareMenuOpen, setShareMenuOpen] = useState(false);
  const [shareMenuPosition, setShareMenuPosition] = useState<{ left: number; top: number; opensUpward: boolean } | null>(null);
  const [lastRange, setLastRange] = useState<BattleReportQuickRange>("today");
  const shareTriggerRef = useRef<HTMLButtonElement | null>(null);
  const shareMenuRef = useRef<HTMLDivElement | null>(null);
  const shareMenuId = `share-report-${useId().replaceAll(":", "")}`;

  const updateShareMenuPosition = useCallback(() => {
    const trigger = shareTriggerRef.current;
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const roomBelow = window.innerHeight - rect.bottom - VIEWPORT_PADDING;
    const opensUpward = roomBelow < SHARE_MENU_HEIGHT && rect.top > roomBelow;
    const desiredTop = opensUpward ? rect.top - SHARE_MENU_HEIGHT - 5 : rect.bottom + 5;
    const left = Math.min(
      Math.max(VIEWPORT_PADDING, window.innerWidth - SHARE_MENU_WIDTH - VIEWPORT_PADDING),
      Math.max(VIEWPORT_PADDING, rect.right - SHARE_MENU_WIDTH),
    );
    const top = Math.min(
      Math.max(VIEWPORT_PADDING, window.innerHeight - SHARE_MENU_HEIGHT - VIEWPORT_PADDING),
      Math.max(VIEWPORT_PADDING, desiredTop),
    );
    setShareMenuPosition({ left, top, opensUpward });
  }, []);

  const closeShareMenu = useCallback((restoreFocus = false) => {
    setShareMenuOpen(false);
    setShareMenuPosition(null);
    if (restoreFocus) window.requestAnimationFrame(() => shareTriggerRef.current?.focus());
  }, []);

  useEffect(() => {
    if (!statsModuleInstalled) closeShareMenu();
  }, [closeShareMenu, statsModuleInstalled]);

  useLayoutEffect(() => {
    if (shareMenuOpen) updateShareMenuPosition();
  }, [shareMenuOpen, updateShareMenuPosition]);

  useEffect(() => {
    if (!shareMenuOpen || !shareMenuPosition) return;
    const frame = window.requestAnimationFrame(() => {
      shareMenuRef.current?.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus();
    });
    const handleOutsidePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (shareTriggerRef.current?.contains(target) || shareMenuRef.current?.contains(target)) return;
      closeShareMenu();
    };
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeShareMenu(true);
    };
    document.addEventListener("pointerdown", handleOutsidePointerDown, true);
    document.addEventListener("keydown", handleEscape);
    window.addEventListener("resize", updateShareMenuPosition);
    window.addEventListener("scroll", updateShareMenuPosition, true);
    return () => {
      window.cancelAnimationFrame(frame);
      document.removeEventListener("pointerdown", handleOutsidePointerDown, true);
      document.removeEventListener("keydown", handleEscape);
      window.removeEventListener("resize", updateShareMenuPosition);
      window.removeEventListener("scroll", updateShareMenuPosition, true);
    };
  }, [closeShareMenu, shareMenuOpen, shareMenuPosition, updateShareMenuPosition]);

  useEffect(() => {
    if (sharingReport && shareMenuOpen) closeShareMenu();
  }, [closeShareMenu, shareMenuOpen, sharingReport]);

  const handleShareTriggerClick = (event: ReactMouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    setShareMenuOpen(value => !value);
  };

  const handleShareTriggerKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (["ArrowDown", "Enter", " "].includes(event.key)) {
      event.preventDefault();
      setShareMenuOpen(true);
    } else if (event.key === "Escape" && shareMenuOpen) {
      event.preventDefault();
      closeShareMenu(true);
    }
  };

  const handleShareMenuKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const items = [...(shareMenuRef.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') || [])];
    if (!items.length) return;
    const currentIndex = Math.max(0, items.indexOf(document.activeElement as HTMLButtonElement));
    const nextIndex = event.key === "ArrowDown"
      ? (currentIndex + 1) % items.length
      : event.key === "ArrowUp"
        ? (currentIndex - 1 + items.length) % items.length
        : event.key === "Home"
          ? 0
          : event.key === "End"
            ? items.length - 1
            : null;
    if (nextIndex === null) return;
    event.preventDefault();
    items[nextIndex]?.focus();
  };

  return (
    <>
      <div
        className="top-nav-material shrink-0 flex items-center px-4 select-none"
        data-tauri-drag-region
      >
        <div className="flex items-center gap-2.5 mr-5" data-tauri-drag-region>
          <span className="swiss-mark">
            <img src="/logo.png" alt="D2RHub" className="w-[18px] h-[18px] object-contain opacity-90" />
          </span>
          <span className="text-xs font-semibold text-text-secondary tracking-normal h-5 flex items-center">
            D2RHub
          </span>
        </div>

        <div className="flex-1" data-tauri-drag-region />

        <div className="flex items-center gap-1.5">
          <button
            onClick={onOpenConfig}
            className="icon-btn w-7 h-7"
            title="配置"
          >
            <Settings size={14} strokeWidth={1.8} />
          </button>
          {statsModuleInstalled && (
            <>
              <button onClick={onStats}
                className="icon-btn w-7 h-7" title="查看统计">
                <BarChart3 size={14} strokeWidth={1.8} />
              </button>
              <button
                ref={shareTriggerRef}
                type="button"
                onClick={handleShareTriggerClick}
                onKeyDown={handleShareTriggerKeyDown}
                className="icon-btn w-7 h-7 disabled:cursor-wait disabled:opacity-40"
                title={sharingReport ? "正在生成战报" : "选择战报周期并复制图片"}
                aria-label={sharingReport ? "正在生成战报" : "选择战报周期"}
                aria-haspopup="menu"
                aria-expanded={shareMenuOpen}
                aria-controls={shareMenuOpen ? shareMenuId : undefined}
                aria-busy={sharingReport}
                disabled={sharingReport}
              >
                <Share2 size={14} strokeWidth={1.8} />
              </button>
            </>
          )}
          <button onClick={onHelp}
            className="icon-btn w-7 h-7" title="帮助文档">
            <BookOpen size={14} strokeWidth={1.8} />
          </button>
          <button onClick={onAbout}
            className="icon-btn w-7 h-7" title="关于">
            <Info size={14} strokeWidth={1.8} />
          </button>
          <button onClick={onExit}
            className="icon-btn w-7 h-7 hover:!text-text-primary" title="最小化到托盘">
            <Minus size={15} strokeWidth={1.8} />
          </button>
        </div>
      </div>

      {statsModuleInstalled && shareMenuOpen && shareMenuPosition && createPortal(
        <div
          ref={shareMenuRef}
          id={shareMenuId}
          role="menu"
          aria-label="选择战报周期"
          className="share-report-menu"
          data-placement={shareMenuPosition.opensUpward ? "top" : "bottom"}
          style={{ left: shareMenuPosition.left, top: shareMenuPosition.top }}
          onPointerDown={event => event.stopPropagation()}
          onClick={event => event.stopPropagation()}
          onKeyDown={handleShareMenuKeyDown}
        >
          <div className="share-report-menu-heading">复制图片战报</div>
          {SHARE_RANGES.map(option => {
            const Icon = option.icon;
            const selected = option.range === lastRange;
            return (
              <button
                key={option.range}
                type="button"
                role="menuitem"
                className="share-report-option"
                onClick={() => {
                  setLastRange(option.range);
                  closeShareMenu();
                  onShareReport(option.range);
                }}
              >
                <span className="share-report-option-icon" aria-hidden="true">
                  <Icon size={14} strokeWidth={1.8} />
                </span>
                <span className="share-report-option-copy">
                  <strong>{option.label}</strong>
                  <span>{option.detail}</span>
                </span>
                {selected && <Check className="share-report-option-check" size={13} strokeWidth={2} aria-hidden="true" />}
              </button>
            );
          })}
        </div>,
        document.body,
      )}
    </>
  );
}
