import React from "react";
import { Zap } from "lucide-react";
import type { AccountMeta } from "../../store/types";
import {
  DndContext, closestCenter, PointerSensor, useSensor, useSensors, type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext, rectSortingStrategy,
} from "@dnd-kit/sortable";

interface AccountGridProps {
  accounts: AccountMeta[];
  onReorder: (orderedIds: string[]) => Promise<void>;
  children: React.ReactNode;
  isSelectionMode?: boolean;
}

export function AccountGrid({ accounts, onReorder, children, isSelectionMode }: AccountGridProps) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: isSelectionMode ? 99999 : 5 } })
  );

  const handleDragEnd = (event: DragEndEvent) => {
    if (isSelectionMode) return;
    const { active, over } = event;
    if (over && active.id !== over.id) {
      const oldIndex = accounts.findIndex(a => a.id === active.id);
      const newIndex = accounts.findIndex(a => a.id === over.id);
      if (oldIndex === -1 || newIndex === -1) return;
      const reordered = [...accounts];
      const [moved] = reordered.splice(oldIndex, 1);
      reordered.splice(newIndex, 0, moved);
      onReorder(reordered.map(a => a.id));
    }
  };

  return (
    <div className="flex-1 overflow-auto px-5 pb-5">
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
        <SortableContext items={accounts.map(a => a.id)} strategy={rectSortingStrategy}>
          <div className={`spatial-grid ${isSelectionMode ? "scheme-spatial-grid" : ""}`}>
            {children}
          </div>
        </SortableContext>
      </DndContext>
    </div>
  );
}

export function AccountGridLoading() {
  return (
    <div className="flex-1 overflow-auto px-5 pb-5">
      <div className="spatial-grid">
        {[1,2,3,4,5,6].map(i => (
          <div key={i} className="skeleton rounded-[22px] h-[172px]" />
        ))}
      </div>
    </div>
  );
}

export function AccountGridEmpty({ onAddAccount }: { onAddAccount: () => void }) {
  return (
    <div className="flex-1 flex items-center justify-center px-8">
      <div className="text-center spatial-panel px-8 py-7 max-w-sm w-full">
        <div className="w-12 h-12 rounded-[18px] flex items-center justify-center mx-auto mb-4"
          style={{ background: "rgb(var(--accent-rgb) / 0.06)", border: "1px solid var(--border-default)" }}>
          <Zap size={20} className="text-text-muted" strokeWidth={1.7} />
        </div>
        <p className="text-sm font-semibold text-text-primary mb-1">还没有账号</p>
        <p className="micro-meta mb-5">添加一个账号开始多开</p>
        <button onClick={onAddAccount}
          className="primary-cta">
          创建第一个账号
        </button>
      </div>
    </div>
  );
}
