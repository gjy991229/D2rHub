import { useEffect, useState } from "react";
import { Download } from "lucide-react";
import { Modal } from "./Modal";
import { Button } from "./Button";
import { invokeCommand } from "../../platform/tauri";
import { taskGateway } from "../../features/tasks/gateway";
import { subscribeBeforeReadingTasks } from "../../features/tasks/taskSync";
import type { TaskSnapshot } from "../../features/tasks/types";

interface Props { open: boolean; onClose: () => void; version: string; downloadUrl?: string }
export default function UpdateConfirmModal({ open, onClose, version }: Props) {
  const [busy, setBusy] = useState(false);
  const [downloaded, setDownloaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [task, setTask] = useState<TaskSnapshot | null>(null);
  useEffect(() => { setDownloaded(false); setError(null); setTask(null); }, [version]);
  useEffect(() => {
    if (!open) return;
    let live = true; let stop: (() => void) | undefined;
    void subscribeBeforeReadingTasks(taskGateway, snapshot => {
      if (!live) return;
      const current = [...snapshot.values()].filter(t => t.kind === "software-update-download" && t.subject === version).sort((a,b) => b.task_id-a.task_id)[0];
      if (current) { setTask(current); if (current.state === "succeeded") setDownloaded(true); }
    }).then(s => { if (live) stop=s; else s(); }).catch(e => { if (live) setError(String(e)); });
    return () => { live=false; stop?.(); };
  }, [open, version]);
  const running = busy || task?.state === "running";
  const act = async () => {
    setBusy(true); setError(null);
    try {
      if (downloaded) await invokeCommand("launch_downloaded_update", { version });
      else { await invokeCommand("download_software_update", { version }); setDownloaded(true); }
    } catch (e) { setError(String(e)); if (downloaded) setDownloaded(false); }
    finally { setBusy(false); }
  };
  return <Modal open={open} onClose={onClose} title="软件更新" width="max-w-sm" footer={<>
    <Button variant="ghost" onClick={onClose}>{running ? "后台下载" : "稍后"}</Button>
    {task?.state === "running" ? <Button disabled={task.cancel_requested} onClick={() => void taskGateway.cancel(task.task_id).catch(e => setError(String(e)))}>取消下载</Button>
      : <Button variant="primary" disabled={running} onClick={() => void act()}>{downloaded ? "安装更新" : "下载安装包"}</Button>}
  </>}>
    <div className="space-y-3">
      <p className="text-sm font-semibold"><Download size={17} className="inline mr-2" />发现新版本 v{version.replace(/^v/, "")}</p>
      <p className="text-xs text-text-muted">{downloaded ? "下载完成，校验通过。点击安装将退出 Hub 并启动完整安装器。" : "应用内下载安装包，首选源失败时自动切换；校验通过后才允许安装。"}</p>
      {running && <div role="status"><p className="text-xs">{task?.message || "正在准备下载…"}</p><progress className="w-full" value={task?.progress ?? 0} max={100} /></div>}
      {error && <p role="alert" className="text-xs text-red-400 break-words">{error}</p>}
    </div>
  </Modal>;
}
