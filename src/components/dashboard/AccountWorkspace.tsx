import { Children, useState, type ReactNode } from "react";
import {
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  pointerWithin,
  useDroppable,
  useSensor,
  useSensors,
  type DragCancelEvent,
  type DragEndEvent,
  type DragStartEvent,
  type CollisionDetection,
} from "@dnd-kit/core";
import { sortableKeyboardCoordinates } from "@dnd-kit/sortable";
import { ArchiveRestore, GripVertical } from "lucide-react";
import type { AccountMeta, GlobalConfig } from "../../store/types";
import { AccountGrid } from "./AccountGrid";
import { StandbyPool, STANDBY_POOL_DROP_ID } from "./StandbyPool";

const LAUNCHPAD_DROP_ID = "account-launchpad-drop-zone";

interface AccountWorkspaceProps {
  activeAccounts: AccountMeta[];
  standbyAccounts: AccountMeta[];
  gridAccounts: AccountMeta[];
  config: GlobalConfig | null;
  children: ReactNode;
  isSelectionMode?: boolean;
  disabled?: boolean;
  onReorderActive: (orderedIds: string[]) => void;
  onReorderStandby: (orderedIds: string[]) => void;
  onMoveToStandby: (accountId: string, beforeId?: string | null) => void;
  onMoveToLaunchpad: (accountId: string, beforeId?: string | null) => void;
  onLaunchStandby: (accountId: string) => void;
  onConfigure: (account: AccountMeta) => void;
}

