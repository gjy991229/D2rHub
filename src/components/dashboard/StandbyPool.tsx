import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
  type WheelEvent,
} from "react";
import { ArchiveRestore, Play, Settings2 } from "lucide-react";
import { useDroppable } from "@dnd-kit/core";
import { SortableContext, horizontalListSortingStrategy, useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import type { AccountMeta, GlobalConfig } from "../../store/types";
import { accountRegionLabel, requiresTokenMigration } from "../../utils/regionPaths";

export const STANDBY_POOL_DROP_ID = "standby-pool-drop-zone";

interface StandbyPoolProps {
  accounts: AccountMeta[];
  config: GlobalConfig | null;
  onMoveToLaunchpad: (accountId: string) => void;
  onLaunch: (accountId: string) => void;
  onConfigure: (account: AccountMeta) => void;
  disabled?: boolean;
  isDraggingFromLaunchpad?: boolean;
}

interface PreviewState {
  accountId: string;
  anchorX: number;
}

function lastLaunchLabel(account: AccountMeta): string {
  if (!account.last_launched_at) return account.initialized ? "已就绪" : "待配置";
  const launched = new Date(account.last_launched_at);
  if (Number.isNaN(launched.getTime())) return "已启动过";
  return launched.toLocaleDateString("zh-CN", { month: "numeric", day: "numeric" });
}

function accountStateLabel(account: AccountMeta): string {
  if (account.is_running) return "运行中";
  return account.initialized ? "待机" : "待配置";
}

function StandbyPreviewCard({
  account,
  config,
  onMoveToLaunchpad,
  onLaunch,
  onConfigure,
  onKeepOpen,
  onRequestClose,
}: {
  account: AccountMeta;
  config: GlobalConfig | null;
  onMoveToLaunchpad: (accountId: string) => void;
  onLaunch: (accountId: string) => void;
  onConfigure: (account: AccountMeta) => void;
  onKeepOpen: () => void;
  onRequestClose: () => void;
}) {
  const displayName = account.display_name || account.id;
  const launchable = account.initialized
    && !requiresTokenMigration(account.auth_mode, account.region, config);
  const authBadgeClass = account.auth_mode === "token"
    ? "hig-badge hig-badge-violet"
    : "hig-badge hig-badge-blue";
  const authLabel = account.auth_mode === "token"
    ? "网页 Token"
    : account.auth_mode
      ? "战网认证"
      : "待配置";
  const stopPointer = (event: PointerEvent<HTMLButtonElement>) => event.stopPropagation();
  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    onRequestClose();
  };

  return (
    <article
      className="spatial-tile standby-preview-card"
      data-running={account.is_running ? "true" : undefined}
      aria-label={`待机账号 ${displayName}`}
      onPointerEnter={onKeepOpen}
      onPointerLeave={onRequestClose}
      onFocusCapture={onKeepOpen}
      onBlurCapture={onRequestClose}
      onKeyDown={handleKeyDown}
    >
      <div className="standby-preview-head">
        <span className="tile-index">{accountStateLabel(account)}</span>
        <span
          className={`standby-dock-state ${account.initialized ? "ready" : "warn"}`}
          aria-hidden="true"
        />
      </div>

      <strong className="tile-name" title={displayName}>{displayName}</strong>

      <div className="standby-preview-tags">
        <span className="hig-badge hig-badge-neutral">{lastLaunchLabel(account)}</span>
        <span className="hig-badge hig-badge-neutral">
          {accountRegionLabel(account.region) || "未选区服"}
        </span>
        <span className={authBadgeClass}>{authLabel}</span>
      </div>

      <div className="standby-preview-actions">
        <button
          type="button"
          className="icon-button compact"
          title="账号设置"
          aria-label={`设置账号 ${displayName}`}
          onPointerDown={stopPointer}
          onClick={() => onConfigure(account)}
        >
          <Settings2 size={13} strokeWidth={1.8} aria-hidden="true" />
        </button>
        <button
          type="button"
          className="control-btn"
          disabled={!launchable || account.is_running}
          onPointerDown={stopPointer}
          onClick={() => onLaunch(account.id)}
        >
          <Play size={12} strokeWidth={1.9} aria-hidden="true" />
          启动
        </button>
        <button
          type="button"
          className="control-btn"
          onPointerDown={stopPointer}
          onClick={() => onMoveToLaunchpad(account.id)}
        >
          <ArchiveRestore size={12} strokeWidth={1.9} aria-hidden="true" />
          移到启动台
        </button>
      </div>
    </article>
  );
}

