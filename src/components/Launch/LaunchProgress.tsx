import { AlertCircle, ChevronDown, Trash2 } from "lucide-react";
import { useState, useEffect, useId, useRef } from "react";
import type { LaunchLog } from "../../store/launch";
import type { AccountMeta } from "../../store/types";
import "./launchProgress.css";

interface Props {
  accounts: AccountMeta[];
  logs: LaunchLog[];
  onClear: () => void;
  revealRevision?: number;
  language?: string;
}

export function LaunchProgressView({ accounts, logs, onClear, revealRevision = 0, language }: Props) {
  const english = language === "en-US";
  const [expanded, setExpanded] = useState(false);
  const userCollapsed = useRef(false);
  const logId = useId();

  useEffect(() => {
    if (revealRevision > 0) { userCollapsed.current = false; setExpanded(true); }
  }, [revealRevision]);

  // New output reveals the timeline; it stays readable until the user closes it.
  useEffect(() => {
    if (logs.length === 0) userCollapsed.current = false;
    if (logs.length > 0 && !userCollapsed.current) {
      setExpanded(true);
    }
  }, [logs.length]);

  if (logs.length === 0) return null;

  const getAccountName = (id: string) => {
    const a = accounts.find(a => a.id === id);
    return a?.display_name || id;
  };

  const statusColor = (status: string) => {
    switch (status) {
      case "ok": return "var(--success)";
      case "error": return "var(--error)";
      case "warning": return "var(--warning)";
      default: return "var(--text-muted)";
    }
  };

  return (
    <section className="launch-activity account-line overflow-hidden" aria-label={english ? "Activity log" : "运行日志"}>
      <div className="flex items-center pr-3">
      <button
        type="button"
        aria-expanded={expanded}
        aria-controls={logId}
        onClick={() => setExpanded(prev => { userCollapsed.current = prev; return !prev; })}
        className="w-full flex items-center gap-2 px-3.5 py-2 text-left hover:bg-surface-hover transition-colors"
      >
        <ChevronDown size={12}
          className={"text-text-muted transition-transform duration-200 " + (expanded ? "rotate-0" : "-rotate-90")} />
        <span className="text-xs font-semibold text-text-secondary flex-1">
          {english ? "Activity log" : "运行日志"}
          <span className="text-text-muted font-normal ml-1.5">({logs.length})</span>
          {logs.some(log => log.status === "error") && <span className="launch-activity-error"><AlertCircle size={12} aria-hidden="true" />{english ? "Errors recorded" : "有失败记录"}</span>}
        </span>
      </button>
        <button type="button" onClick={onClear}
          className="icon-btn w-8 h-8 shrink-0" title={english ? "Clear log" : "清除日志"} aria-label={english ? "Clear log" : "清除日志"}>
          <Trash2 size={13} aria-hidden="true" />
        </button>
      </div>

      <div
        id={logId}
        aria-hidden={!expanded}
        inert={!expanded}
        className="transition-all duration-300 ease-in-out grid"
        style={{
          gridTemplateRows: expanded ? "1fr" : "0fr",
          opacity: expanded ? 1 : 0,
        }}
      >
        <div className="overflow-hidden">
          <div className="launch-activity-list" tabIndex={expanded ? 0 : -1} role="region" aria-label={english ? "Log entries" : "日志条目"}>
            {[...logs].reverse().map((log, i) => (
              <div key={i} className="launch-activity-entry">
                <time className="launch-activity-time" dateTime={log.timestamp}>
                  {new Date(log.timestamp).toLocaleTimeString(english ? "en-GB" : "zh-CN", { hour12: false })}
                </time>
                <span className="launch-activity-dot" aria-hidden="true" style={{ background: statusColor(log.status) }} />
                <span className="launch-activity-account" title={getAccountName(log.account_id)}>
                  {getAccountName(log.account_id)}
                </span>
                <span className="launch-activity-message" style={log.status === "error" ? { color: "var(--error)" } : undefined}>{log.message}</span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
