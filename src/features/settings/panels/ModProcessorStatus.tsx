import { useEffect, useRef, useState } from "react";
import { invokeCommand } from "../../../platform/tauri";
import "./modResources.css";

interface Props { edition: string; en: boolean; onReady?: (ready: boolean | null) => void; onInventory?: (available: boolean) => void }
export function ModProcessorStatus({ edition, en, onReady, onInventory }: Props) {
  const [status, setStatus] = useState<{ ready: boolean; capabilities?: string[]; blocking_reason?: string | null } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const readyCallback = useRef({onReady, onInventory});
  readyCallback.current = {onReady, onInventory};
  useEffect(() => {
    let live = true;
    const notify = (ready: boolean | null, inventory = false) => {
      readyCallback.current.onReady?.(ready);
      readyCallback.current.onInventory?.(inventory);
    };
    setStatus(null); setError(null); notify(null);
    void invokeCommand<{ ready: boolean; capabilities?: string[]; blocking_reason?: string | null }>("get_bundled_processor_status")
      .then(value => { if (live) { setStatus(value); notify(value.ready, value.capabilities?.includes("jcy_inventory") === true); } })
      .catch(cause => { if (live) { setError(String(cause)); notify(false); } });
    return () => { live = false; };
  }, [edition]);
  if (status?.ready) return null;
  const failed = !!error || status?.ready === false;
  return <div className="processor-status" data-state={failed ? "error" : "checking"} role={failed ? "alert" : "status"}><span>{error ?? status?.blocking_reason ?? (en ? "Checking bundled processor…" : "正在检查内置加工器…")}</span></div>;
}
