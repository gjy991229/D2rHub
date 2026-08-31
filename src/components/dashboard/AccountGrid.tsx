import React from "react";
import { Zap } from "lucide-react";
import type { AccountMeta } from "../../store/types";
import {
  SortableContext, rectSortingStrategy,
} from "@dnd-kit/sortable";

interface AccountGridProps {
  accounts: AccountMeta[];
  children: React.ReactNode;
  isSelectionMode?: boolean;
  isDropTarget?: boolean;
  embedded?: boolean;
}

export function AccountGrid({ accounts, children, isSelectionMode, isDropTarget, embedded }: AccountGridProps) {
  return (
    <div className={embedded ? "" : "h-full overflow-auto px-5 pb-4"}>
      <SortableContext items={accounts.map(a => a.id)} strategy={rectSortingStrategy}>
        <div
          className={`spatial-grid ${isSelectionMode ? "scheme-spatial-grid" : ""}`}
          data-drop-target={isDropTarget ? "true" : undefined}
        >
          {children}
        </div>
      </SortableContext>
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
