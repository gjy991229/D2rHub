import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { createPortal } from "react-dom";
import { Check, ChevronDown, Loader2 } from "lucide-react";

import { useAccounts } from "../../store/accounts";
import {
  ACCOUNT_REGION_LABELS,
  INTERNATIONAL_ACCOUNT_REGIONS,
  accountRegionLabel,
  type InternationalAccountRegion,
} from "../../utils/regionPaths";
import { showToast } from "../ui/Toast";

interface AccountRegionSwitcherProps {
  accountId: string;
  currentRegion: string | null | undefined;
  isRunning: boolean;
}

interface MenuPosition {
  left: number;
  top: number;
  opensUpward: boolean;
}

const REGION_MENU_WIDTH = 188;
const REGION_MENU_HEIGHT = 160;
const REGION_MENU_GAP = 6;
const VIEWPORT_PADDING = 8;

export function AccountRegionSwitcher({
  accountId,
  currentRegion,
  isRunning,
}: AccountRegionSwitcherProps) {
  const updateAccountRegion = useAccounts((state) => state.updateAccountRegion);
  const [open, setOpen] = useState(false);
  const [menuPosition, setMenuPosition] = useState<MenuPosition | null>(null);
  const [switchingRegion, setSwitchingRegion] = useState<InternationalAccountRegion | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const menuId = `account-region-${useId().replaceAll(":", "")}`;
  const currentCanonicalRegion = INTERNATIONAL_ACCOUNT_REGIONS.find(
    (region) => region === currentRegion?.trim().toUpperCase(),
  );
  const currentLabel = accountRegionLabel(currentRegion);
  const menuReady = menuPosition !== null;

  const updateMenuPosition = useCallback(() => {
    const trigger = triggerRef.current;
    if (!trigger) return;

    const rect = trigger.getBoundingClientRect();
    const maxLeft = Math.max(
      VIEWPORT_PADDING,
      window.innerWidth - REGION_MENU_WIDTH - VIEWPORT_PADDING,
    );
    const left = Math.min(
      maxLeft,
      Math.max(VIEWPORT_PADDING, rect.left),
    );
    const spaceBelow = window.innerHeight - rect.bottom - VIEWPORT_PADDING;
    const opensUpward = spaceBelow < REGION_MENU_HEIGHT + REGION_MENU_GAP
      && rect.top > REGION_MENU_HEIGHT + REGION_MENU_GAP + VIEWPORT_PADDING;
    const desiredTop = opensUpward
      ? rect.top - REGION_MENU_HEIGHT - REGION_MENU_GAP
      : rect.bottom + REGION_MENU_GAP;
    const maxTop = Math.max(
      VIEWPORT_PADDING,
      window.innerHeight - REGION_MENU_HEIGHT - VIEWPORT_PADDING,
    );
    const top = Math.min(maxTop, Math.max(VIEWPORT_PADDING, desiredTop));

    setMenuPosition({ left, top, opensUpward });
  }, []);

  const closeMenu = useCallback((restoreFocus = false) => {
    setOpen(false);
    setMenuPosition(null);
    if (restoreFocus) {
      window.requestAnimationFrame(() => triggerRef.current?.focus());
    }
  }, []);

  useLayoutEffect(() => {
    if (open) updateMenuPosition();
  }, [open, updateMenuPosition]);

  useEffect(() => {
    if (!open || !menuReady) return;

    const selectedIndex = currentCanonicalRegion
      ? INTERNATIONAL_ACCOUNT_REGIONS.indexOf(currentCanonicalRegion)
      : 0;
    const frame = window.requestAnimationFrame(() => {
      optionRefs.current[Math.max(0, selectedIndex)]?.focus();
    });

    const handleOutsidePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (triggerRef.current?.contains(target) || menuRef.current?.contains(target)) return;
      closeMenu();
    };
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeMenu(true);
    };

    document.addEventListener("pointerdown", handleOutsidePointerDown, true);
    document.addEventListener("keydown", handleEscape);
    window.addEventListener("resize", updateMenuPosition);
    window.addEventListener("scroll", updateMenuPosition, true);
    return () => {
      window.cancelAnimationFrame(frame);
      document.removeEventListener("pointerdown", handleOutsidePointerDown, true);
      document.removeEventListener("keydown", handleEscape);
      window.removeEventListener("resize", updateMenuPosition);
      window.removeEventListener("scroll", updateMenuPosition, true);
    };
  }, [closeMenu, currentCanonicalRegion, menuReady, open, updateMenuPosition]);

  const handleTriggerClick = (event: ReactMouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    setOpen((value) => !value);
  };

  const handleTriggerKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (["ArrowDown", "Enter", " "].includes(event.key)) {
      event.preventDefault();
      event.stopPropagation();
      setOpen(true);
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      event.stopPropagation();
      closeMenu(true);
    }
  };

  const handleListKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const currentIndex = optionRefs.current.findIndex((option) => option === document.activeElement);
    let nextIndex: number | null = null;
    if (event.key === "ArrowDown") nextIndex = (currentIndex + 1) % INTERNATIONAL_ACCOUNT_REGIONS.length;
    if (event.key === "ArrowUp") nextIndex = (currentIndex - 1 + INTERNATIONAL_ACCOUNT_REGIONS.length) % INTERNATIONAL_ACCOUNT_REGIONS.length;
    if (event.key === "Home") nextIndex = 0;
    if (event.key === "End") nextIndex = INTERNATIONAL_ACCOUNT_REGIONS.length - 1;
    if (nextIndex === null) return;

    event.preventDefault();
    event.stopPropagation();
    optionRefs.current[nextIndex]?.focus();
  };

  const switchRegion = async (region: InternationalAccountRegion) => {
    if (switchingRegion) return;
    if (region === currentCanonicalRegion) {
      closeMenu(true);
      return;
    }

    setSwitchingRegion(region);
    try {
      await updateAccountRegion(accountId, region);
      closeMenu(true);
      showToast(
        "success",
        isRunning
          ? `已切换至${ACCOUNT_REGION_LABELS[region]}，当前游戏不变，下次启动生效`
          : `已切换至${ACCOUNT_REGION_LABELS[region]}，下次启动生效`,
      );
    } catch (error) {
      showToast("error", `切换服务器失败: ${error}`);
    } finally {
      setSwitchingRegion(null);
    }
  };

  const stopPointerPropagation = (event: ReactPointerEvent<HTMLElement>) => {
    event.stopPropagation();
  };

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        className="hig-badge hig-badge-neutral account-region-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        aria-label={`${currentLabel}，切换国际服服务器`}
        title="复用现有 Token，切换下次启动服务器"
        onPointerDown={stopPointerPropagation}
        onClick={handleTriggerClick}
        onKeyDown={handleTriggerKeyDown}
      >
        <span>{currentLabel}</span>
        <ChevronDown className="account-region-chevron" size={9} strokeWidth={2} aria-hidden="true" />
      </button>

      {open && menuPosition && createPortal(
        <div
          ref={menuRef}
          id={menuId}
          className="account-region-menu"
          data-placement={menuPosition.opensUpward ? "top" : "bottom"}
          style={{ left: menuPosition.left, top: menuPosition.top }}
          onPointerDown={stopPointerPropagation}
          onClick={(event) => event.stopPropagation()}
        >
          <div className="account-region-menu-header">
            <span>切换服务器</span>
            <span>Token 不变</span>
          </div>
          <div
            role="listbox"
            aria-label="选择国际服服务器"
            aria-busy={switchingRegion !== null}
            onKeyDown={handleListKeyDown}
          >
            {INTERNATIONAL_ACCOUNT_REGIONS.map((region, index) => {
              const selected = region === currentCanonicalRegion;
              const loading = region === switchingRegion;
              return (
                <button
                  key={region}
                  ref={(node) => { optionRefs.current[index] = node; }}
                  type="button"
                  role="option"
                  aria-selected={selected}
                  className="account-region-option"
                  data-selected={selected ? "true" : undefined}
                  disabled={switchingRegion !== null}
                  onClick={(event) => {
                    event.stopPropagation();
                    void switchRegion(region);
                  }}
                >
                  <span className="account-region-option-label">{ACCOUNT_REGION_LABELS[region]}</span>
                  <span className="account-region-option-code">{region}</span>
                  <span className="account-region-option-state" aria-hidden="true">
                    {loading
                      ? <Loader2 size={12} strokeWidth={2} className="animate-spin" />
                      : selected
                        ? <Check size={12} strokeWidth={2.2} />
                        : null}
                  </span>
                </button>
              );
            })}
          </div>
        </div>,
        document.body,
      )}
    </>
  );
}
