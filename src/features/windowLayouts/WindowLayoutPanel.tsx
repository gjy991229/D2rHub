import { useEffect, useId, useRef, useState } from "react";
import { AlignCenter, ArrowLeft, Camera, Check, ChevronDown, LayoutDashboard, Maximize, Monitor, Plus, RefreshCw, Save, Trash2 } from "lucide-react";
import type { GlobalConfig, LayoutMonitor, LayoutSlot, WindowLayout } from "../../store/types";
import { Toggle } from "../../components/ui/Toggle";
import { Button } from "../../components/ui/Button";
import { ResolutionInput } from "../../components/ui/ResolutionInput";
import { useAccounts } from "../../store/accounts";
import { layoutAccountLabels } from "./accountLabels";
import { useLaunch } from "../../store/launch";
import { showToast } from "../../components/ui/Toast";
import { LayoutCanvas } from "./LayoutCanvas";
import { captureGameLayout, getLayoutAccountOrder, getLayoutMonitors, restoreGameLayout } from "./gateway";
import { adaptLayout, changeSlotCount, constrainSlot, defaultPresetResolution, MAX_LAYOUT_WINDOWS, MIN_VISIBLE_PIXELS, MIN_LAYOUT_HEIGHT, MIN_LAYOUT_WIDTH, moveSlot, presetSlots, primaryMonitor, resizeSlot, slotRect, visibleSize, visibleInsets, validateLayout, type PresetResolution } from "./model";
import "./windowLayouts.css";

function GeometryField({ label, value, min, max, onChange }: { label: string; value: number; min: number; max: number; onChange: (value: number) => void }) {
  const [draft, setDraft] = useState(String(value));
  const focused = useRef(false);
  useEffect(() => { if (!focused.current) setDraft(String(value)); }, [value]);
  return <label className="layout-geometry-field"><span>{label}</span>
    <input className="settings-input" type="number" aria-label={label} value={draft} min={min} max={max} step={1}
      onFocus={() => { focused.current = true; }} onChange={event => setDraft(event.target.value)}
      onBlur={() => {
        focused.current = false;
        const number = draft.trim() ? Number(draft) : NaN;
        const next = Number.isFinite(number) ? Math.round(Math.min(max, Math.max(min, number))) : value;
        setDraft(String(next));
        if (next !== value) onChange(next);
      }} onKeyDown={event => {
        if (event.key === "Enter") { event.preventDefault(); event.currentTarget.blur(); }
        if (event.key === "Escape" && draft !== String(value)) { event.stopPropagation(); setDraft(String(value)); }
      }} />
  </label>;
}

function PresetThumbnail({ monitors, count, resolution }: { monitors: LayoutMonitor[]; count: number; resolution: PresetResolution }) {
  const monitor = primaryMonitor(monitors)!;
  const windows = presetSlots(monitors, count, resolution).slice().reverse();
  return <svg viewBox={`-30 -30 ${monitor.work_area.width + 60} ${monitor.work_area.height + 60}`} aria-hidden="true">
    {windows.map((slot, index) => {
      const size = visibleSize(slot, monitor), edge = visibleInsets(monitor);
      return <g key={index}>
        <rect x={slot.x} y={slot.y} width={size.width} height={size.height} rx={4}
          className={index === windows.length - 1 ? "preset-primary" : "preset-secondary"} vectorEffect="non-scaling-stroke" />
        {edge.top > 0 && <rect className="preset-caption" x={slot.x} y={slot.y} width={size.width} height={edge.top} />}
      </g>;
    })}
  </svg>;
}

