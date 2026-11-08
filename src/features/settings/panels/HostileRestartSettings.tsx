import { useEffect, useId, useState } from "react";
import { listenEvent } from "../../../platform/tauri";
import type { GlobalConfig } from "../../../store/types";
import { ChevronRight } from "lucide-react";
import { Toggle } from "../../../components/ui/Toggle";

export function HostileRestartSettings({ config, persist, error }: {
  config: GlobalConfig;
  persist: (config: GlobalConfig) => Promise<unknown>;
  error?: string | null;
}) {
  const en = config.app_language === "en-US";
  const [expanded, setExpanded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");
  const descriptionId = useId();
  const [messages, setMessages] = useState<string[]>([]);
  useEffect(() => {
    let disposed = false;
    const subscription = listenEvent<{ pid: number; error: string | null }>("hostile-restart-status", ({ payload }) => {
      if (!disposed) setMessages(previous => [...previous.slice(-7), `PID ${payload.pid}: ${payload.error || (en ? "Four Esc presses sent" : "已发送四次 Esc")}`]);
    });
    void subscription.catch(() => undefined);
    return () => { disposed = true; void subscription.then(unlisten => unlisten()).catch(() => undefined); };
  }, [en]);
  const save = async (patch: Partial<GlobalConfig>) => {
    if (saving) return;
    setSaving(true); setSaveError("");
    try { await persist({ ...config, ...patch }); }
    catch (error) { setSaveError(String(error)); }
    finally { setSaving(false); }
  };
  return <section className="spatial-panel hostile-restart-settings" aria-label={en ? "Hostile auto-restart" : "敌对自动重开"}>
    <div className="hostile-restart-header">
      <button type="button" className="hostile-restart-disclosure" aria-expanded={expanded} aria-controls={descriptionId}
        onClick={() => setExpanded(value => !value)}>
        <ChevronRight size={14} aria-hidden="true" />
        {en ? "Hostile auto-restart" : "敌对自动重开"}
      </button>
      <div className="hostile-restart-controls">
        <Toggle checked={!!config.hostile_auto_restart_enabled} disabled={saving}
          ariaLabel={en ? "Enable hostile auto-restart" : "启用敌对自动重开"}
          onChange={enabled => { void save({ hostile_auto_restart_enabled: enabled }); }} />
        <label className="hostile-restart-scope"><span>{en ? "Action scope" : "触发范围"}</span>
        <select className="settings-input" disabled={saving}
          value={(config.hostile_auto_restart_all_windows ?? true) ? "all" : "listening"}
          onChange={event => { void save({ hostile_auto_restart_all_windows: event.target.value === "all" }); }}>
          <option value="all">{en ? "Listening and managed windows" : "监听及已管理窗口"}</option>
          <option value="listening">{en ? "Listening window only" : "仅监听窗口"}</option>
        </select></label>
      </div>
    </div>
    <div id={descriptionId} className="hostile-restart-description" hidden={!expanded}>
      <p>{en ? "Press Esc four times when the listening game plays a hostility cue. Keep Hub running and game sound and effects enabled." : "监听窗口播放敌对提示音时，自动连按四次 Esc。需保持 Hub 运行，并开启游戏声音及音效。"}</p>
      <p>{en
        ? "Uses the shared listening target. Works with scene/drop tracking off. Prepare the listening Mod with Hostile auto-restart; all targets need Double Esc: next Hell game. All windows includes the listening window and games managed by Hub."
        : "共用识别页面的监听目标，关闭场景、掉落识别后仍可使用。监听 Mod 需加工“敌对自动重开”，所有目标需启用“双击 Esc 下一局”。所有窗口包含监听窗口及 Hub 管理的游戏窗口。"}</p>
      {messages.length > 0 && <div role="status">{messages.map((message, index) => <p key={index}>{message}</p>)}</div>}
    </div>
    {saveError && <p role="alert" className="feature-error">{saveError}</p>}
    {config.hostile_auto_restart_enabled && error && <p role="status" className="text-xs text-warning">{error}</p>}
  </section>;
}
