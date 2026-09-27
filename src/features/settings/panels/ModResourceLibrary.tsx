import { useCallback, useEffect, useRef, useState } from "react";
import { Download, ExternalLink, FolderOpen, RefreshCw } from "lucide-react";
import { Button } from "../../../components/ui/Button";
import { ProgressBar } from "../../../components/ui/ProgressBar";
import { invokeCommand } from "../../../platform/tauri";
import { taskGateway } from "../../tasks/gateway";
import { subscribeBeforeReadingTasks } from "../../tasks/taskSync";
import type { TaskSnapshot } from "../../tasks/types";
import type { ModCapsuleController } from "../../modCapsules/useModCapsulePool";
import { LIGHTWEIGHT_PROFILES } from "../../modCapsules/lightweightModel";
import "./modResources.css";

interface Asset { id: string; version: string; url: string; mirrors?: {platform: string; url: string}[]; size: number; game_data_version: string | null }
interface ResourceState {
  catalog: { release_url: string; assets: Asset[] };
  processor: { ready: boolean; update_available?: boolean; installed_version: string | null; recommended_version: string;
    installed_path: string | null; install_directory: string; legacy: boolean };
  mods_directory: string | null; game_data_version: string | null; warning: string | null;
  preferred_source?: string;
  mods?: {id: string; installed_version: string | null; update_available: boolean; protected: boolean; message: string}[];
}
interface Props { edition: string; en: boolean; catalog?: ModCapsuleController; processorOnly?: boolean; onBusy?: (busy: boolean) => void }

