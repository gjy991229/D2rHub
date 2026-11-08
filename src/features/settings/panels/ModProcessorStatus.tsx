import { useEffect, useRef, useState } from "react";
import { invokeCommand } from "../../../platform/tauri";
import "./modResources.css";

interface Props { edition: string; en: boolean; onReady: (ready: boolean | null) => void }
export function ModProcessorStatus({ edition, en, onReady }: Props) {
  const [status, setStatus] = useState<{ ready: boolean; blocking_reason?: string | null } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const readyCallback = useRef(onReady);
  readyCallback.current = onReady;
  useEffect(() => {
    let live = true;
    setStatus(null); setError(null); readyCallback.current(null);
    void invokeCommand<{ ready: boolean; blocking_reason?: string | null }>("get_bundled_processor_status")
      .then(value => { if (live) { setStatus(value); readyCallback.current(value.ready); } })
      .catch(cause => { if (live) { setError(String(cause)); readyCallback.current(false); } });
    return () => { live = false; };
  }, [edition]);
  if (status?.ready) return null;
  const failed = !!error || status?.ready === false;
  return <div className="processor-status" data-state={failed ? "error" : "checking"} role={failed ? "alert" : "status"}><span>{error ?? status?.blocking_reason ?? (en ? "Checking bundled processor…" : "正在检查内置加工器…")}</span></div>;
}
