import { useCallback, useEffect, useRef, useState } from "react";
import { Download, ExternalLink, FolderOpen, RefreshCw } from "lucide-react";
import { Button } from "../../../components/ui/Button";
import { Modal } from "../../../components/ui/Modal";
import { invokeCommand } from "../../../platform/tauri";
import { taskGateway } from "../../tasks/gateway";
import { subscribeBeforeReadingTasks } from "../../tasks/taskSync";
import type { TaskSnapshot } from "../../tasks/types";
import type { ModCapsuleController } from "../../modCapsules/useModCapsulePool";
import { useGlobalConfig } from "../../../store/globalConfig";
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
interface Props { edition: string; en: boolean; catalog?: ModCapsuleController; processorOnly?: boolean; onReady?: (ready: boolean) => void }

export function ModResourceLibrary({ edition, en, catalog, processorOnly = false, onReady }: Props) {
  const autoCheck = useGlobalConfig(s => s.config?.enable_auto_update ?? true);
  const [data, setData] = useState<ResourceState | null>(null);
  const [checking, setChecking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installed, setInstalled] = useState<string | null>(null);
  const [task, setTask] = useState<TaskSnapshot | null>(null);
  const completedTask = useRef<number | null>(null);
  const read = useCallback(async (refresh: boolean) => {
    setChecking(true);
    try {
      const state = await invokeCommand<ResourceState>("get_mod_resources", { edition, refresh });
      setData(state); onReady?.(state.processor.ready);
    } catch (e) { setError(String(e)); onReady?.(false); }
    finally { setChecking(false); }
  }, [edition, onReady]);
  useEffect(() => {
    let live = true;
    setData(null); setError(null); setInstalled(null); setChecking(true);
    // Show local state immediately; network failure never hides installed resources.
    void invokeCommand<ResourceState>("get_mod_resources", { edition, refresh: false }).then((value) => {
      if (live) { setData(value); onReady?.(value.processor.ready); }
      return autoCheck ? invokeCommand<ResourceState>("get_mod_resources", { edition, refresh: true }) : value;
    }).then((value) => { if (live) { setData(value); onReady?.(value.processor.ready); } })
      .catch((e) => { if (live) setError(String(e)); }).finally(() => { if (live) setChecking(false); });
    return () => { live = false; };
  }, [edition, onReady, autoCheck]);
  useEffect(() => {
    let live = true; let stop: (() => void) | undefined;
    void subscribeBeforeReadingTasks(taskGateway, (snapshot) => {
      if (!live) return;
      const tasks = [...snapshot.values()].filter((t) => t.kind === "mod-resource-install");
      setTask(tasks.find((t) => t.state === "running") ?? tasks.sort((a, b) => b.task_id - a.task_id)[0] ?? null);
    }).then((unsubscribe) => { if (live) stop = unsubscribe; else unsubscribe(); }).catch(() => {});
    return () => { live = false; stop?.(); };
  }, []);
  useEffect(() => {
    if (!task || task.state === "running" || completedTask.current === task.task_id) return;
    completedTask.current = task.task_id;
    if (task.state === "succeeded") { void read(false); void catalog?.refresh(); }
  }, [task, read, catalog]);
  const active = busy || task?.state === "running";
  const install = async (asset: Asset, manual: boolean) => {
    if (active) return;
    setBusy(true); setError(null); setInstalled(null);
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
    finally { setBusy(false); }
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
      const ready = processor ? p.ready && !p.update_available : !!existing && !modStatus?.update_available;
      const protectedMod = !!modStatus?.protected;
      const location = processor ? p.install_directory : data.mods_directory ? `${data.mods_directory}\\${asset.id}` : "";
      const profile = LIGHTWEIGHT_PROFILES.find((value) => value.name === asset.id);
      return <article key={asset.id} className="resource-card">
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
            {protectedMod ? (en ? "Preserved" : "保留现有 Mod") : ready ? (en ? "Installed" : "已是当前版本") : processor && p.installed_path ? (en ? "Update processor" : "更新加工器") : modStatus?.update_available ? (en ? "Verify & update" : "校验并原位更新") : (en ? "Download & install" : "下载并安装")}</Button>
          <Button size="sm" variant="ghost" disabled={active || ready || protectedMod || mismatch || (!processor && !location)} onClick={() => void install(asset, true)}>{en ? "Import downloaded file" : "导入已下载文件"}</Button>
          <Button size="sm" variant="ghost" onClick={() => void external(asset.mirrors?.find(m => m.platform === data.preferred_source)?.url ?? asset.mirrors?.[0]?.url ?? asset.url)}><ExternalLink size={12} />{en ? "Browser download" : "浏览器下载"}</Button>
          <Button size="sm" variant="ghost" disabled={!location} onClick={() => void folder(processor)}><FolderOpen size={12} />{en ? "Open folder" : "打开目录"}</Button>
        </div>
      </article>;
    })}</div>}
    {active && <div className="resource-progress" role="status"><p>{task?.state === "running" ? task.message : (en ? "Preparing…" : "正在准备…")}</p>
      <progress max={100} value={task?.state === "running" ? task.progress : 0} />
      {task?.state === "running" && <Button size="sm" variant="ghost" disabled={task.cancel_requested} onClick={() => void taskGateway.cancel(task.task_id).catch((e) => setError(String(e)))}>{en ? "Cancel" : "取消"}</Button>}
      <small>{en ? "Progress remains in Background tasks if you close this view." : "关闭此窗口后，可在后台任务中查看进度。"}</small></div>}
    {error && <p className="resource-error" role="alert">{error}</p>}
    {installed && <p className="resource-success" role="status">{en ? "Installed at" : "已安装到"}<code>{installed}</code>{en ? "Return to Mod Management to assign or process a Mod." : "返回 Mod 管理即可分配账号或继续加工。"}</p>}
  </section>;
}

export function ModResourceDialog({ open, onClose, ...props }: Props & { open: boolean; onClose: () => void }) {
  return <Modal open={open} onClose={onClose} title={`${props.en ? "Mod resources" : "Mod 资源下载"} · ${props.edition === "CN" ? (props.en ? "China" : "国服") : (props.en ? "Global" : "国际服")}`} width="max-w-2xl"
    footer={<Button variant="ghost" onClick={onClose}>{props.en ? "Back to Mod Management" : "返回 Mod 管理"}</Button>}>
    <div className="resource-dialog-scroll">{open && <ModResourceLibrary {...props} />}</div>
  </Modal>;
}
