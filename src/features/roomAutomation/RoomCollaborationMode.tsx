import { useId } from "react";
import type { CollaborationMode } from "./types";
import "./roomCollaborationMode.css";

export function roomModeLabel(mode: CollaborationMode | undefined, en: boolean) {
  return ({ uber_diablo: en ? "Uber Diablo" : "超级迪亚波罗", default: en ? "Default" : "默认", host: en ? "Shared host" : "拼房主机", follower: en ? "Shared follower" : "拼房跟随" })[mode ?? "default"];
}

export function RoomCollaborationMode({ value = "default", onChange, disabled, en = false, compact = false }: {
  value?: CollaborationMode; onChange: (value: CollaborationMode) => void; disabled?: boolean; en?: boolean; compact?: boolean;
}) {
  const name = useId();
  const descriptions = en
    ? { uber_diablo: "Primary and followers each create sequential, separate rooms with the same password. Use the create shortcut.", default: "Primary creates; followers join its room manually or after the configured delay.", host: "Primary signals and creates; local followers join its room.", follower: "Read the primary account’s chat; all local accounts join together." }
    : { uber_diablo: "创建快捷键：主号、小号按配置顺序各自创建独立房间，序号递增、密码相同，不执行跟房。", default: "主号创建；小号手动或按配置延时加入主号房间。", host: "主号发送聊天信号后创建；本机小号加入主号房间。", follower: "监听本机主号聊天，主号和小号一起加入。" };
  return <fieldset className="room-collaboration" data-compact={compact} disabled={disabled}>
    <legend>{en ? "Room collaboration" : "拼房模式"}</legend>
    <div className="room-collaboration-options">
      {(["default", "host", "follower", "uber_diablo"] as const).map(mode => <label key={mode} title={descriptions[mode]} data-selected={value === mode}>
        <input type="radio" name={name} value={mode} checked={value === mode} onChange={() => onChange(mode)} />
        <strong>{roomModeLabel(mode, en)}</strong>
        <small>{descriptions[mode]}</small>
      </label>)}
    </div>
    <p>{compact ? `${descriptions[value]} ${en ? "Applying a switch cancels the previous workflow." : "应用切换会取消旧流程。"}` : value === "follower"
      ? (en ? "Both room shortcuts join instead of creating. Keep the room rule synchronized with the host." : "本机不创建房间：两个房间快捷键均执行加入。请与拼房主机同步房间规则。")
      : (en ? "One mode at a time. Applying a mode change cancels the previous workflow and pending signals." : "四种模式互斥。应用切换时会取消旧流程和待执行信号。")}</p>
  </fieldset>;
}