export function AccountWorkspace({
  activeAccounts,
  standbyAccounts,
  gridAccounts,
  config,
  children,
  isSelectionMode,
  disabled,
  onReorderActive,
  onReorderStandby,
  onMoveToStandby,
  onMoveToLaunchpad,
  onLaunchStandby,
  onConfigure,
}: AccountWorkspaceProps) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: isSelectionMode ? 99999 : 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const { setNodeRef: setLaunchpadRef, isOver: launchpadIsOver } = useDroppable({
    id: LAUNCHPAD_DROP_ID,
    data: { container: "active" },
    disabled: isSelectionMode || disabled,
  });
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const activeIds = activeAccounts.map(account => account.id);
  const standbyIds = standbyAccounts.map(account => account.id);
  const gridChildren = Children.toArray(children);
  const childByAccountId = new Map(
    gridAccounts.map((account, index) => [account.id, gridChildren[index]]),
  );
  const activeGridChildren = activeAccounts
    .map(account => childByAccountId.get(account.id));
  const standbyGridChildren = standbyAccounts
    .map(account => childByAccountId.get(account.id));
  const draggingAccount = [...activeAccounts, ...standbyAccounts]
    .find(account => account.id === draggingId) ?? null;
  const draggingFromLaunchpad = !!draggingId && activeIds.includes(draggingId);

  const collisionStrategy: CollisionDetection = args => {
    const pointerCollisions = pointerWithin(args);
    const itemCollision = pointerCollisions.find(collision => {
      const id = String(collision.id);
      return activeIds.includes(id) || standbyIds.includes(id);
    });
    if (itemCollision) return [itemCollision];
    return pointerCollisions.length > 0 ? pointerCollisions : closestCenter(args);
  };

  const containerFor = (id: string): "active" | "standby" | null => {
    if (id === LAUNCHPAD_DROP_ID || activeIds.includes(id)) return "active";
    if (id === STANDBY_POOL_DROP_ID || standbyIds.includes(id)) return "standby";
    return null;
  };

  const handleDragStart = ({ active }: DragStartEvent) => {
    setDraggingId(String(active.id));
  };

  const finishDrag = (_event?: DragCancelEvent) => setDraggingId(null);

  const handleDragEnd = ({ active, over }: DragEndEvent) => {
    setDraggingId(null);
    if (!over || isSelectionMode || disabled) return;
    const activeId = String(active.id);
    const overId = String(over.id);
    const source = containerFor(activeId);
    const target = containerFor(overId);
    if (!source || !target) return;

    if (source === "active" && target === "standby") {
      onMoveToStandby(activeId, standbyIds.includes(overId) ? overId : null);
      return;
    }
    if (source === "standby" && target === "active") {
      onMoveToLaunchpad(activeId, activeIds.includes(overId) ? overId : null);
      return;
    }
    if (source === "active" && target === "active" && activeId !== overId) {
      const next = [...activeIds];
      const oldIndex = next.indexOf(activeId);
      const newIndex = next.indexOf(overId);
      if (oldIndex >= 0 && newIndex >= 0) {
        const [moved] = next.splice(oldIndex, 1);
        next.splice(newIndex, 0, moved);
        onReorderActive(next);
      }
      return;
    }
    if (source === "standby" && target === "standby" && activeId !== overId && standbyIds.includes(overId)) {
      const next = [...standbyIds];
      const oldIndex = next.indexOf(activeId);
      const newIndex = next.indexOf(overId);
      if (oldIndex >= 0 && newIndex >= 0) {
        const [moved] = next.splice(oldIndex, 1);
        next.splice(newIndex, 0, moved);
        onReorderStandby(next);
      }
    }
  };

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={collisionStrategy}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onDragCancel={finishDrag}
    >
      <div className="account-workspace">
        <div
          ref={setLaunchpadRef}
          className="account-workspace-launchpad"
          data-drop-active={launchpadIsOver && !!draggingId ? "true" : undefined}
        >
          {isSelectionMode ? (
            <div className="scheme-account-sections">
              <section className="scheme-account-section" aria-labelledby="scheme-active-accounts-heading">
                <header className="scheme-account-section-head">
                  <div>
                    <strong id="scheme-active-accounts-heading">启动台账号</strong>
                    <span>{activeAccounts.length}</span>
                  </div>
                  <small>默认启动会使用这里的账号</small>
                </header>
                {activeAccounts.length > 0 ? (
                  <AccountGrid accounts={activeAccounts} isSelectionMode embedded>
                    {activeGridChildren}
                  </AccountGrid>
                ) : (
                  <div className="scheme-account-section-empty">当前没有启动台账号</div>
                )}
              </section>

              <section className="scheme-account-section scheme-account-section-standby" aria-labelledby="scheme-standby-accounts-heading">
                <header className="scheme-account-section-head">
                  <div>
                    <strong id="scheme-standby-accounts-heading">待机账号</strong>
                    <span>{standbyAccounts.length}</span>
                  </div>
                  <small>默认启动时跳过，当前策略仍可正常使用</small>
                </header>
                {standbyAccounts.length > 0 ? (
                  <AccountGrid accounts={standbyAccounts} isSelectionMode embedded>
                    {standbyGridChildren}
                  </AccountGrid>
                ) : (
                  <div className="scheme-account-section-empty">当前没有待机账号</div>
                )}
              </section>
            </div>
          ) : gridAccounts.length > 0 ? (
            <AccountGrid
              accounts={gridAccounts}
              isDropTarget={launchpadIsOver && !!draggingId}
            >
              {children}
            </AccountGrid>
          ) : (
            <div className="launchpad-empty-state">
              <ArchiveRestore size={18} strokeWidth={1.7} aria-hidden="true" />
              <strong>启动台为空</strong>
              <span>从底部待机 Dock 拖出账号，或检视账号后点击“移到启动台”</span>
            </div>
          )}
        </div>

        {!isSelectionMode && (
          <StandbyPool
            accounts={standbyAccounts}
            config={config}
            onMoveToLaunchpad={onMoveToLaunchpad}
            onLaunch={onLaunchStandby}
            onConfigure={onConfigure}
            disabled={disabled}
            isDraggingFromLaunchpad={draggingFromLaunchpad}
          />
        )}
      </div>

      <DragOverlay
        dropAnimation={{ duration: 180, easing: "cubic-bezier(0.16, 1, 0.3, 1)" }}
      >
        {draggingAccount ? (
          <div className="account-workspace-drag-preview">
            <GripVertical size={14} strokeWidth={1.8} aria-hidden="true" />
            <span>{draggingAccount.display_name || draggingAccount.id}</span>
          </div>
        ) : null}
      </DragOverlay>
    </DndContext>
  );
}
