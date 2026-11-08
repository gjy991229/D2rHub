import React from "react";
import { Zap } from "lucide-react";
import type { AccountMeta } from "../../store/types";
import {
  closestCenter,
  DndContext,
  type DragEndEvent,
  KeyboardSensor,
  MouseSensor,
  TouchSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  arrayMove,
  rectSortingStrategy,
  sortableKeyboardCoordinates,
  SortableContext,
} from "@dnd-kit/sortable";

interface AccountGridProps {
  accounts: AccountMeta[];
  children: React.ReactNode;
  isSelectionMode?: boolean;
  onReorder: (accountIds: string[]) => void | Promise<void>;
}

export function AccountGrid({ accounts, children, isSelectionMode, onReorder }: AccountGridProps) {
  const sensors = useSensors(
    useSensor(MouseSensor, { activationConstraint: { distance: 6 } }),
    useSensor(TouchSensor, { activationConstraint: { delay: 180, tolerance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const handleDragEnd = ({ active, over }: DragEndEvent) => {
    if (isSelectionMode || !over || active.id === over.id) return;
    const oldIndex = accounts.findIndex(account => account.id === active.id);
    const newIndex = accounts.findIndex(account => account.id === over.id);
    if (oldIndex < 0 || newIndex < 0) return;
    void onReorder(arrayMove(accounts, oldIndex, newIndex).map(account => account.id));
  };

  return (
    <div className="dashboard-accounts flex-1 overflow-auto px-5 pb-5">
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
        <SortableContext items={accounts.map(account => account.id)} strategy={rectSortingStrategy}>
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
    <div className="dashboard-accounts flex-1 overflow-auto px-5 pb-5" role="status" aria-label="正在读取账号" aria-busy="true">
      <div className="spatial-grid">
        {[1,2,3,4,5,6].map(i => (
          <div key={i} className="skeleton rounded-xl h-[172px]" aria-hidden="true" />
        ))}
      </div>
    </div>
  );
}

export function AccountGridEmpty({ onAddAccount, onOpenRecognition }: { onAddAccount: () => void; onOpenRecognition?: () => void }) {
  return (
    <div className="dashboard-empty flex-1 flex items-center justify-center px-8">
      <div className="text-center spatial-panel px-8 py-7 max-w-sm w-full">
        <div className="w-12 h-12 rounded-[18px] flex items-center justify-center mx-auto mb-4"
          style={{ background: "rgb(var(--accent-rgb) / 0.06)", border: "1px solid var(--border-default)" }}>
          <Zap size={20} className="text-text-muted" strokeWidth={1.7} />
        </div>
        <h2 className="text-sm font-semibold text-text-primary mb-1">还没有账号</h2>
        <p className="micro-meta mb-5">添加一个账号开始多开</p>
        <button onClick={onAddAccount}
          className="primary-cta">
          创建第一个账号
        </button>
        {onOpenRecognition && <button onClick={onOpenRecognition} className="mt-3 block w-full text-xs text-accent hover:underline">
          无需账号，使用掉落识别与统计
        </button>}
      </div>
    </div>
  );
}
