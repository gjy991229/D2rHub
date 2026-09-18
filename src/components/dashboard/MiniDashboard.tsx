import { useEffect, useRef, useState } from "react";
import { AlertCircle, ChevronDown, Circle, ListChecks, LoaderCircle, Maximize2, Minus, MoreHorizontal, Pin, Play, Settings, Users } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invokeCommand } from "../../platform/tauri";
import { useAccounts } from "../../store/accounts";
import { useGlobalConfig } from "../../store/globalConfig";
import { useLaunch } from "../../store/launch";
import type { AccountMeta } from "../../store/types";
import { sortAccountsByCardOrder } from "../../utils/accountOrder";
import { accountRegionLabel, requiresTokenMigration } from "../../utils/regionPaths";
import { inspectLaunchGroup, launchEntriesForGroup, launchGroupAccountIds, launchGroupIssueDetails } from "../../utils/launchGroups";
import { optionalFeaturesAreAvailable } from "../../features/profile/featureProfile";
import { roomAutomationGateway } from "../../features/roomAutomation/gateway";
import type { RoomAutomationWorkflowStatus } from "../../features/roomAutomation/types";
import { showToast } from "../ui/Toast";
import "../../styles/miniMode.css";

interface MiniDashboardProps {
  busy: boolean;
  pinned: boolean;
  onPin: () => Promise<void>;
  onExpand: () => Promise<boolean>;
  onConfigure: (account?: AccountMeta) => void;
  onManageGroups: () => void;
  onTasks: () => void;
  onKillAll: () => void;
  onAddAccount: () => void;
  onRoomAutomation: () => void;
}

const regionLabels: Record<string, string> = { "国服": "CN", "亚服": "Asia", "美服": "Americas", "欧服": "Europe" };
let runningRefresh: Promise<Set<string>> | null = null;
function refreshRunning(): Promise<Set<string>> {
  if (runningRefresh) return runningRefresh;
  runningRefresh = invokeCommand<string[]>("refresh_account_running_state").then(values => {
    const ids = new Set(values);
    useAccounts.setState(state => ({ accounts: state.accounts.map(account =>
      account.is_running === ids.has(account.id) ? account : { ...account, is_running: ids.has(account.id) }),
    }));
    return ids;
  }).finally(() => { runningRefresh = null; });
  return runningRefresh;
}