export function ModResourceLibrary({ edition, en, catalog, processorOnly = false, onBusy }: Props) {
  const [data, setData] = useState<ResourceState | null>(null);
  const [checking, setChecking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installed, setInstalled] = useState<string | null>(null);
  const [task, setTask] = useState<TaskSnapshot | null>(null);
  const completedTask = useRef<number | null>(null);
  const observedTask = useRef<number | null>(null);
  const request = useRef(0);
  const progressTarget = useRef<HTMLDivElement | null>(null);
  const [activeAsset, setActiveAsset] = useState<string | null>(null);
  const [feedbackAsset, setFeedbackAsset] = useState<string | null>(null);
  const read = useCallback(async (refresh: boolean) => {
    const ticket = ++request.current;
    setChecking(true);
    try {
      const state = await invokeCommand<ResourceState>("get_mod_resources", { edition, refresh });
      if (ticket === request.current) { setData(state); setError(null); }
    } catch (e) { if (ticket === request.current) { setError(String(e)); setFeedbackAsset(null); } }
    finally { if (ticket === request.current) setChecking(false); }
  }, [edition]);
  useEffect(() => {
    setData(null); setError(null); setInstalled(null); setFeedbackAsset(null);
    void read(false);
    return () => { ++request.current; };
  }, [read]);
  useEffect(() => {
    let live = true; let stop: (() => void) | undefined;
    void subscribeBeforeReadingTasks(taskGateway, (snapshot) => {
      if (!live) return;
      const tasks = [...snapshot.values()].filter((t) => t.kind === "mod-resource-install");
      const running = tasks.find((t) => t.state === "running");
      if (running) observedTask.current = running.task_id;
      setTask(running ?? tasks.find(t => t.task_id === observedTask.current) ?? null);
    }).then((unsubscribe) => { if (live) stop = unsubscribe; else unsubscribe(); }).catch(() => {});
    return () => { live = false; stop?.(); };
  }, []);
  useEffect(() => {
    if (!task || task.state === "running" || completedTask.current === task.task_id) return;
    completedTask.current = task.task_id;
    if (!busy && task.state === "succeeded") { void read(false); void catalog?.refresh(); }
  }, [task, read, catalog, busy]);
  const active = busy || task?.state === "running";
  useEffect(() => { onBusy?.(busy); }, [busy, onBusy]);
  const taskAsset = task?.subject?.split(":")[1];
  const currentTaskHere = task?.subject?.startsWith(`${edition}:`) || taskAsset === "processor";
  useEffect(() => {
    if (activeAsset || task?.state === "running") progressTarget.current?.scrollIntoView?.({ block: "nearest", behavior: "smooth" });
    // Reveal a new operation once, not on every progress event.
  }, [activeAsset, task?.task_id, Boolean(data)]);
  const install = async (asset: Asset, manual: boolean) => {
    if (active) return;
    setBusy(true); setActiveAsset(asset.id); setFeedbackAsset(asset.id); setTask(null); observedTask.current = null; setError(null); setInstalled(null);
    try {
      let localFile: string | null = null;
      if (manual) {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const selected = await open({ multiple: false, directory: false,
          filters: [{ name: asset.id === "processor" ? "EXE" : "ZIP", extensions: [asset.id === "processor" ? "exe" : "zip"] }] });
        if (typeof selected !== "string") return;
        localFile = selected;
      }
      const result = await invokeCommand<{ path: string }>("install_mod_resource", { edition, resourceId: asset.id, localFile });
      setInstalled(result.path);
      await read(false); await catalog?.refresh();
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); setActiveAsset(null); }
  };
  const external = async (url: string) => {
    try { const { open } = await import("@tauri-apps/plugin-shell"); await open(url); }
    catch (e) { setError(String(e)); }
  };
  const folder = async (processor: boolean) => {
    try { if (processor) await invokeCommand("open_mod_processor_directory");
      else await invokeCommand("open_mods_directory", { edition }); }
    catch (e) { setError(String(e)); }
  };
  return <section className="mod-resources" aria-label={en ? "Mod resources" : "Mod 资源下载"}>
    <header><div><h3>{processorOnly ? (en ? "Mod processor" : "Mod 加工器") : (en ? "Download and install" : "下载与安装")}</h3>
      <p>{en ? "Choose a resource. Hub verifies it and installs it in the location shown below." : "选择资源后自动下载、校验并安装到下方位置；无需手动解压或搬运。"}</p></div>
      <Button size="sm" variant="ghost" disabled={active} loading={checking} onClick={() => void read(true)}><RefreshCw size={13} />{en ? "Check updates" : "检查更新"}</Button></header>
    {!data && <p role="status">{en ? "Checking installed resources…" : "正在检查本地资源…"}</p>}
    {data?.warning && <p className="resource-note" role="status">{data.warning}</p>}
    {data && <div className="resource-cards">{data.catalog.assets.filter((a) => !processorOnly || a.id === "processor").map((asset) => {
      const processor = asset.id === "processor";
      const p = data.processor;
      const modStatus = data.mods?.find(m => m.id === asset.id);
      const existing = !processor && catalog?.pool?.capsules.find((c) => c.edition === edition && c.name.toLowerCase() === asset.id.toLowerCase() && c.origin === "scanned");
      const mismatch = !processor && data.game_data_version !== asset.game_data_version;
      const ready = processor ? p.ready && !p.update_available : !!(modStatus?.installed_version || existing) && !modStatus?.update_available;
      const protectedMod = !!modStatus?.protected;
      const rowTask = currentTaskHere && taskAsset === asset.id ? task : null;
      const rowActive = activeAsset === asset.id || rowTask?.state === "running";
      const location = processor ? p.install_directory : data.mods_directory ? `${data.mods_directory}\\${asset.id}` : "";
      const profile = LIGHTWEIGHT_PROFILES.find((value) => value.name === asset.id);
      return <article key={asset.id} className="resource-card" aria-label={processor ? (en ? "Mod processor" : "Mod 加工器") : asset.id} aria-busy={rowActive}>
        <div className="resource-title"><strong>{processor ? (en ? "Independent Mod processor" : "独立 Mod 加工器") : asset.id}</strong>
          <span>{(asset.size / 1048576).toFixed(1)} MB</span></div>
        <p>{processor ? (en ? "Adds selected features to your own Mods. Installed separately from Hub." : "为已有 Mod 添加所选功能，独立安装和更新。") : (en ? profile?.enDetail : profile?.detail)}</p>
        <small>{processor ? `${en ? "Recommended" : "推荐版本"} ${p.recommended_version}` : `${en ? "Game data" : "游戏数据版本"} ${asset.game_data_version}`}</small>
        {processor && p.installed_path && <p className="resource-note">{p.legacy ? (en ? "Legacy bundled processor found" : "检测到旧版内置加工器") : (en ? "Installed processor" : "已安装加工器")}
          {` · ${p.installed_version ?? (en ? "Unknown version" : "版本未知")}`}
          {!p.ready && (en ? ". Install the compatible version below before processing." : "。请安装下方兼容版本后再加工。")}
          <code>{p.installed_path}</code></p>}
        {modStatus?.message && <p className="resource-note">{modStatus.message}</p>}
        {modStatus?.installed_version && <small>{en ? "Installed" : "已安装"} {modStatus.installed_version}</small>}
        {processor && p.ready && p.update_available && <p className="resource-note">{en ? "A compatible update is available. Your installed processor remains usable." : "有兼容新版本，当前加工器仍可使用。"}</p>}
        {mismatch && <p className="resource-note">{en ? `Current game: ${data.game_data_version ?? "not configured"}. This package requires ${asset.game_data_version}.` : `当前游戏版本：${data.game_data_version ?? "尚未配置或无法识别"}，此成品需要 ${asset.game_data_version}。`}</p>}
        <div className="resource-location"><span>{en ? "Install location" : "安装位置"}</span><code>{location || (en ? "Configure a game directory first" : "请先在运行环境中设置游戏目录")}</code></div>
        <div className="resource-actions">
          <Button size="sm" variant="primary" disabled={active || ready || protectedMod || mismatch || (!processor && !location)} onClick={() => void install(asset, false)}><Download size={13} />
            {rowActive ? (en ? "Working…" : "正在处理…") : protectedMod ? (en ? "Preserved" : "保留现有 Mod") : ready ? (en ? "Installed" : "已是当前版本") : processor && p.installed_path ? (en ? "Update processor" : "更新加工器") : modStatus?.update_available ? (en ? "Verify & update" : "校验并原位更新") : (en ? "Download & install" : "下载并安装")}</Button>
          <Button size="sm" variant="ghost" disabled={active || ready || protectedMod || mismatch || (!processor && !location)} onClick={() => void install(asset, true)}>{en ? "Import downloaded file" : "导入已下载文件"}</Button>
          <Button size="sm" variant="ghost" onClick={() => void external(asset.mirrors?.find(m => m.platform === data.preferred_source)?.url ?? asset.mirrors?.[0]?.url ?? asset.url)}><ExternalLink size={12} />{en ? "Browser download" : "浏览器下载"}</Button>
          <Button size="sm" variant="ghost" disabled={!location} onClick={() => void folder(processor)}><FolderOpen size={12} />{en ? "Open folder" : "打开目录"}</Button>
        </div>
        {rowActive && <div ref={progressTarget} className="resource-progress" role="status" aria-live="polite">
          <div className="resource-progress-heading"><span>{rowTask?.state === "running" ? rowTask.message : (en ? "Preparing…" : "正在准备…")}</span>
            {rowTask?.state === "running" && <span>{rowTask.progress}%</span>}</div>
          <ProgressBar label={en ? `${asset.id} progress` : `${asset.id} 进度`} value={rowTask?.state === "running" ? rowTask.progress : undefined} />
          <div className="resource-progress-heading"><small>{en ? "You can leave this page; progress stays in Background tasks." : "可离开此页面，进度会保留在后台任务中。"}</small>
            {rowTask?.state === "running" && <Button size="sm" variant="ghost" disabled={rowTask.cancel_requested} onClick={() => void taskGateway.cancel(rowTask.task_id).catch(e => { setFeedbackAsset(asset.id); setError(String(e)); })}>{rowTask.cancel_requested ? (en ? "Cancelling…" : "正在取消…") : (en ? "Cancel" : "取消")}</Button>}</div>
        </div>}
        {feedbackAsset === asset.id && error && <p className="resource-error" role="alert">{error}</p>}
        {feedbackAsset === asset.id && installed && <p className="resource-success" role="status">{en ? "Installed at" : "已安装到"}<code>{installed}</code></p>}
        {!busy && rowTask?.state === "failed" && !error && <p className="resource-error" role="alert">{rowTask.message}</p>}
      </article>;
    })}</div>}
    {task?.state === "running" && !currentTaskHere && <p role="status" className="resource-note">{en ? "Another game edition is downloading resources. Follow it in Background tasks." : "另一个游戏版本正在安装资源，可在后台任务中查看进度。"}</p>}
    {!feedbackAsset && error && <p className="resource-error" role="alert">{error}</p>}
  </section>;
}

export function ModDownloadsPage({ edition, en, catalog, onBack, onEditionChange }: Props & { onBack: () => void; onEditionChange: (edition: "CN" | "Global") => void }) {
  const [busy, setBusy] = useState(false);
  return <div className="mod-downloads-page">
    <header className="mod-processing-header"><div><h2>{en ? "Mod downloads & updates" : "Mod 下载与更新"}</h2>
      <p>{en ? "Download Mods and manage the independent processor." : "下载 Mod，管理独立加工器及已安装资源。"}</p></div>
      <Button size="sm" variant="ghost" onClick={onBack}>{en ? "Back" : "返回"}</Button></header>
    <div className="mod-catalog-editions" role="tablist" aria-label={en ? "Game edition" : "游戏版本"}>
      {(["CN", "Global"] as const).map(value => <button key={value} role="tab" type="button" aria-selected={edition === value} disabled={busy} onClick={() => onEditionChange(value)}>{value === "CN" ? (en ? "China" : "国服") : (en ? "Global" : "国际服")}</button>)}
    </div>
    <ModResourceLibrary edition={edition} en={en} catalog={catalog} onBusy={setBusy} />
  </div>;
}
