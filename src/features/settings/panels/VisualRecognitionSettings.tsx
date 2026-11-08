import { roomModeLabel } from "../../roomAutomation/RoomCollaborationMode";
import type { CollaborationMode } from "../../roomAutomation/types";
import { useEffect, useRef, useState } from "react";
import { ChevronDown, ScanLine, Settings2, Bug, RefreshCw, Image, FolderOpen } from "lucide-react";
import { Toggle } from "../../../components/ui/Toggle";
import { Button } from "../../../components/ui/Button";
import "./featureSettings.css";
import { invokeCommand } from "../../../platform/tauri";
import type { AccountMeta } from "../../../store/types";

interface Roi { x: number; y: number; width: number; height: number }
interface Config {
  diagnostic_enabled: boolean;
  chat_send: boolean; chat_follow: boolean; chat_account_id: string; chat_roi: Roi; poll_ms: number;
  area_enabled: boolean; area_roi: Roi;

}
interface Preview { image_url: string; mask_url: string; tz_image_url?: string | null; candidate_lines: number; line_analysis: Array<{matched: boolean; marker_runs: number; reason: string}> }
interface Snapshot { generation: number; config: Config }
interface Status { running: boolean; ready: boolean; message: string; checked: number; matched: number; rejected: number; unavailable: number }

// A save continues across tab unmounts; new editors wait before loading persisted state.
let pendingSave: Promise<void> = Promise.resolve();

