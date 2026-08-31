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
import { AlertTriangle, ChevronDown, ListChecks, Pencil, Play, Plus, Star } from "lucide-react";

import type { AccountMeta, GlobalConfig, LaunchGroup } from "../../store/types";
import {
  inspectLaunchGroup,
  launchGroupAccountIds,
  launchGroupIssueDetails,
} from "../../utils/launchGroups";
import { useI18n } from "../../i18n";

interface LaunchGroupMenuProps {
  groups: LaunchGroup[];
  accounts: AccountMeta[];
  config: GlobalConfig | null;
  favoriteGroupIds?: string[];
  disabled?: boolean;
  onLaunch: (group: LaunchGroup) => void;
  onCreate: () => void;
  onEdit: (group: LaunchGroup) => void;
  onToggleFavorite: (group: LaunchGroup) => void;
}

interface MenuPosition {
  left: number;
  top: number;
  opensUpward: boolean;
}

const MENU_WIDTH = 320;
const VIEWPORT_PADDING = 8;

export function LaunchGroupMenu({
  groups,
  accounts,
  config,
  favoriteGroupIds = [],
  disabled = false,
  onLaunch,
  onCreate,
  onEdit,
  onToggleFavorite,
}: LaunchGroupMenuProps) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [menuPosition, setMenuPosition] = useState<MenuPosition | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const menuId = `launch-groups-${useId().replaceAll(":", "")}`;

  const updateMenuPosition = useCallback(() => {
    const trigger = triggerRef.current;
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const estimatedHeight = Math.min(356, 86 + Math.max(1, groups.length) * 58);
    const roomBelow = window.innerHeight - rect.bottom - VIEWPORT_PADDING;
    const opensUpward = roomBelow < estimatedHeight && rect.top > roomBelow;
    const desiredTop = opensUpward ? rect.top - estimatedHeight - 5 : rect.bottom + 5;
    const maxLeft = Math.max(VIEWPORT_PADDING, window.innerWidth - MENU_WIDTH - VIEWPORT_PADDING);
    const left = Math.min(maxLeft, Math.max(VIEWPORT_PADDING, rect.left));
    const maxTop = Math.max(VIEWPORT_PADDING, window.innerHeight - estimatedHeight - VIEWPORT_PADDING);
    const top = Math.min(maxTop, Math.max(VIEWPORT_PADDING, desiredTop));
    setMenuPosition({ left, top, opensUpward });
  }, [groups.length]);

  const closeMenu = useCallback((restoreFocus = false) => {
    setOpen(false);
    setMenuPosition(null);
    if (restoreFocus) window.requestAnimationFrame(() => triggerRef.current?.focus());
  }, []);

  useLayoutEffect(() => {
    if (open) updateMenuPosition();
  }, [open, updateMenuPosition]);

  useEffect(() => {
    if (!open || !menuPosition) return;
    const frame = window.requestAnimationFrame(() => {
      menuRef.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
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
  }, [closeMenu, menuPosition, open, updateMenuPosition]);

  useEffect(() => {
    if (disabled && open) closeMenu();
  }, [closeMenu, disabled, open]);

  const handleTriggerClick = (event: ReactMouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    setOpen(value => !value);
  };

  const handleTriggerKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (["ArrowDown", "Enter", " "].includes(event.key)) {
      event.preventDefault();
      setOpen(true);
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      closeMenu(true);
    }
  };

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        className="control-btn launch-group-trigger min-w-[86px]"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        disabled={disabled}
        onClick={handleTriggerClick}
        onKeyDown={handleTriggerKeyDown}
      >
        <ListChecks size={13} strokeWidth={1.9} aria-hidden="true" />
        <span>{t("launch.scheme.label")}</span>
        {groups.length > 0 && <span className="launch-group-trigger-count">{groups.length}</span>}
        <ChevronDown className="launch-group-trigger-chevron" size={11} strokeWidth={2} aria-hidden="true" />
      </button>

      {open && menuPosition && createPortal(
        <div
          ref={menuRef}
          id={menuId}
          role="dialog"
          aria-label={t("launch.scheme.label")}
          className="launch-group-menu"
          data-placement={menuPosition.opensUpward ? "top" : "bottom"}
          style={{ left: menuPosition.left, top: menuPosition.top }}
          onPointerDown={event => event.stopPropagation()}
          onClick={event => event.stopPropagation()}
        >
          <div className="launch-group-menu-header">
            <div>
              <p>{t("launch.scheme.label")}</p>
              <span>{t("launch.scheme.subtitle")}</span>
            </div>
            <span>{t("launch.scheme.count", { count: groups.length })}</span>
          </div>

          <div className="launch-group-list">
            {groups.length === 0 ? (
              <div className="launch-group-empty">
                <ListChecks size={18} strokeWidth={1.6} aria-hidden="true" />
                <div>
                  <p>{t("launch.scheme.empty.title")}</p>
                  <span>{t("launch.scheme.empty.body")}</span>
                </div>
              </div>
            ) : groups.map(group => {
              const availability = inspectLaunchGroup(group, accounts, config);
              const memberCount = launchGroupAccountIds(group).length;
              const empty = memberCount === 0;
              const unavailable = availability.issues.length > 0;
              const status = empty
                ? t("launch.scheme.status.empty")
                : unavailable
                  ? t("launch.scheme.status.unavailable", { count: availability.issues.length })
                  : t("launch.scheme.status.ready", { count: memberCount });
              const title = empty ? t("launch.scheme.editFirst") : launchGroupIssueDetails(availability.issues);
              const isFavorite = favoriteGroupIds.includes(group.id);
              return (
                <div
                  key={group.id}
                  className="launch-group-row"
                  data-warning={!availability.can_launch ? "true" : undefined}
                >
                  <button
                    type="button"
                    className="launch-group-launch"
                    disabled={!availability.can_launch}
                    title={title || t("launch.scheme.launchTitle", { name: group.name })}
                    onClick={() => {
                      closeMenu();
                      onLaunch(group);
                    }}
                  >
                    <span className="launch-group-play" aria-hidden="true">
                      {availability.can_launch
                        ? <Play size={12} fill="currentColor" strokeWidth={1.7} />
                        : <AlertTriangle size={12} strokeWidth={1.8} />}
                    </span>
                    <span className="launch-group-copy">
                      <strong data-i18n-skip>{group.name}</strong>
                      <span>{status}</span>
                    </span>
                  </button>
                  <button
                    type="button"
                    className="launch-group-favorite"
                    data-active={isFavorite ? "true" : undefined}
                    aria-label={t(
                      isFavorite ? "launch.favorite.removeLabel" : "launch.favorite.addLabel",
                      { name: group.name },
                    )}
                    title={t(isFavorite ? "launch.favorite.removeTitle" : "launch.favorite.addTitle")}
                    onClick={() => onToggleFavorite(group)}
                  >
                    <Star
                      size={13}
                      fill={isFavorite ? "currentColor" : "none"}
                      strokeWidth={1.8}
                      aria-hidden="true"
                    />
                  </button>
                  <button
                    type="button"
                    className="launch-group-edit"
                    aria-label={t("launch.scheme.editLabel", { name: group.name })}
                    title={t("launch.scheme.editTitle", { name: group.name })}
                    onClick={() => {
                      closeMenu();
                      onEdit(group);
                    }}
                  >
                    <Pencil size={13} strokeWidth={1.8} aria-hidden="true" />
                  </button>
                </div>
              );
            })}
          </div>

          <button
            type="button"
            className="launch-group-create"
            onClick={() => {
              closeMenu();
              onCreate();
            }}
          >
            <Plus size={13} strokeWidth={2} aria-hidden="true" />
            {t("launch.scheme.create")}
          </button>
        </div>,
        document.body,
      )}
    </>
  );
}
