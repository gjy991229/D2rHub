import { prefersReducedMotion } from "../../store/motionPreference";
import { useI18n } from "../../i18n";
import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { LayoutGrid, Rows3, Zap } from "lucide-react";
import { useGlobalConfig } from "../../store/globalConfig";
import type { AccountMeta, AccountRosterView } from "../../store/types";
import { showToast } from "../ui/Toast";
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
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";

const LEGACY_ROSTER_VIEW_KEY = "hub-account-view";

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
  const configuredView = useGlobalConfig(state => state.config?.account_roster_view);
  const [fallbackView, setFallbackView] = useState<AccountRosterView>(() => {
    try { return localStorage.getItem(LEGACY_ROSTER_VIEW_KEY) === "list" ? "list" : "cards"; }
    catch { return "cards"; }
  });
  const [pendingView, setPendingView] = useState<AccountRosterView>();
  const latestViewChange = useRef(0);
  const view = pendingView ?? configuredView ?? fallbackView;
  const [filter, setFilter] = useState<"all" | "running" | "attention">("all");
  const grid = useRef<HTMLDivElement>(null);
  const [sorting, setSorting] = useState(false);
  const [pendingOrder, setPendingOrder] = useState<string[]>();
  const reorderQueue = useRef(Promise.resolve());
  const orderRanks = new Map(pendingOrder?.map((id, index) => [id, index]));
  const orderedAccounts = pendingOrder ? [...accounts].sort((a, b) =>
    (orderRanks.get(a.id) ?? orderRanks.size) - (orderRanks.get(b.id) ?? orderRanks.size)) : accounts;
  const attention = orderedAccounts.filter(account => !!issue(account));
  const visible = filter === "all" ? orderedAccounts : filter === "running" ? orderedAccounts.filter(account => account.is_running) : attention;

  useLayoutEffect(() => {
    if (prefersReducedMotion()) return;
    const animation = grid.current?.animate?.(
      [{ opacity: .75, translate: "0 3px" }, { opacity: 1, translate: "0 0" }],
      { duration: 170, easing: "ease-out" },
    );
    return () => animation?.cancel();
  }, [view, filter]);

  const changeView = async (next: AccountRosterView) => {
    if (next === view) return;
    const { config, patch } = useGlobalConfig.getState();
    if (!config) {
      setFallbackView(next);
      try { localStorage.setItem(LEGACY_ROSTER_VIEW_KEY, next); } catch { /* Keep the selection for this session. */ }
      return;
    }
    const change = ++latestViewChange.current;
    setPendingView(next);
    try {
      await patch({ account_roster_view: next });
    } catch {
      if (latestViewChange.current === change) showToast("error", en ? "Could not save the account view" : "无法保存账号视图");
    } finally {
      if (latestViewChange.current === change) setPendingView(undefined);
    }
  };
  const sensors = useSensors(
    useSensor(MouseSensor, { activationConstraint: { distance: 6 } }),
    useSensor(TouchSensor, { activationConstraint: { delay: 180, tolerance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const handleDragEnd = ({ active, over }: DragEndEvent) => {
    setSorting(false);
    if (isSelectionMode || !over || active.id === over.id) return;
    const oldIndex = visible.findIndex(account => account.id === active.id);
    const newIndex = visible.findIndex(account => account.id === over.id);
    if (oldIndex < 0 || newIndex < 0) return;
    const reordered = arrayMove(visible, oldIndex, newIndex);
    const visibleIds = new Set(visible.map(account => account.id));
    let next = 0;
    const nextOrder = orderedAccounts.map(account => visibleIds.has(account.id) ? reordered[next++].id : account.id);
    setPendingOrder(nextOrder);
    const settle = () => setPendingOrder(current => current === nextOrder ? undefined : current);
    reorderQueue.current = reorderQueue.current.then(() => onReorder(nextOrder)).then(settle, settle);
  };

  return (
    <div className="dashboard-accounts flex-1 overflow-auto px-5 pb-5">
      <header className="account-grid-heading">
        <div className="roster-filters" role="group" aria-label={en ? "Filter accounts" : "筛选账号"}>
          {([["all", en ? "All" : "全部", accounts.length], ["running", en ? "Running" : "运行中", running],
            ["attention", en ? "Needs setup" : "待处理", attention.length]] as const).map(([id, label, count]) =>
            <button key={id} type="button" disabled={sorting} aria-pressed={filter === id} onClick={() => setFilter(id)}>{label} <span>{count}</span></button>)}
        </div>
        <div className="roster-views" role="group" aria-label={en ? "Account view" : "账号视图"}>
          {([["cards", en ? "Cards" : "卡片", LayoutGrid], ["list", en ? "List" : "列表", Rows3]] as const).map(([id,label,Icon]) =>
            <button key={id} type="button" disabled={sorting} title={label} aria-label={label} aria-pressed={view === id} onClick={() => void changeView(id)}><Icon size={15} /></button>)}
        </div>
      </header>
      {view === "list" && visible.length > 0 && <div className="roster-columns" aria-hidden="true">
        {(en ? ["Account", "Region", "Active Mod", "Status", "Actions"] : ["账号名称", "区服", "当前 Mod", "状态", "操作"]).map(label => <span key={label}>{label}</span>)}
      </div>}
      {!visible.length && <p className="roster-empty" role="status">{en ? "No accounts match this filter." : "当前筛选下没有账号。"}</p>}
      <DndContext sensors={sensors} collisionDetection={closestCenter}
        onDragStart={() => setSorting(true)} onDragEnd={handleDragEnd} onDragCancel={() => setSorting(false)}>
        <SortableContext items={visible.map(account => account.id)} strategy={view === "list" ? verticalListSortingStrategy : rectSortingStrategy}>
          <div ref={grid} data-view={view} className={`spatial-grid ${isSelectionMode ? "scheme-spatial-grid" : ""}`}>
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
