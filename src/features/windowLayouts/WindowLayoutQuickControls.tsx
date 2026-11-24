import { useRef, useState } from "react";
import { Check, ChevronDown, LayoutDashboard, LoaderCircle, RotateCcw, Settings2 } from "lucide-react";
import { AnchoredPanel } from "../../components/ui/AnchoredPanel";
import { showToast } from "../../components/ui/Toast";
import { useGlobalConfig } from "../../store/globalConfig";
import { restoreGameLayout } from "./gateway";
import "./windowLayouts.css";

export function WindowLayoutQuickControls({ onOpenSettings, disabled = false }: { onOpenSettings: () => void; disabled?: boolean }) {
  const { config, saving, patch } = useGlobalConfig();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState("");
  const [error, setError] = useState("");
  const trigger = useRef<HTMLButtonElement>(null);
  const busyRef = useRef(false);
  const english = config?.app_language === "en-US";
  const layouts = config?.window_layouts ?? [];
  const selected = layouts.find(layout => layout.id === config?.active_window_layout_id);
  const enabled = config?.window_layout_enabled ?? false;
  const locked = disabled || saving || busy || !config;
  const active = enabled && !!selected;

  const restore = async (notify = true) => {
    const result = await restoreGameLayout();
    const message = result.failures.length ? result.failures.join("；") : !result.applied.length
      ? (english ? "No running account windows found" : "未找到正在运行的账号窗口")
      : english ? `Restored ${result.applied.length} windows` : `已恢复 ${result.applied.length} 个窗口的布局`;
    if (result.failures.length) setError(message);
    else if (notify) setFeedback(message);
    if (notify || result.failures.length) showToast(result.failures.length || !result.applied.length ? "warning" : "success", message);
  };
  const run = async (work: () => Promise<void>) => {
    if (busyRef.current || locked) return;
    busyRef.current = true; setBusy(true); setError(""); setFeedback("");
    try { await work(); }
    catch (cause) {
      const message = `${english ? "Unable to update layout" : "窗口布局操作失败"}：${cause}`;
      setError(message); showToast("error", message);
    } finally { busyRef.current = false; setBusy(false); }
  };
  const select = (id: string | null) => {
    if (id === (selected?.id ?? null)) return;
    void run(async () => {
      await patch({ active_window_layout_id: id });
      setFeedback(id ? (english ? "Layout selected" : "已选择布局") : (english ? "Using account positions" : "已切换为账号坐标"));
      if (id && enabled) await restore(false);
    });
  };

  return <div className="window-layout-quick" data-i18n-skip>
    <div className="layout-quick-trigger-group" data-enabled={enabled}>
      <button type="button" role="switch" aria-checked={enabled} aria-label={english ? "Enable window layouts" : "启用窗口布局"}
        className="layout-quick-power" disabled={locked} title={english ? "Toggle window layouts" : "开关窗口布局"}
        onClick={() => void run(async () => {
          await patch({ window_layout_enabled: !enabled });
          if (!enabled && selected) await restore(false);
        })}><span aria-hidden="true" /></button>
      <button ref={trigger} type="button" className="control-btn room-automation-quick-trigger layout-quick-trigger"
        data-active={open ? "true" : undefined} aria-haspopup="dialog" aria-expanded={open}
        disabled={disabled || !config} onClick={() => setOpen(value => !value)}
        title={active ? selected.name : english ? "Choose a window layout" : "选择窗口布局"}>
        <LayoutDashboard size={13} strokeWidth={1.9} aria-hidden="true" />
        <span>{english ? "Window layout" : "窗口布局"}</span>
        <ChevronDown size={12} strokeWidth={1.9} aria-hidden="true" />
      </button>
    </div>
    <AnchoredPanel open={open && !disabled} anchor={trigger} title={english ? "Window layout" : "窗口布局"} width={420}
      className="layout-quick-panel" closeLabel={english ? "Close" : "关闭"} onClose={() => setOpen(false)}>
      <div className="layout-quick-content" data-i18n-skip>
        <div className="layout-quick-summary">
          <span className="layout-status-dot" data-active={active} aria-hidden="true" />
          <div><strong>{active ? selected.name : enabled ? (english ? "Choose a layout" : "尚未选用布局") : (english ? "Layouts are off" : "布局已关闭")}</strong>
            <p>{active ? (english ? "Window mode and windowed resolution apply on the next launch." : "窗口模式和窗口化分辨率在下次启动时生效。")
              : (english ? "Using individual account positions." : "当前使用各账号的窗口坐标。")}</p></div>
        </div>
        <div className="layout-capsules" role="group" aria-label={english ? "Choose a layout" : "选择布局"}>
          <button type="button" className="layout-capsule" aria-pressed={!selected} disabled={locked} onClick={() => select(null)}>
            {!selected && <Check size={12} aria-hidden="true" />}{english ? "No layout" : "不使用布局"}</button>
          {layouts.map(layout => <button type="button" key={layout.id} className="layout-capsule" disabled={locked}
            aria-pressed={layout.id === selected?.id} onClick={() => select(layout.id)}>
            {layout.id === selected?.id && <Check size={12} aria-hidden="true" />}
            <span>{layout.name}</span><small>{layout.windows.length}{english ? " windows" : " 开"}{layout.windows.some(slot => slot.fullscreen) && (english ? " · Fullscreen" : " · 全屏")}</small>
          </button>)}
        </div>
        {!layouts.length && <p className="layout-helper">{english ? "Create and save a layout in settings to choose it here." : "新建并保存布局后，就能在这里快速切换。"}</p>}
        <div className="layout-quick-feedback" aria-live="polite">
          {error ? <p className="layout-error" role="alert">{error}</p> : feedback && <p role="status">{feedback}</p>}
        </div>
        <footer className="room-automation-quick-footer layout-quick-footer">
          <button type="button" className="room-automation-quick-settings" disabled={busy}
            onClick={() => { setOpen(false); onOpenSettings(); }}><Settings2 size={12} />{english ? "Manage layouts" : "管理布局"}</button>
          <button type="button" className="control-btn" disabled={locked || !active} onClick={() => void run(() => restore())}
            title={english ? "Restore positions by window title" : "按窗口名称恢复运行窗口的位置"}>
            {busy ? <LoaderCircle className="animate-spin" size={13} /> : <RotateCcw size={13} />}
            {busy ? (english ? "Applying…" : "调整中…") : (english ? "Restore layout" : "恢复布局")}</button>
        </footer>
      </div>
    </AnchoredPanel>
  </div>;
}