function SortableStandbyDockItem({
  account,
  inspected,
  disabled,
  onInspect,
  onRequestClose,
  onDismiss,
}: {
  account: AccountMeta;
  inspected: boolean;
  disabled?: boolean;
  onInspect: (accountId: string, node: HTMLElement) => void;
  onRequestClose: () => void;
  onDismiss: () => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: account.id,
    data: { container: "standby" },
    disabled,
  });
  const itemRef = useRef<HTMLDivElement | null>(null);
  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  } as CSSProperties;
  const displayName = account.display_name || account.id;
  const regionLabel = accountRegionLabel(account.region) || "未选区服";

  useEffect(() => {
    if (isDragging) onDismiss();
  }, [isDragging, onDismiss]);

  const setItemNode = (node: HTMLDivElement | null) => {
    itemRef.current = node;
    setNodeRef(node);
  };
  const inspect = () => {
    if (itemRef.current && !isDragging) onInspect(account.id, itemRef.current);
  };
  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (isDragging) {
      listeners?.onKeyDown?.(event);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      inspect();
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onDismiss();
      return;
    }
    listeners?.onKeyDown?.(event);
  };

  return (
    <div
      ref={setItemNode}
      className="standby-dock-slot"
      data-dragging={isDragging ? "true" : undefined}
      style={style}
      role="listitem"
    >
      <div
        className="standby-dock-item"
        data-inspected={inspected ? "true" : undefined}
        data-running={account.is_running ? "true" : undefined}
        data-ready={account.initialized ? "true" : undefined}
        {...attributes}
        {...listeners}
        role="button"
        tabIndex={0}
        aria-expanded={inspected}
        aria-label={`${displayName}，${regionLabel}，${accountStateLabel(account)}。拖动调整顺序，点击查看账号`}
        onPointerEnter={inspect}
        onPointerLeave={onRequestClose}
        onFocus={inspect}
        onBlur={onRequestClose}
        onClick={inspect}
        onKeyDown={handleKeyDown}
      >
        <span
          className={`standby-dock-state ${account.initialized ? "ready" : "warn"}`}
          aria-hidden="true"
        />
        <strong title={displayName}>{displayName}</strong>
        <small>{regionLabel}</small>
      </div>
    </div>
  );
}

export function StandbyPool({
  accounts,
  config,
  onMoveToLaunchpad,
  onLaunch,
  onConfigure,
  disabled,
  isDraggingFromLaunchpad,
}: StandbyPoolProps) {
  const { setNodeRef, isOver } = useDroppable({
    id: STANDBY_POOL_DROP_ID,
    data: { container: "standby" },
    disabled,
  });
  const dockRef = useRef<HTMLElement | null>(null);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [preview, setPreview] = useState<PreviewState | null>(null);

  const cancelClose = useCallback(() => {
    if (closeTimer.current) clearTimeout(closeTimer.current);
    closeTimer.current = null;
  }, []);
  const dismissPreview = useCallback(() => {
    cancelClose();
    setPreview(null);
  }, [cancelClose]);
  const requestClose = useCallback(() => {
    cancelClose();
    closeTimer.current = setTimeout(() => setPreview(null), 120);
  }, [cancelClose]);
  const inspectAccount = (accountId: string, node: HTMLElement) => {
    cancelClose();
    const dockBounds = dockRef.current?.getBoundingClientRect();
    const itemBounds = node.getBoundingClientRect();
    const anchorX = dockBounds
      ? itemBounds.left + itemBounds.width / 2 - dockBounds.left
      : itemBounds.left + itemBounds.width / 2;
    setPreview({ accountId, anchorX });
  };
  const setDockNode = (node: HTMLElement | null) => {
    dockRef.current = node;
    setNodeRef(node);
  };

  useEffect(() => () => cancelClose(), []);
  useEffect(() => {
    if (preview && !accounts.some(account => account.id === preview.accountId)) {
      setPreview(null);
    }
  }, [accounts, preview]);

  const previewAccount = preview
    ? accounts.find(account => account.id === preview.accountId) ?? null
    : null;
  const dropActive = isOver || isDraggingFromLaunchpad;
  const scrollCards = (event: WheelEvent<HTMLDivElement>) => {
    if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
    event.currentTarget.scrollLeft += event.deltaY;
    event.preventDefault();
  };
  const previewStyle = preview
    ? ({ "--standby-preview-x": `${preview.anchorX}px` } as CSSProperties)
    : undefined;

  return (
    <section
      ref={setDockNode}
      className="standby-dock"
      data-drop-active={dropActive ? "true" : undefined}
      aria-label="全局待机账号池"
    >
      {previewAccount && !disabled && (
        <div className="standby-preview-anchor" style={previewStyle}>
          <StandbyPreviewCard
            account={previewAccount}
            config={config}
            onMoveToLaunchpad={onMoveToLaunchpad}
            onLaunch={onLaunch}
            onConfigure={onConfigure}
            onKeepOpen={cancelClose}
            onRequestClose={requestClose}
          />
        </div>
      )}

      <div className="standby-dock-shell" data-empty={accounts.length === 0 ? "true" : undefined}>
        <div className="standby-dock-label" title="这些账号会在默认启动时被跳过">
          <strong>{isOver ? "松开以待机" : "待机"}</strong>
          {!isOver && <span>{accounts.length}</span>}
        </div>

        {accounts.length === 0 ? (
          <div className="standby-dock-empty">将暂时不用的账号拖到这里</div>
        ) : (
          <div className="standby-dock-viewport" onWheel={scrollCards}>
            <SortableContext items={accounts.map(account => account.id)} strategy={horizontalListSortingStrategy}>
              <div className="standby-dock-rail" role="list" aria-label="待机账号">
                {accounts.map(account => (
                  <SortableStandbyDockItem
                    key={account.id}
                    account={account}
                    inspected={preview?.accountId === account.id}
                    disabled={disabled}
                    onInspect={inspectAccount}
                    onRequestClose={requestClose}
                    onDismiss={dismissPreview}
                  />
                ))}
              </div>
            </SortableContext>
          </div>
        )}
      </div>
    </section>
  );
}
