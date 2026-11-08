import type { BatchMode } from "../../hooks/useAccountBatch";
import { Check, Wrench } from "lucide-react";
import "../../styles/accountStatusLight.css";

export interface AccountStatusLightProps {
  name: string;
  running: boolean;
  issue?: string | null;
  selected?: boolean;
  mode?: BatchMode;
  disabled?: boolean;
  uncertain?: boolean;
  paused?: boolean;
  onToggle?: () => void;
  onRepair?: () => void;
  activity?: string;
  english?: boolean;
}
export function AccountStatusLight({ name, running, issue, selected, mode, disabled, uncertain, paused, onToggle, onRepair, activity, english = false }: AccountStatusLightProps) {
  const copy = (zh: string, en: string) => english ? en : zh;
  const action = running ? copy("关闭", "close") : copy("启动", "launch");
  const mismatch = !!mode && mode !== (running ? "close" : "launch");
  const unknown = uncertain || issue === undefined;
  const activityLabel = english && activity ? ({ "关闭中": "Closing", "启动中": "Launching" } as Record<string, string>)[activity] || activity : activity;
  const label = activityLabel || (selected ? copy(`待${action}`, `To ${action}`) : issue ? copy(`${running ? "运行中 · " : ""}配置异常`, `${running ? "Running · " : ""}Needs setup`) : unknown ? copy("状态待确认", "Checking status") : running ? copy("运行中", "Running") : copy("未运行", "Stopped"));
  const title = unknown ? copy("状态待确认，请稍后重试", "Status unavailable. Try again shortly.") : selected ? copy(`已选中待${action}，再次点击取消`, `Selected to ${action}. Click to deselect.`)
    : mismatch ? copy("请先取消当前选择", "Clear the current selection first.") : issue && !running ? copy(`配置异常：${issue}，点击配置`, `Needs setup: ${issue}. Click to configure.`)
    : copy(`${running ? "运行中" : "未运行"}，点击选择${action}${issue ? `；配置异常：${issue}` : ""}`, `${running ? "Running" : "Stopped"}. Click to select for ${action}.${issue ? ` Needs setup: ${issue}` : ""}`);
  return <div className="account-status-control" data-paused={paused || undefined}>
    <span className="account-status-dot" aria-hidden="true" data-running={running} data-error={!!issue}
      data-selection={selected ? running ? "close" : "launch" : undefined} />
    {label && <span className="account-status-label" title={label}>{label}</span>}
    <button type="button" className="account-status-button" title={title}
      aria-label={`${name}：${title}`} aria-pressed={!!selected}
      disabled={disabled || (unknown && !selected) || mismatch}
      onPointerDown={event => event.stopPropagation()}
      onMouseDown={event => event.stopPropagation()}
      onTouchStart={event => event.stopPropagation()}
      onKeyDown={event => { if (event.key === " " || event.key === "Enter") event.stopPropagation(); }}
      onClick={event => { event.stopPropagation(); if (issue && !running && !selected) onRepair?.(); else onToggle?.(); }}>
      {issue && !running && !selected
        ? <Wrench size={12} aria-hidden="true" />
        : <span className="account-select-check" aria-hidden="true">{selected && <Check size={11} />}</span>}
    </button>
  </div>;
}