export function WindowLayoutPanel({ config, updateConfig, persistConfig, readDraft, onDraftChange, onSavingChange }: {
  config: GlobalConfig;
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
  persistConfig: (config: GlobalConfig, quiet?: boolean) => Promise<GlobalConfig | null>;
  readDraft: () => GlobalConfig | null;
  onDraftChange?: (dirty: boolean) => void;
  onSavingChange?: (saving: boolean) => void;
}) {
  const english = config.app_language === "en-US";
  const launching = useLaunch(state => state.launching);
  const accounts = useAccounts(state => state.accounts);
  const [monitors, setMonitors] = useState<LayoutMonitor[]>([]);
  const [monitorError, setMonitorError] = useState<string | null>(null);
  const [accountOrder, setAccountOrder] = useState<string[] | undefined>();
  const [accountOrderError, setAccountOrderError] = useState("");
  const [monitorLoading, setMonitorLoading] = useState(true);
  const [detecting, setDetecting] = useState(false);
  const [monitorFeedback, setMonitorFeedback] = useState(false);
  const [editor, setEditor] = useState<{ draft: WindowLayout; baseline: WindowLayout | null } | null>(null);
  const [selected, setSelected] = useState(0);
  const [profile, setProfile] = useState<PresetResolution | null>(null);
  const [presetsOpen, setPresetsOpen] = useState(false);
  const presetsId = useId();
  const [focusedMonitor, setFocusedMonitor] = useState("");
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<{ text: string; error?: boolean } | null>(null);
  const [savedId, setSavedId] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const panel = useRef<HTMLFieldSetElement>(null);
  const mounted = useRef(true);
  const loading = useRef(false);
  const busyRef = useRef(false);
  const refreshMonitors = async (explicit = false) => {
    if (loading.current) return;
    loading.current = true;
    if (explicit) { setDetecting(true); setMonitorFeedback(false); }
    try {
      const [displayResult, orderResult] = await Promise.allSettled([getLayoutMonitors(), getLayoutAccountOrder()]);
      if (mounted.current) {
        if (orderResult.status === "fulfilled") {
          setAccountOrder(previous => JSON.stringify(previous) === JSON.stringify(orderResult.value) ? previous : orderResult.value);
          setAccountOrderError("");
        } else { setAccountOrder(undefined); setAccountOrderError(String(orderResult.reason)); }
        if (displayResult.status === "fulfilled") {
          const current = displayResult.value;
          setMonitors(previous => JSON.stringify(previous) === JSON.stringify(current) ? previous : current);
          setMonitorError(null);
          if (explicit) setMonitorFeedback(true);
        } else { setMonitorError(String(displayResult.reason)); setMonitorFeedback(false); }
      }
    } catch (error) { if (mounted.current) { setMonitorError(String(error)); setMonitorFeedback(false); } }
    finally { loading.current = false; if (mounted.current) { setMonitorLoading(false); setDetecting(false); } }
  };
  useEffect(() => {
    mounted.current = true;
    void refreshMonitors();
    const interval = window.setInterval(() => { if (!document.hidden) void refreshMonitors(); }, 10000);
    const focus = () => { void refreshMonitors(); };
    window.addEventListener("focus", focus);
    return () => { mounted.current = false; window.clearInterval(interval); window.removeEventListener("focus", focus); };
  }, []);
  const accountRuntimeKey = accounts.map(account => `${account.id}:${account.is_running}:${account.running_pid ?? ""}`).join("|");
  useEffect(() => { void refreshMonitors(); }, [accountRuntimeKey]);
  useEffect(() => { panel.current?.closest(".settings-panel-scroll")?.scrollTo?.({ top: 0 }); }, [editor?.draft.id]);

  const layouts = config.window_layouts ?? [];
  const active = layouts.find(layout => layout.id === config.active_window_layout_id);
  const layout = editor && monitors.length ? adaptLayout(editor.draft, monitors) : null;
  const primary = primaryMonitor(monitors);
  const resolution = profile ?? defaultPresetResolution(primary);
  const selectedIndex = Math.min(selected, (layout?.windows.length ?? 1) - 1);
  const slot = layout?.windows[selectedIndex];
  const monitor = slot ? monitors.find(monitor => monitor.id === slot.monitor_id) : undefined;
  const rect = slot && layout ? slotRect(slot, layout.monitors) : undefined;
  const visible = slot ? visibleSize(slot, monitor) : undefined;
  const capable = !!primary && primary.work_area.width >= MIN_LAYOUT_WIDTH && primary.work_area.height >= MIN_LAYOUT_HEIGHT;
  const changedDisplays = editor?.baseline && monitors.length && JSON.stringify(editor.baseline.monitors) !== JSON.stringify(monitors);
  const dirty = !!editor && (!editor.baseline || JSON.stringify(layout ?? editor.draft) !== JSON.stringify(editor.baseline));
  useEffect(() => { onDraftChange?.(dirty); }, [dirty, onDraftChange]);
  useEffect(() => () => { onDraftChange?.(false); }, [onDraftChange]);
  useEffect(() => () => { onSavingChange?.(false); }, [onSavingChange]);
  const labels = layoutAccountLabels(accounts, layout?.windows.length ?? 0, english, accountOrder);
  const replace = (next: WindowLayout) => { setEditor(current => current ? { ...current, draft: next } : current); setFeedback(null); setConfirmDelete(false); };
  const changeWindows = (windows: LayoutSlot[]) => { if (layout) replace({ ...layout, windows }); };
  const changeSlot = (patch: Partial<LayoutSlot>) => {
    if (!layout || !slot) return;
    const display = monitors.find(monitor => monitor.id === (patch.monitor_id ?? slot.monitor_id))!;
    changeWindows(layout.windows.map((candidate, index) => index === selectedIndex ? constrainSlot({ ...slot, ...patch }, display) : candidate));
  };
  const openEditor = (draft: WindowLayout, baseline: WindowLayout | null) => {
    setEditor({ draft: structuredClone(draft), baseline: baseline ? structuredClone(baseline) : null });
    setSelected(0); setFocusedMonitor(""); setProfile(null); setPresetsOpen(false); setConfirmDelete(false); setFeedback(null);
  };
  const create = () => {
    if (!capable) return;
    let name = english ? "New layout" : "新布局";
    let suffix = 2;
    while (layouts.some(layout => layout.name === name)) { name = `${english ? "New layout" : "新布局"} ${suffix}`; suffix += 1; }
    openEditor({ id: crypto.randomUUID(), name, monitors, windows: presetSlots(monitors, 1, resolution) }, null);
  };
  // Keep editing local. A failed persistence attempt also removes its pending
  // layout intent from the settings session, so closing cannot save it silently.
  const commit = async (mutate: (current: GlobalConfig) => void) => {
    const before = readDraft() ?? config;
    const candidate = structuredClone(before);
    mutate(candidate);
    const keys = ["window_layouts", "active_window_layout_id", "window_layout_enabled"] as const;
    const rollback = () => {
      updateConfig(current => {
        for (const key of keys) {
          if (JSON.stringify(current[key]) === JSON.stringify(candidate[key])) Object.assign(current, { [key]: before[key] });
        }
      });
    };
    let saved: GlobalConfig | null;
    try { saved = await persistConfig(candidate, true); }
    catch (error) { rollback(); throw error; }
    if (!saved) {
      rollback();
      throw new Error(english ? "Save failed. Your draft is kept; please retry." : "保存失败，草稿已保留，请重试。");
    }
    updateConfig(current => {
      for (const key of keys) if (JSON.stringify(before[key]) !== JSON.stringify(candidate[key])) Object.assign(current, { [key]: saved[key] });
    });
    return saved;
  };
  const run = async (work: () => Promise<void>) => {
    if (busyRef.current || launching) return;
    busyRef.current = true; setBusy(true); onSavingChange?.(true); setFeedback(null);
    try { await work(); }
    catch (error) { if (mounted.current) setFeedback({ text: String(error instanceof Error ? error.message : error), error: true }); }
    finally { busyRef.current = false; onSavingChange?.(false); if (mounted.current) setBusy(false); }
  };
  const save = (apply = false) => void run(async () => {
    if (!layout) return;
    const candidate = { ...layout, name: layout.name.trim() };
    const error = validateLayout(candidate);
    if (error) throw new Error(error);
    const current = readDraft() ?? config;
    if ((current.window_layouts ?? []).some(other => other.id !== candidate.id && other.name.trim().toLocaleLowerCase() === candidate.name.toLocaleLowerCase())) {
      throw new Error(english ? "A layout with this name already exists" : "此布局名称已存在");
    }
    await commit(current => {
      const existing = current.window_layouts ?? [];
      current.window_layouts = existing.some(other => other.id === candidate.id)
        ? existing.map(other => other.id === candidate.id ? candidate : other) : [...existing, candidate];
      if (apply) { current.active_window_layout_id = candidate.id; current.window_layout_enabled = true; }
    });
    if (!mounted.current) return;
    setEditor(null); setSavedId(candidate.id);
    const savedMessage = english ? `Saved “${candidate.name}”` : `“${candidate.name}”已保存`;
    setFeedback({ text: savedMessage });
    showToast("success", savedMessage);
    if (apply) {
      let result;
      try { result = await restoreGameLayout(); }
      catch (error) {
        if (mounted.current) setFeedback({ text: `${savedMessage}；${english ? "Could not restore windows" : "恢复窗口失败"}：${error}`, error: true });
        return;
      }
      if (!mounted.current) return;
      const message = result.failures.length ? `${savedMessage}；${result.failures.join("；")}`
        : !result.applied.length ? (english ? `${savedMessage}. Selected for the next launch.` : `${savedMessage}，已设为当前布局，下次启动时应用。`)
          : english ? `${savedMessage}. Applied to ${result.applied.length} windows.` : `${savedMessage}，已应用到 ${result.applied.length} 个窗口。`;
      setFeedback({ text: message, error: result.failures.length > 0 });
    }
  });
  const capture = () => void run(async () => {
    const result = await captureGameLayout();
    if (!mounted.current) return;
    const existing = (readDraft() ?? config).window_layouts ?? [];
    if (existing.length >= 64) throw new Error(english ? "You can save up to 64 layouts." : "最多保存 64 个窗口布局，请先删除不需要的布局。");
    const base = english ? "Current windows" : "当前窗口布局";
    let name = base, suffix = 2;
    while (existing.some(item => item.name.trim().toLocaleLowerCase() === name.toLocaleLowerCase())) name = `${base} ${suffix++}`;
    setMonitors(result.layout.monitors);
    setMonitorError(null);
    setAccountOrder(result.account_ids);
    setAccountOrderError("");
    openEditor({ ...result.layout, name }, null);
    setFeedback({ text: english
      ? `Captured ${result.layout.windows.length} windows. Review the preview, then save the layout.`
      : `已捕获 ${result.layout.windows.length} 个窗口的实际位置与游戏分辨率，确认预览后点击“保存布局”。` });
  });
  const remove = () => {
    if (!editor?.baseline) return;
    if (!confirmDelete) { setConfirmDelete(true); return; }
    const deleting = editor.baseline;
    void run(async () => {
      await commit(current => {
        current.window_layouts = (current.window_layouts ?? []).filter(candidate => candidate.id !== deleting.id);
        if (current.active_window_layout_id === deleting.id) current.active_window_layout_id = null;
      });
      if (mounted.current) { setEditor(null); setFeedback({ text: english ? `Deleted “${deleting.name}”` : `“${deleting.name}”已删除` }); }
    });
  };
  const displayStatus = <div className="layout-display-status">
    <div><Monitor size={14} aria-hidden="true" /><span>{monitorLoading ? (english ? "Detecting displays…" : "正在检测显示器…")
      : monitorFeedback ? (english ? `Detection complete · ${monitors.length} displays` : `检测完成，已识别 ${monitors.length} 台显示器`)
        : (english ? `${monitors.length} displays connected` : `已识别 ${monitors.length} 台显示器`)}</span>
      {primary && <small>{primary.name} · {english ? "Primary" : "主屏"} · {primary.bounds.width} × {primary.bounds.height}</small>}</div>
    <Button type="button" size="sm" loading={detecting} disabled={busy || monitorLoading} onClick={() => void refreshMonitors(true)}>
      {!detecting && <RefreshCw size={13} />}
      {detecting ? (english ? "Detecting…" : "检测中…") : (english ? "Detect displays" : "检测显示器")}</Button>
  </div>;

  const saveFeedback = feedback && <div className="layout-save-feedback" aria-live="polite"><p className={feedback.error ? "layout-error" : "layout-success"} role={feedback.error ? "alert" : "status"}>
    {!feedback.error && <Check size={14} aria-hidden="true" />}{feedback.text}</p></div>;

  return <fieldset ref={panel} className="window-layout-panel" disabled={launching || busy} data-i18n-skip>
    {launching && <p className="layout-helper" role="status">{english ? "Launching accounts. Editing resumes when launch completes." : "账号正在启动，完成后即可调整布局。"}</p>}
    {!editor ? <>
      <div className="layout-panel-heading">
        <div><h3>{english ? "Your window layouts" : "我的窗口布局"}</h3>
          <p>{english ? "Edit a saved layout here, then select it from the dashboard." : "选择已保存的布局进行编辑，在主界面快捷面板中切换使用。"}</p></div>
        <Toggle checked={config.window_layout_enabled ?? false} label={english ? "Enable layouts" : "启用布局"}
          onChange={value => void run(async () => { await commit(current => { current.window_layout_enabled = value; }); })} />
      </div>
      {saveFeedback}
      <section className="layout-library" aria-label={english ? "Saved layouts" : "已保存布局"}>
        <div className="layout-library-heading"><span>{english ? "Saved layouts" : "已保存布局"}</span>
          <div className="layout-library-actions">
            <Button type="button" size="sm" onClick={capture} loading={busy} disabled={monitorLoading || layouts.length >= 64}><Camera size={14} />{english ? "Capture current layout" : "捕获当前布局"}</Button>
            <Button type="button" size="sm" onClick={create} disabled={!capable || monitorLoading || layouts.length >= 64}><Plus size={14} />{english ? "New layout" : "新建布局"}</Button>
          </div></div>
        <p className="layout-helper">{english ? "Arrange your game windows, then capture their positions and resolutions. Restore minimized or maximized windows first." : "摆好游戏窗口后，可捕获实际位置和分辨率；请先将最小化、最大化或全屏窗口还原为普通窗口。"}</p>
        {layouts.length >= 64 && <p className="layout-helper" role="status">{english ? "You have reached the limit of 64 layouts. Delete one before adding another." : "已达到 64 个布局上限，删除不需要的布局后即可继续添加。"}</p>}
        {layouts.length ? <div className="layout-capsules layout-library-capsules">
          {layouts.map(saved => <button key={saved.id} type="button" className="layout-capsule" data-saved={saved.id === savedId}
            aria-label={english ? `Edit layout: ${saved.name}` : `编辑布局：${saved.name}`} data-applied={!!config.window_layout_enabled && saved.id === active?.id}
            onClick={() => openEditor(saved, saved)}>
            {!!config.window_layout_enabled && saved.id === active?.id && <Check size={13} aria-hidden="true" />}
            <span>{saved.name}</span><small>{saved.windows.length}{english ? " windows" : " 开"}</small>
            {saved.id === active?.id && <em>{config.window_layout_enabled ? (english ? "Current" : "当前应用") : (english ? "Selected" : "已选择")}</em>}
          </button>)}
        </div> : <div className="layout-library-empty"><LayoutDashboard size={24} strokeWidth={1.3} /><strong>{english ? "No saved layouts yet" : "还没有保存的布局"}</strong>
          <p>{english ? "Create a layout, arrange its windows, then save it here." : "新建布局，安排窗口后保存，就能在这里管理和快速选用。"}</p></div>}
        <p className="layout-helper">{config.window_layout_enabled && active
          ? (english ? `Currently using “${active.name}”.` : `当前使用“${active.name}”，优先于账号及启动方案的窗口位置。`)
          : (english ? "Using individual account positions." : "当前使用各账号的窗口坐标。")}</p>
      </section>
      {displayStatus}
    </> : <>
      <header className="layout-editor-heading">
        <Button type="button" size="sm" variant="ghost" title={dirty ? (english ? "Return without saving this draft" : "返回并放弃未保存的草稿") : undefined} onClick={() => { setEditor(null); setFeedback(null); }}><ArrowLeft size={14} />{english ? "Back to layouts" : "返回布局列表"}</Button>
        <div><h3>{editor.baseline ? (english ? "Edit layout" : "编辑布局") : (english ? "New layout" : "新建布局")}</h3>
          <span className="layout-draft-state">{dirty ? (english ? "Unsaved draft" : "草稿未保存") : (english ? "Saved" : "已保存")}</span></div>
        <Button type="button" variant="primary" loading={busy} onClick={() => save()} disabled={!layout || !capable}>
          {!busy && <Save size={13} />}{busy ? (english ? "Saving…" : "保存中…") : (english ? "Save layout" : "保存布局")}</Button>
      </header>
      {saveFeedback}
      {!layout && displayStatus}
      {layout && <>
        <div className="layout-draft-toolbar">
          <label className="layout-name-field"><span>{english ? "Layout name" : "布局名称"}</span><input className="settings-input" value={layout.name} maxLength={40} autoFocus
            aria-label={english ? "Layout name" : "布局名称"} onChange={event => replace({ ...layout, name: event.target.value })} /></label>
          <GeometryField label={english ? "Window count" : "窗口数量"} value={layout.windows.length} min={1} max={MAX_LAYOUT_WINDOWS}
            onChange={value => { replace(changeSlotCount(layout, value, resolution)); setSelected(index => Math.min(index, value - 1)); }} />
          <div className="layout-draft-actions"><Button type="button" size="sm" className="layout-preset-disclosure" aria-expanded={presetsOpen} aria-controls={presetsId}
            onClick={() => setPresetsOpen(value => !value)}>{english ? "Use a preset" : "使用预设"}<ChevronDown size={13} /></Button></div>
        </div>
        {presetsOpen && <section id={presetsId} className="layout-preset-shelf" aria-label={english ? "Preset templates" : "布局预设模板"}>
          <div className="layout-preset-heading"><span>{english ? "Single-monitor templates" : "单显示器预设"}</span>
            <div className="layout-resolution-profiles" role="group" aria-label={english ? "Preset resolution" : "预设分辨率"}>
              {(["1080p", "2k", "4k"] as const).map(value => <button type="button" key={value} aria-pressed={resolution === value}
                onClick={() => setProfile(value)}>{value === "2k" ? "2K" : value === "4k" ? "4K" : value}</button>)}
            </div>
          </div>
          <div className="layout-presets">{[2, 3, 4, 5, 6, 7, 8].map(count => <button type="button" key={count} disabled={!capable} className="layout-preset"
            onClick={() => { replace({ ...layout, windows: presetSlots(monitors, count, resolution) }); setSelected(0); setPresetsOpen(false); }}
            aria-label={english ? `Fill ${resolution} ${count}-window preset` : `填充 ${resolution.toUpperCase()} ${count} 开预设`}>
            <PresetThumbnail monitors={monitors} count={count} resolution={resolution} /><span>{english ? `${count} windows` : ["", "", "双开", "三开", "四开", "五开", "六开", "七开", "八开"][count]}</span>
          </button>)}</div>
          <p className="layout-helper">{english ? "Fills this draft on the primary display. Save to keep it." : "仅填充当前草稿，窗口排在主显示器；保存后成为你的布局配置。"}</p>
        </section>}
        {displayStatus}
        {changedDisplays && <p className="layout-display-notice" role="status">{english ? "Preview adapted to the current displays. Save to keep these changes." : "显示器配置已变化，预览已适配当前工作区，保存后更新此布局。"}</p>}
        <div className="layout-stage">
          <div className="layout-stage-toolbar"><span>{english ? "Desktop preview" : "桌面预览"}</span>
            {monitors.length > 1 && <select aria-label={english ? "Preview display" : "预览显示器"} className="settings-input" value={focusedMonitor}
              onChange={event => setFocusedMonitor(event.target.value)}><option value="">{english ? "All displays" : "全部显示器"}</option>
              {monitors.map(monitor => <option key={monitor.id} value={monitor.id}>{monitor.name}{monitor.primary ? (english ? " · Primary" : " · 主屏") : ""}</option>)}
            </select>}
          </div>
          <LayoutCanvas layout={layout} labels={labels} selected={selectedIndex} onSelect={setSelected} onChange={changeWindows}
            focusedMonitor={monitors.some(monitor => monitor.id === focusedMonitor) ? focusedMonitor : undefined} english={english} disabled={launching || busy || !capable} />
          <div className="layout-stage-hint"><span>{english ? "Overlap allowed · magnetic edges" : "允许重叠 · 边缘磁吸"}</span>
            <span>{english ? "Shift-drag bypasses snap · arrows move · Shift+arrows change resolution" : "Shift 拖动关闭吸附 · 方向键移动 · Shift＋方向键调整分辨率"}</span></div>
        </div>
        <div className="layout-slot-list" role="group" aria-label={english ? "Accounts in launch order" : "启动顺序对应账号"}>
          {layout.windows.map((slot, index) => <button key={index} type="button" aria-pressed={index === selectedIndex}
            onKeyDown={event => {
              if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
              event.preventDefault(); setSelected(index);
              const step = event.ctrlKey || event.metaKey ? 1 : 10;
              const dx = event.key === "ArrowLeft" ? -step : event.key === "ArrowRight" ? step : 0;
              const dy = event.key === "ArrowUp" ? -step : event.key === "ArrowDown" ? step : 0;
              const current = slotRect(slot, layout.monitors);
              const next = event.shiftKey ? resizeSlot(layout, index, slot, dx, dy, "se") : moveSlot(layout, index, { ...current, x: current.x + dx, y: current.y + dy });
              changeWindows(layout.windows.map((slot, at) => at === index ? next : slot));
            }}
            title={`${english ? "Launch position" : "启动顺序"} ${index + 1} · ${slot.width} × ${slot.height}`}
            onClick={() => { setSelected(index); if (focusedMonitor && focusedMonitor !== slot.monitor_id) setFocusedMonitor(slot.monitor_id); }}>
            <span>{labels[index]}</span>{index === 0 && <small>{english ? "Primary" : "主窗口"}</small>}
          </button>)}
        </div>
        {slot && monitor && rect && <div className="layout-inspector">
          <div className="layout-inspector-heading"><strong>{labels[selectedIndex]}</strong><span>{english ? "Window position and game resolution" : "窗口位置与游戏分辨率"}</span></div>
          <label className="layout-monitor-field"><span>{english ? "Display" : "所在显示器"}</span><select className="settings-input" value={slot.monitor_id}
            onChange={event => changeSlot({ monitor_id: event.target.value })}>
            {monitors.map(monitor => <option key={monitor.id} value={monitor.id} disabled={monitor.work_area.width < MIN_LAYOUT_WIDTH || monitor.work_area.height < MIN_LAYOUT_HEIGHT}>
              {monitor.name}{monitor.primary ? (english ? " · Primary" : " · 主屏") : ""}</option>)}
          </select></label>
          <GeometryField label="X" value={rect.x} min={monitor.work_area.x + MIN_VISIBLE_PIXELS - visible!.width} max={monitor.work_area.x + Math.max(0, monitor.work_area.width - visible!.width)} onChange={value => changeSlot({ x: value - monitor.work_area.x })} />
          <GeometryField label="Y" value={rect.y} min={monitor.work_area.y + MIN_VISIBLE_PIXELS - visible!.height} max={monitor.work_area.y + Math.max(0, monitor.work_area.height - visible!.height)} onChange={value => changeSlot({ y: value - monitor.work_area.y })} />
          <div className="layout-resolution-field"><span>{english ? "Game resolution" : "游戏分辨率"}</span>
            <ResolutionInput label={english ? "Game resolution" : "游戏分辨率"} value={`${slot.width}x${slot.height}`}
              onChange={value => { const [width, height] = value.split("x").map(Number); changeSlot({ width, height }); }} />
          </div>
          <p className="layout-visible-size">{english ? `Visible window: ${visible!.width} × ${visible!.height} · includes title bar` : `窗口实际占位：${visible!.width} × ${visible!.height} · 含标题栏`}</p>
          <div className="layout-inspector-actions">
            <Button type="button" size="sm" onClick={() => changeSlot({ x: Math.round((monitor.work_area.width - visible!.width) / 2), y: Math.round((monitor.work_area.height - visible!.height) / 2) })}
              title={english ? "Center on this display" : "在此显示器居中"}><AlignCenter size={14} /><span>{english ? "Center" : "居中"}</span></Button>
            <Button type="button" size="sm" onClick={() => changeSlot({ x: 0, y: 0, width: monitor.bounds.width, height: monitor.bounds.height })}
              title={english ? "Use display resolution" : "使用显示器分辨率"}><Maximize size={14} /><span>{english ? "Match display" : "匹配显示器"}</span></Button>
          </div>
        </div>}
        <div className="layout-panel-footer"><p>{english ? "Resolution has the same meaning as account resolution and takes effect on the next launch. Restoring a layout moves existing windows without resizing them. Coordinates align the visible window edge. The preview includes the title bar and visible borders; measurements are calibrated after launch." : "分辨率与账号分辨率含义相同，下次启动时生效。恢复布局仅移动运行中的窗口，不改变其大小。坐标对齐窗口可见边缘，画布包含标题栏和可见边框，启动后自动校准。"}</p>
          <div>{confirmDelete && <Button type="button" size="sm" variant="ghost" onClick={() => setConfirmDelete(false)}>{english ? "Keep layout" : "保留布局"}</Button>}
            {editor.baseline && <Button type="button" size="sm" variant="danger" onClick={remove}><Trash2 size={13} />{confirmDelete ? (english ? "Confirm delete" : "确认删除") : (english ? "Delete layout" : "删除布局")}</Button>}
            <Button type="button" size="sm" onClick={() => save(true)}><Check size={14} />{english ? "Save & use layout" : "保存并使用布局"}</Button></div>
        </div>
      </>}
    </>}
    {monitorError && <p className="layout-error" role="alert">{monitorError}</p>}
    {accountOrderError && <p className="layout-helper" role="status">{english ? "Unable to read live window order; nicknames use launch records for this preview." : "暂未读取到实际窗口顺序，昵称按启动记录预览。"} {accountOrderError}</p>}
    {!monitorLoading && !capable && !monitorError && <p className="layout-error" role="alert">{english ? "The primary work area must be at least 800 × 600." : "主显示器工作区需要至少 800 × 600。"}</p>}
  </fieldset>;
}
