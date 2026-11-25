import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import type { SyntheticEvent } from "react";

import { useLaunch } from "../../store/launch";
import { AccountGridItem, type GridItemProps } from "./AccountCard";

export function SortableAccountCard({
  account,
  isSelectionMode,
  ...cardProps
}: GridItemProps) {
  const sortingDisabled = !!isSelectionMode || !!cardProps.runtimeStatus?.disabled || !!cardProps.runtimeStatus?.mode;
  const english = cardProps.config?.app_language === "en-US";
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({
    id: account.id,
    disabled: sortingDisabled,
    transition: { duration: 200, easing: "cubic-bezier(.2,.8,.2,1)" },
  });
  const { progress } = useLaunch();
  const canStartDrag = (event: SyntheticEvent<HTMLElement>) => {
    if (sortingDisabled || !(event.target instanceof Element)) return false;
    // Portalled editors also bubble through this component's React tree.
    if (!event.currentTarget.contains(event.target)) return false;
    return !event.target.closest("button, input, select, textarea, a, label, summary, [contenteditable]:not([contenteditable='false']), [role='button'], [role='switch'], [role='checkbox'], [role='combobox'], [data-no-drag]");
  };

  return (
    <div
      ref={setNodeRef}
      {...attributes}
      role="group"
      tabIndex={sortingDisabled ? -1 : 0}
      aria-label={`${account.display_name || account.id} · ${english ? "Drag empty areas to reorder; press Space then arrow keys" : "拖动空白区域排序；键盘按空格后使用方向键移动"}`}
      className="sortable-account-card"
      data-dragging={isDragging || undefined}
      data-sortable-disabled={sortingDisabled || undefined}
      data-sortable-account-id={account.id}
      onMouseDown={event => { if (canStartDrag(event)) listeners?.onMouseDown?.(event); }}
      onTouchStart={event => { if (canStartDrag(event)) listeners?.onTouchStart?.(event); }}
      onKeyDown={event => { if (!sortingDisabled && event.target === event.currentTarget) listeners?.onKeyDown?.(event); }}
      style={{
        transform: CSS.Transform.toString(transform),
        transition,
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
