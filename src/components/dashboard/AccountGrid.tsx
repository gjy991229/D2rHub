import { useI18n } from "../../i18n";
import { useState, type ReactNode } from "react";
import { LayoutGrid, Rows3, Zap } from "lucide-react";
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
  children: (account: AccountMeta) => ReactNode;
  issue: (account: AccountMeta) => string | null | undefined;
  isSelectionMode?: boolean;
  onReorder: (accountIds: string[]) => void | Promise<void>;
}

export function AccountGrid({ accounts, children, issue, isSelectionMode, onReorder }: AccountGridProps) {
  const en = useI18n().language === "en-US";
  const running = accounts.filter(account => account.is_running).length;
  const [view, setView] = useState<"list" | "cards">(() => {
    try { return localStorage.getItem("hub-account-view") === "list" ? "list" : "cards"; }
    catch { return "cards"; }
  });
  const [filter, setFilter] = useState<"all" | "running" | "attention">("all");
  const attention = accounts.filter(account => !!issue(account));
  const visible = filter === "all" ? accounts : filter === "running" ? accounts.filter(account => account.is_running) : attention;
  const changeView = (next: typeof view) => {
    setView(next);
    try { localStorage.setItem("hub-account-view", next); } catch { /* Session preference still applies. */ }
  };
  const sensors = useSensors(
    useSensor(MouseSensor, { activationConstraint: { distance: 6 } }),
    useSensor(TouchSensor, { activationConstraint: { delay: 180, tolerance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const handleDragEnd = ({ active, over }: DragEndEvent) => {
    if (isSelectionMode || !over || active.id === over.id) return;
    const oldIndex = visible.findIndex(account => account.id === active.id);
    const newIndex = visible.findIndex(account => account.id === over.id);
    if (oldIndex < 0 || newIndex < 0) return;
    const reordered = arrayMove(visible, oldIndex, newIndex);
    const visibleIds = new Set(visible.map(account => account.id));
    let next = 0;
    void onReorder(accounts.map(account => visibleIds.has(account.id) ? reordered[next++].id : account.id));
  };

  return (
    <div className="dashboard-accounts flex-1 overflow-auto px-5 pb-5">
      <header className="account-grid-heading">
        <h2>{en ? "Accounts" : "账号"}</h2>
        <div className="roster-filters" role="group" aria-label={en ? "Filter accounts" : "筛选账号"}>
          {([["all", en ? "All" : "全部", accounts.length], ["running", en ? "Running" : "运行中", running],
            ["attention", en ? "Needs setup" : "待处理", attention.length]] as const).map(([id, label, count]) =>
            <button key={id} type="button" aria-pressed={filter === id} onClick={() => setFilter(id)}>{label} <span>{count}</span></button>)}
        </div>
        <div className="roster-views" role="group" aria-label={en ? "Account view" : "账号视图"}>
          {([["cards", en ? "Cards" : "卡片", LayoutGrid], ["list", en ? "List" : "列表", Rows3]] as const).map(([id,label,Icon]) =>
            <button key={id} type="button" title={label} aria-label={label} aria-pressed={view === id} onClick={() => changeView(id)}><Icon size={15} /></button>)}
        </div>
      </header>
      {view === "list" && visible.length > 0 && <div className="roster-columns" aria-hidden="true">
        {(en ? ["Account", "Region", "Active Mod", "Status", "Actions"] : ["账号名称", "区服", "当前 Mod", "状态", "操作"]).map(label => <span key={label}>{label}</span>)}
      </div>}
      {!visible.length && <p className="roster-empty" role="status">{en ? "No accounts match this filter." : "当前筛选下没有账号。"}</p>}
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
        <SortableContext items={visible.map(account => account.id)} strategy={rectSortingStrategy}>
          <div data-view={view} className={`spatial-grid ${isSelectionMode ? "scheme-spatial-grid" : ""}`}>
            {visible.map(children)}
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
