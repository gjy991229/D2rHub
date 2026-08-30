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
import { AlertTriangle, ChevronDown, ListChecks, Pencil, Play, Plus } from "lucide-react";

import type { AccountMeta, GlobalConfig, LaunchGroup } from "../../store/types";
import { inspectLaunchGroup, type LaunchGroupMemberIssue } from "../../utils/launchGroups";

interface LaunchGroupMenuProps {
  groups: LaunchGroup[];
  accounts: AccountMeta[];
  config: GlobalConfig | null;
  disabled?: boolean;
  onLaunch: (group: LaunchGroup) => void;
  onCreate: () => void;
  onEdit: (group: LaunchGroup) => void;
}

interface MenuPosition {
  left: number;
  top: number;
  opensUpward: boolean;
}

const MENU_WIDTH = 320;
const VIEWPORT_PADDING = 8;

function issueReason(issue: LaunchGroupMemberIssue): string {
  if (issue.reason === "missing") return "账号已删除";
  if (issue.reason === "not_initialized") return "尚未初始化";
  return "需要迁移 Token";
}

function issueDetails(issues: LaunchGroupMemberIssue[]): string {
  return issues.map(issue => `${issue.account_name}：${issueReason(issue)}`).join("；");
}

export function LaunchGroupMenu({
  groups,
  accounts,
  config,
  disabled = false,
  onLaunch,
  onCreate,
  onEdit,
}: LaunchGroupMenuProps) {
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
        <span>启动组</span>
        {groups.length > 0 && <span className="launch-group-trigger-count">{groups.length}</span>}
        <ChevronDown className="launch-group-trigger-chevron" size={11} strokeWidth={2} aria-hidden="true" />
      </button>

      {open && menuPosition && createPortal(
        <div
          ref={menuRef}
          id={menuId}
          role="dialog"
          aria-label="启动组"
          className="launch-group-menu"
          data-placement={menuPosition.opensUpward ? "top" : "bottom"}
          style={{ left: menuPosition.left, top: menuPosition.top }}
          onPointerDown={event => event.stopPropagation()}
          onClick={event => event.stopPropagation()}
        >
          <div className="launch-group-menu-header">
            <div>
              <p>启动组</p>
              <span>按固定组合启动账号</span>
            </div>
            <span>{groups.length} 个启动组</span>
          </div>

          <div className="launch-group-list">
            {groups.length === 0 ? (
              <div className="launch-group-empty">
                <ListChecks size={18} strokeWidth={1.6} aria-hidden="true" />
                <div>
                  <p>还没有启动组</p>
                  <span>创建一个常用账号组合，之后可以直接启动。</span>
                </div>
              </div>
            ) : groups.map(group => {
              const availability = inspectLaunchGroup(group, accounts, config);
              const empty = group.account_ids.length === 0;
              const unavailable = availability.issues.length > 0;
              const status = empty
                ? "尚未选择账号"
                : unavailable
                  ? `${availability.issues.length} 个账号不可用`
                  : `${group.account_ids.length} 个账号`;
              const title = empty ? "请先编辑并选择账号" : issueDetails(availability.issues);
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
                    title={title || `启动“${group.name}”`}
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
                      <strong>{group.name}</strong>
                      <span>{status}</span>
                    </span>
                  </button>
                  <button
                    type="button"
                    className="launch-group-edit"
                    aria-label={`编辑启动组“${group.name}”`}
                    title={`编辑“${group.name}”`}
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
            新建启动组
          </button>
        </div>,
        document.body,
      )}
    </>
  );
}
