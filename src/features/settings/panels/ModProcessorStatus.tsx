import { useEffect, useState } from "react";
import { invokeCommand } from "../../../platform/tauri";
import { Button } from "../../../components/ui/Button";
import "./modResources.css";

interface Props { edition: string; en: boolean; onReady: (ready: boolean) => void; onManage: () => void }
export function ModProcessorStatus({ edition, en, onReady, onManage }: Props) {
  const [status, setStatus] = useState<{ ready: boolean; update_available?: boolean } | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setStatus(null); setError(null); onReady(false);
    // Only inspect local availability. The app's daily update check owns automatic networking.
    void invokeCommand<{ processor: { ready: boolean; update_available?: boolean } }>("get_mod_resources", { edition, refresh: false })
      .then(value => { if (live) { setStatus(value.processor); onReady(value.processor.ready); } })
      .catch(cause => { if (live) setError(String(cause)); });
    return () => { live = false; };
  }, [edition, onReady]);
  if (status?.ready && !status.update_available) return null;
  return <div className="processor-status" role="status">
    <span>{error ?? (status ? status.ready
      ? (en ? "A processor update is available. You can continue processing." : "加工器有新版本，当前版本仍可继续加工。")
      : (en ? "Install a compatible processor before processing." : "加工前需要安装兼容的加工器。")
      : (en ? "Checking local processor…" : "正在读取本地加工器状态…"))}</span>
    {(status || error) && <Button size="sm" variant="secondary" onClick={onManage}>{en ? "Downloads & updates" : "下载与更新"}</Button>}
  </div>;
}