export function VisualRecognitionSettings({ mode, language, accounts = [], primaryAccountId, collaborationMode = "default" }: {
  mode: "chat" | "audio"; language?: string | null; accounts?: AccountMeta[]; primaryAccountId?: string; collaborationMode?: CollaborationMode;
}) {
  const en = language === "en-US";
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [draft, setDraft] = useState<Config | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [opened, setOpened] = useState(false);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const loaded = useRef(false);
  const [savedMessage, setSavedMessage] = useState("");
  useEffect(() => {
    if (!opened) return;
    let alive = true;
    if (!loaded.current) void pendingSave.then(() => invokeCommand<Snapshot>("vision_get_config")).then(value => {
      if (alive && value) { loaded.current = true; setSnapshot(value); setDraft(value.config); }
    }).catch(e => { if (alive) setError(String(e)); });
    const refresh = () => void invokeCommand<Status>("vision_get_status").then(value => {
      if (alive) setStatus(value);
    }).catch(e => { if (alive) setError(String(e)); });
    refresh(); const timer = window.setInterval(refresh, 1500);
    return () => { alive = false; window.clearInterval(timer); };
  }, [opened]);
  const update = (patch: Partial<Config>) => {
    setSavedMessage("");
    setDraft(value => value ? { ...value, ...patch } : value);
  };
  const save = async (next: Config) => {
    if (!snapshot || busy) return;
    setBusy(true); setError(""); setSavedMessage("");
    const operation = pendingSave.then(async () => {
      // Merge only this editor's changes so the chat/audio pages do not overwrite each other.
      const latest = await invokeCommand<Snapshot>("vision_get_config");
      const patch = Object.fromEntries(Object.entries(next).filter(([key, value]) =>
        JSON.stringify(value) !== JSON.stringify(snapshot.config[key as keyof Config])));
      const value = await invokeCommand<Snapshot>("vision_save_config", {
        expectedGeneration: latest.generation, config: { ...latest.config, ...patch },
      });
      setSnapshot(value); setDraft(value.config);
      setSavedMessage(en ? "Saved; settings survive app restarts." : "已保存，重启软件后仍保留。");
    });
    pendingSave = operation.catch(() => {});
    try { await operation; } catch (e) { setError(`${en ? "Save not confirmed: " : "保存未确认，请重试："}${String(e)}`); }
    finally { setBusy(false); }
  };
  const commit = (patch: Partial<Config>) => {
    if (!draft) return;
    const next = { ...draft, ...patch };
    update(next); void save(next);
  };
  const toggle = (key: "diagnostic_enabled" | "chat_send" | "chat_follow" | "area_enabled", title: string, description: string, unavailable = false) => (
    <div className="feature-setting-row">
      <div><strong>{title}</strong><p>{description}</p></div>
      <Toggle checked={draft?.[key] ?? false} ariaLabel={title} disabled={busy || unavailable} onChange={value => commit({ [key]: value })} />
    </div>
  );
  const roi = (key: "chat_roi" | "area_roi") => draft && (
    <fieldset className="feature-roi"><legend>{en ? "Capture region · % of window" : "截图范围 · 占游戏窗口的百分比"}</legend>
      {(["x", "y", "width", "height"] as const).map((field, index) => <label key={field}>{en ? field : ["左侧起点", "顶部起点", "宽度", "高度"][index]}
        <input className="settings-input" type="number" min="0" max="100" step="1" value={Math.round(draft[key][field] * 100)} disabled={busy}
          onBlur={() => { if (dirty) void save(draft); }}
          onChange={e => { if (Number.isFinite(e.target.valueAsNumber)) update({ [key]: { ...draft[key], [field]: e.target.valueAsNumber / 100 } }); }} />
      </label>)}
    </fieldset>
  );
  const dirty = !!draft && !!snapshot && JSON.stringify(draft) !== JSON.stringify(snapshot.config);
  return <details className="feature-settings spatial-panel" onToggle={e => { if (e.target === e.currentTarget) setOpened(e.currentTarget.open); }}>
    <summary className="feature-settings-heading">
      <ScanLine size={17} aria-hidden="true" />
      <span><strong>{mode === "chat" ? (en ? "Cross-PC room signals" : "跨电脑聊天信号") : (en ? "Scene timing & drop screenshots" : "场景计时与掉落截图")}</strong>
        <small>{mode === "chat" ? (en ? "Follow another player's next room" : "通过聊天信号，跟随另一台电脑进入下一局") : (en ? "Recognize scenes and save confirmed high-rune screenshots" : "补充地区名称，保存高号符文掉落截图")}</small></span>
      <ChevronDown className="feature-chevron" size={15} aria-hidden="true" />
    </summary>
    <div className="feature-settings-body">
      {!draft && !error && <p className="feature-help" role="status">{en ? "Loading settings…" : "正在读取设置…"}</p>}
      {draft && <>
        <div className="feature-save-state" role="status" aria-live="polite"><span>{en ? "Changes save automatically" : "修改自动保存"}</span><span>{busy ? (previewing ? (en ? "Capturing…" : "正在截图…") : (en ? "Saving…" : "正在保存…")) : error && dirty ? (en ? "Save needs attention" : "保存需处理") : dirty ? (en ? "Editing…" : "编辑中…") : savedMessage ? (en ? "Saved" : "已保存") : ""}</span></div>
        {mode === "chat" ? <>
          <p className="feature-note">{en ? "Current mode: " : "当前模式："}{roomModeLabel(collaborationMode, en)}{en ? ". Change it in Room collaboration above or in quick settings." : "。在上方拼房模式或快捷配置中切换。"}</p>
          <div className="feature-setting-row feature-account"><div><strong>{en ? "Listening account" : "接收信号的账号"}</strong><p>{en ? "Shared follower listens to the primary account. The primary and followers join using the local room rule." : "拼房跟随监听本机主号聊天框，主号与小号按本机规则一起加入。"}</p></div>
            <span className="feature-listening-account">{accounts.find(account => account.id === primaryAccountId)?.display_name || primaryAccountId || (en ? "Choose a primary account" : "请先配置主号")}</span>
          </div>
          <div className="feature-note"><strong>{en ? "Before following another computer" : "跨电脑跟房前"}</strong><ol className="feature-instructions">
            <li>{en ? "Use Shared host on the computer creating rooms, and Shared follower on the joining computer." : "建房电脑选择“拼房主机”，跟房电脑选择“拼房跟随”。"}</li>
            <li>{en ? "Copy and paste the same room rule on both computers, including the sequence and password." : "两端复制、粘贴相同房间规则，确认序号与密码一致。"}</li>
            <li>{en ? "Keep the primary account’s chat visible. Signals such as *********** 077 *********** must fit inside the capture region." : "保持本机主号聊天框可见，将 *********** 077 *********** 这类完整信号包含在截图范围内。"}</li>
          </ol></div>
        </> : <>
          {toggle("area_enabled", en ? "Recognize scenes and time runs" : "场景识别与计时", en ? "Audio or OCR confirms scenes. Terror Zone text is read after a scene is confirmed." : "声纹或 OCR 确认场景并计时；确认场景后检测下方 TZ 文字区域，通过 OCR 匹配恐怖区域。")}
          <p className="feature-help">{en ? "Turning scene recognition off disables both audio and OCR scene timing and location attribution for drops." : "关闭场景识别后，地区声纹与 OCR 均不再自动切换地点、计时或确定掉落地点。"}</p>
          <p className="feature-note">{en ? "Confirmed rune drops #24–#33 automatically save a full game-window PNG, grouped by date. Screenshot failures leave drop records intact." : "声纹确认 #24–#33 符文掉落后，自动保存完整游戏窗口 PNG，按日期归档。截图保存失败不影响掉落记录。"}</p>
          <Button size="sm" onClick={() => { void invokeCommand("open_drop_screenshots_dir").catch(e => setError(String(e))); }}><FolderOpen size={14} aria-hidden="true" />{en ? "Open drop screenshots" : "打开掉落截图目录"}</Button>
          <p className="feature-help">{en ? "Images are processed locally. OCR resources are prepared only when needed." : "截图仅在本机处理，识别资源按需准备。"}</p>
        </>}
        <details className="feature-disclosure"><summary><Settings2 size={14} aria-hidden="true" />{en ? "Capture and timing" : "截图与识别参数"}<ChevronDown size={14} className="feature-chevron" aria-hidden="true" /></summary><div className="feature-disclosure-body">
          {mode === "chat" && <div className="feature-field-row"><label htmlFor="vision-poll">{en ? "Check interval" : "检查间隔"}</label><div className="feature-unit-input"><input id="vision-poll" className="settings-input" type="number" min="1000" max="10000" step="500" value={draft.poll_ms} disabled={busy} onChange={e => { if (Number.isFinite(e.target.valueAsNumber)) update({ poll_ms: e.target.valueAsNumber }); }} onBlur={() => { if (dirty) void save(draft); }} /><span>ms</span></div></div>}
          <p className="feature-help">{mode === "chat" ? (en ? "Include both star groups and the numeric center on the same line." : "范围应覆盖同一行的左右星号串和中间数字。") : (en ? "Include the red entry title and Terror Zone text below it; a 9% height is recommended. Drops use a separate capture." : "范围应覆盖红字地区名及下方恐怖／恐惧区域提示，高度建议 9%；掉落使用独立截图。")}</p>
          {roi(mode === "chat" ? "chat_roi" : "area_roi")}
          <Button size="sm" disabled={busy || (mode === "chat" && !primaryAccountId)} onClick={async () => {
            setBusy(true); setPreviewing(true); setError("");
            try { setPreview(await invokeCommand<Preview>("vision_preview", { kind: mode === "chat" ? "chat" : "area", accountId: mode === "chat" ? primaryAccountId : null, roi: mode === "chat" ? draft.chat_roi : draft.area_roi })); }
            catch (e) { setError(String(e)); } finally { setBusy(false); setPreviewing(false); }
          }}><Image size={14} aria-hidden="true" />{en ? "Preview capture region" : "预览截图"}</Button>
          {preview && <>
            <figure className="feature-preview"><img src={preview.image_url} alt={en ? "Current capture region" : "当前截图区域"} onError={() => setError(en ? "Preview image could not load. Restart the updated app and retry capture." : "预览图片加载失败，请重启更新后的软件并重新截图。")} /><figcaption>{en ? "Current capture · local preview" : "当前截图 · 仅本机预览"}</figcaption></figure>
            <figure className="feature-preview"><img src={preview.mask_url} alt={mode === "chat" ? (en ? "Color mask" : "颜色掩膜") : (en ? "Original OCR crop" : "OCR 原图裁片")} /><figcaption>{mode === "chat" ? (en ? "Color mask · candidate lines" : "颜色掩膜 · 候选行") : (en ? "Original OCR crop · red-text candidates" : "OCR 原图裁片 · 红字候选")}：{preview.candidate_lines}</figcaption></figure>
            {mode === "audio" && preview.tz_image_url && <figure className="feature-preview"><img src={preview.tz_image_url} alt={en ? "Terror Zone OCR crop" : "TZ 原图裁片"} /><figcaption>{en ? "Terror Zone original OCR crop" : "恐怖／恐惧区域 · OCR 原图裁片"}</figcaption></figure>}
            {mode === "chat" && <p className="feature-help">{en ? "Signal" : "信号"}：{preview.line_analysis.some(line => line.matched) ? (en ? "Detected" : "星号串特征已检测到（数字由运行时 OCR 读取）") : (en ? "Not detected" : "未检测到")} · {preview.line_analysis.map((line,i) => `${i+1}: ${line.marker_runs} (${({marker: "星号串布局匹配",too_few_marker_trains: "未找到两侧连续符号",no_numeric_center: "未找到符号包围的数字区域"} as Record<string,string>)[line.reason] ?? line.reason})`).join("; ")}</p>}
          </>}
        </div></details>
        <details className="feature-disclosure"><summary><Bug size={14} aria-hidden="true" />{en ? "Diagnostics and resources" : "诊断与资源"}<ChevronDown size={14} className="feature-chevron" aria-hidden="true" /></summary><div className="feature-disclosure-body">
          {toggle("diagnostic_enabled", en ? "Diagnostic logging" : "记录诊断日志", en ? "Diagnostic logs for chat and scene recognition; independent of drop screenshots." : "聊天与地区识别诊断日志，仅排查问题时开启；与掉落截图保存无关。")}
          {status && <><p className="feature-help">{status.message || (en ? "Service idle" : "服务待机")}</p><dl className="feature-metrics">{[[en ? "Checked" : "核验", status.checked], [en ? "Matched" : "匹配", status.matched], [en ? "Unmatched" : "未匹配", status.rejected], [en ? "Uncertain" : "不确定", status.unavailable]].map(([label, count]) => <div key={label}><dt>{label}</dt><dd>{count ?? 0}</dd></div>)}</dl></>}
          <div className="feature-actions"><Button size="sm" onClick={() => { void invokeCommand("open_logs_dir").catch(e => setError(String(e))); }}><FolderOpen size={14} aria-hidden="true" />{en ? "Open logs folder" : "打开日志"}</Button><Button size="sm" disabled={busy} onClick={() => { void save(draft); }}><RefreshCw size={14} aria-hidden="true" />{en ? "Retry resources" : "重试资源准备"}</Button></div>
        </div></details>
        {dirty && <div className="feature-save-footer"><p className="feature-help">{en ? "Keep this page open until saved." : "保存成功前请保留当前页面。"}</p><Button size="sm" disabled={busy} onClick={() => { void save(draft); }}>{en ? "Save changes" : "保存修改"}</Button></div>}
      </>}
      {error && <p role="alert" className="feature-error">{error}</p>}
    </div>
  </details>;
}