export function MiniDashboard(props: MiniDashboardProps) {
  const { accounts } = useAccounts();
  const { config, saving } = useGlobalConfig();
  const launch = useLaunch();
  const english = config?.app_language === "en-US";
  const copy = (zh: string, en: string) => english ? en : zh;
  const [groupId, setGroupId] = useState(() => {
    try { return localStorage.getItem("d2rhub-mini-launch-group") || ""; } catch { return ""; }
  });
  const [menuOpen, setMenuOpen] = useState(false);
  const [dispatching, setDispatching] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [room, setRoom] = useState<RoomAutomationWorkflowStatus | null>(null);
  const actionLock = useRef(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuButton = useRef<HTMLButtonElement>(null);
  const groups = config?.launch_groups ?? [];
  const group = groups.find(item => item.id === groupId);
  const availability = group ? inspectLaunchGroup(group, accounts, config) : null;
  const members = group ? new Set(launchGroupAccountIds(group)) : null;
  const visible = sortAccountsByCardOrder(accounts).filter(account => !members || members.has(account.id));
  const eligible = (account: AccountMeta) => account.initialized
    && !requiresTokenMigration(account.auth_mode, account.region, config);
  const remaining = visible.filter(account => eligible(account) && !account.is_running);
  const allRunning = visible.length > 0 && visible.every(account => account.is_running);
  const locked = props.busy || saving || dispatching || launch.launching;
  const roomInstalled = optionalFeaturesAreAvailable(config)
    && config?.installed_optional_modules?.includes("room-automation") === true;
  const statsInstalled = optionalFeaturesAreAvailable(config)
    && config?.installed_optional_modules?.includes("automation") === true;

  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      try {
        if (await getCurrentWindow().isVisible()) {
          await refreshRunning();
          if (!disposed) setRefreshError(null);
        }
      } catch (error) { if (!disposed) setRefreshError(String(error)); }
      if (!disposed) timer = setTimeout(() => { void poll(); }, 2500);
    };
    void poll();
    return () => { disposed = true; clearTimeout(timer); };
  }, []);

  useEffect(() => {
    if (!roomInstalled) { setRoom(null); return; }
    let disposed = false;
    let stop: (() => void) | undefined;
    void roomAutomationGateway.startSync({
      onConfig: () => {},
      onStatus: status => { if (!disposed) setRoom(status); },
    }).then(unlisten => { if (disposed) unlisten(); else stop = unlisten; })
      .catch(error => { if (!disposed) console.error("Mini room status:", error); });
    return () => { disposed = true; stop?.(); };
  }, [roomInstalled]);

  useEffect(() => {
    if (!menuOpen) return;
    menuRef.current?.querySelector<HTMLButtonElement>("button")?.focus();
    const pointer = (event: PointerEvent) => {
      if (event.target instanceof Node && !menuRef.current?.contains(event.target)
        && !menuButton.current?.contains(event.target)) setMenuOpen(false);
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") { setMenuOpen(false); menuButton.current?.focus(); }
    };
    document.addEventListener("pointerdown", pointer);
    document.addEventListener("keydown", key);
    return () => { document.removeEventListener("pointerdown", pointer); document.removeEventListener("keydown", key); };
  }, [menuOpen]);

  const run = async (action: () => Promise<unknown>) => {
    try { await action(); }
    catch (error) { showToast("error", String(error)); }
  };

  const start = (accountId?: string) => run(async () => {
    if (actionLock.current || useLaunch.getState().launching || saving) return;
    actionLock.current = true;
    setDispatching(true);
    try {
      const running = await refreshRunning();
      if (useLaunch.getState().launching || useGlobalConfig.getState().saving) return;
      const latest = useAccounts.getState().accounts;
      const currentConfig = useGlobalConfig.getState().config;
      const selected = group ? currentConfig?.launch_groups.find(item => item.id === group.id) : null;
      if (group && !selected) throw new Error(copy("启动方案已删除，请重新选择", "This scheme was removed. Select another."));
      const ids = new Set(visible.filter(account => !accountId || account.id === accountId).map(account => account.id));
      const targets = sortAccountsByCardOrder(latest).filter(account => ids.has(account.id) && !running.has(account.id)
        && account.initialized && !requiresTokenMigration(account.auth_mode, account.region, currentConfig));
      if (!targets.length) return;
      // A selected scheme always supplies its overrides, including single-row launches.
      if (selected) {
        const inspected = inspectLaunchGroup(selected, latest, currentConfig);
        if (!inspected.can_launch) throw new Error(launchGroupIssueDetails(inspected.issues));
        const targetIds = new Set(targets.map(account => account.id));
        await useLaunch.getState().startSchemeLaunch(launchEntriesForGroup(selected, latest)
          .filter(entry => targetIds.has(entry.account_id)));
      } else await useLaunch.getState().startLaunch(targets.map(account => account.id));
      await refreshRunning();
    } finally { actionLock.current = false; setDispatching(false); }
  });

  const fullAction = (action: () => void) => {
    setMenuOpen(false);
    void run(async () => { if (await props.onExpand()) action(); });
  };
  const activeProgress = Object.values(launch.progress).find(progress => progress.status === "running");
  const failures = new Map<string, string>();
  for (const log of launch.logs) {
    if (log.status === "error") failures.set(log.account_id, log.message);
    else failures.delete(log.account_id);
  }
  for (const result of launch.results) {
    if (!result.success) failures.set(result.account_id, result.error || copy("启动失败", "Launch failed"));
  }
  const roomOwnsFooter = !launch.launching && !launch.error && !failures.size
    && (room?.running || room?.phase === "error");
  const activeName = accounts.find(account => account.id === activeProgress?.account_id)?.display_name;
  const footer = launch.launching
    ? `${activeName || copy("正在启动", "Launching")} · ${activeProgress?.message || copy("准备中", "Preparing")}`
    : launch.error || (failures.size ? copy(`${failures.size} 个账号启动失败 · 查看详情`, `${failures.size} launches failed · Details`)
      : room?.running ? copy("自动跟房进行中", "Room automation running")
      : room?.phase === "error" ? copy("自动跟房失败 · 查看详情", "Room automation failed · Details")
      : copy("任务空闲", "No active tasks"));

  return <section className="mini-dashboard" data-i18n-skip aria-label={copy("迷你模式", "Mini mode")}>
    <header className="mini-titlebar" data-tauri-drag-region>
      <div className="mini-brand" data-tauri-drag-region><img src="/logo.png" alt="" draggable={false} /><strong data-tauri-drag-region>D2RHub</strong><span data-tauri-drag-region>{copy("迷你", "Mini")}</span></div>
      <div className="mini-window-actions">
        <button className="icon-btn" disabled={props.busy} aria-pressed={props.pinned} title={copy("窗口置顶", "Always on top")} aria-label={copy("窗口置顶", "Always on top")} onClick={() => void props.onPin()}><Pin size={15} /></button>
        <button className="icon-btn" disabled={props.busy} title={copy("恢复完整模式", "Full mode")} aria-label={copy("恢复完整模式", "Full mode")} onClick={() => void props.onExpand()}><Maximize2 size={15} /></button>
        <button className="icon-btn" title={copy("最小化到托盘", "Hide to tray")} aria-label={copy("最小化到托盘", "Hide to tray")} onClick={() => void run(() => invokeCommand("hide_main_window"))}><Minus size={16} /></button>
      </div>
    </header>
    <div className="mini-toolbar">
      <div className="mini-scope"><Users size={15} aria-hidden="true" /><select aria-label={copy("账号范围与启动方案", "Account scope and launch scheme")} value={group?.id || ""} disabled={locked} onChange={event => {
        setGroupId(event.target.value);
        try { localStorage.setItem("d2rhub-mini-launch-group", event.target.value); } catch { /* Optional preference. */ }
      }}><option value="">{copy("全部账号", "All accounts")}</option>{groups.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}</select><ChevronDown size={13} aria-hidden="true" /></div>
      {launch.launching ? <button className="control-btn danger-control" onClick={() => void launch.cancelLaunch()}>{copy("取消操作", "Cancel")}</button>
        : <button className="primary-cta" disabled={locked || !remaining.length || availability?.can_launch === false} onClick={() => void start()}><Play size={13} />{allRunning ? copy("全部运行中", "All running") : copy(`启动剩余 (${remaining.length})`, `Start (${remaining.length})`)}</button>}
    </div>
    <div className="mini-summary">{copy(`${visible.length} 个账号 · ${visible.filter(account => account.is_running).length} 个运行中`, `${visible.length} accounts · ${visible.filter(account => account.is_running).length} running`)}</div>
    {refreshError && <button className="mini-notice" title={refreshError} onClick={() => void run(async () => { await refreshRunning(); setRefreshError(null); })}><AlertCircle size={14} />{copy("状态刷新失败，点击重试", "Status unavailable · Retry")}</button>}
    {availability && !availability.can_launch && <button className="mini-notice" title={launchGroupIssueDetails(availability.issues)} onClick={() => fullAction(props.onManageGroups)}><AlertCircle size={14} />{copy("方案配置不完整，点击处理", "Scheme needs attention · Edit")}</button>}
    <div className="mini-account-list">
      {!visible.length && <div className="mini-empty"><Users size={26} /><strong>{copy("还没有账号", "No accounts yet")}</strong><p>{copy("添加账号后，即可在这里启动和切回游戏。", "Add an account to launch and switch games here.")}</p><button className="control-btn" onClick={() => fullAction(props.onAddAccount)}>{copy("添加账号", "Add account")}</button></div>}
      {visible.map((account, index) => {
        const needsConfig = !eligible(account);
        const progress = launch.progress[account.id];
        const active = launch.launching && progress && progress.step !== "done" && progress.status !== "error";
        const failure = failures.get(account.id);
        const status = active ? progress.message : account.is_running ? copy("运行中", "Running")
          : needsConfig ? copy("待配置", "Needs setup") : failure ? copy("启动失败", "Launch failed") : copy("未运行", "Stopped");
        const label = active ? copy("启动中", "Starting") : account.is_running ? copy("切回", "Switch")
          : needsConfig ? copy("配置", "Setup") : failure ? copy("重试", "Retry") : copy("启动", "Start");
        return <article key={account.id} className="mini-account" data-state={active ? "busy" : account.is_running ? "running" : needsConfig ? "warning" : failure ? "error" : "idle"}>
          <span className="mini-index">{String(index + 1).padStart(2, "0")}</span>
          <div className="mini-account-copy"><strong title={account.display_name || account.id}>{account.display_name || account.id}</strong>
            <div className="mini-account-meta"><span className="hig-badge hig-badge-neutral">{english ? regionLabels[accountRegionLabel(account.region)] : accountRegionLabel(account.region)}</span>
              <span className="mini-status" title={failure || status}>{active ? <LoaderCircle size={10} className="animate-spin" /> : <Circle size={6} fill="currentColor" />}{status}</span></div>
          </div>
          <button className="control-btn mini-account-action" disabled={props.busy || active || (!account.is_running && !needsConfig && (dispatching || launch.launching || saving || availability?.can_launch === false))}
            aria-label={`${label} ${account.display_name || account.id}`} onClick={() => {
              if (account.is_running) void run(async () => { await invokeCommand("focus_game_window", { accountId: account.id }); });
              else if (needsConfig) fullAction(() => props.onConfigure(account));
              else void start(account.id);
            }}>{label}</button>
        </article>;
      })}
    </div>
    <footer className="mini-footer">
      <button className="mini-task-status" title={footer} onClick={() => fullAction(roomOwnsFooter ? props.onRoomAutomation : props.onTasks)}><ListChecks size={15} /><span>{footer}</span></button>
      <button ref={menuButton} className="icon-btn" aria-label={copy("更多操作", "More actions")} aria-expanded={menuOpen} aria-controls="mini-more-menu" onClick={() => setMenuOpen(value => !value)}><MoreHorizontal size={19} /></button>
      {menuOpen && <div ref={menuRef} id="mini-more-menu" className="mini-more-menu" aria-label={copy("更多操作", "More actions")}>
        <button onClick={() => fullAction(() => props.onConfigure())}><Settings size={14} />{copy("打开设置", "Settings")}</button>
        <button onClick={() => fullAction(props.onManageGroups)}>{copy("管理启动方案", "Manage schemes")}</button>
        <button onClick={() => fullAction(props.onAddAccount)}>{copy("添加账号", "Add account")}</button>
        {roomInstalled && <button onClick={() => fullAction(props.onRoomAutomation)}>{copy("自动跟房", "Room automation")}</button>}
        {statsInstalled && <button onClick={() => { setMenuOpen(false); void run(() => invokeCommand("open_stats_page")); }}>{copy("查看统计", "Statistics")}</button>}
        <button className="mini-danger" onClick={() => fullAction(props.onKillAll)}>{copy("一键关闭所有游戏…", "Close all games…")}</button>
      </div>}
    </footer>
  </section>;
}
