import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

import { useLaunch } from "../../store/launch";
import { AccountGridItem, type GridItemProps } from "./AccountCard";

export function SortableAccountCard({
  account,
  isSelectionMode,
  ...cardProps
}: GridItemProps) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({
    id: account.id,
    disabled: isSelectionMode,
  });
  const { progress } = useLaunch();

  return (
    <div
      ref={setNodeRef}
      data-sortable-account-id={account.id}
      style={{
        transform: CSS.Transform.toString(transform),
        transition,
        opacity: isDragging ? 0.5 : 1,
        zIndex: isDragging ? 10 : undefined,
        width: "100%",
      }}
      {...(isSelectionMode ? {} : attributes)}
      {...(isSelectionMode ? {} : listeners)}
      onKeyDown={isSelectionMode ? undefined : event => {
        // Keyboard dragging belongs to the wrapper. Inputs and buttons keep
        // their native Space/Enter behavior even when their events bubble.
        if (event.target !== event.currentTarget) return;
        listeners?.onKeyDown?.(event);
      }}
    >
      <AccountGridItem
        {...cardProps}
        account={account}
        progress={progress[account.id] || null}
        isSelectionMode={isSelectionMode}
      />
    </div>
  );
}
