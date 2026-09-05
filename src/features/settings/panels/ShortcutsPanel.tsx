import { X } from "lucide-react";
import type { Dispatch, KeyboardEvent, SetStateAction } from "react";
import type { AccountMeta, GlobalConfig } from "../../../store/types";

interface ShortcutsPanelProps {
  config: GlobalConfig;
  accounts: AccountMeta[];
  recordingPosition: string | null;
  setRecordingPosition: Dispatch<SetStateAction<string | null>>;
  onKeyDown: (event: KeyboardEvent<HTMLInputElement>, target: string) => void;
  onClear: (target: string) => void;
}

export function ShortcutsPanel({
  config,
  accounts,
  recordingPosition,
  setRecordingPosition,
  onKeyDown,
  onClear,
}: ShortcutsPanelProps) {
  const isEnglish = config.app_language === "en-US";
  let bindings: Record<string, string> = {};
  try {
    bindings = config.shortcut_bindings_json ? JSON.parse(config.shortcut_bindings_json) : {};
  } catch {
    bindings = {};
  }

  const applicationShortcuts = [
    {
      target: "app:show",
      label: isEnglish ? "Show main window" : "呼出主面板",
      detail: isEnglish
        ? "Restore, show, and focus D2RHub while it is running"
        : "D2RHub 运行时恢复、显示并聚焦主面板",
      shortcut: config.show_main_window_shortcut || "",
    },
    {
      target: "app:hide",
      label: isEnglish ? "Minimize to tray" : "最小化到托盘",
      detail: isEnglish
        ? "Hide the main window without closing running games"
        : "隐藏主面板，不关闭正在运行的游戏",
      shortcut: config.hide_main_window_shortcut || "",
    },
  ];

  const shortcutControl = (target: string, shortcut: string, label: string) => {
    const isRecording = recordingPosition === target;
    return (
      <div className="flex shrink-0 items-center gap-2">
        <input
          aria-label={`${label}${isEnglish ? " shortcut" : "快捷键"}`}
          type="text"
          readOnly
          value={isRecording
            ? isEnglish ? "Press a key combo..." : "请按键输入组合..."
            : shortcut || (isEnglish ? "None" : "无")}
          onKeyDown={event => onKeyDown(event, target)}
          onClick={() => setRecordingPosition(target)}
          className={`h-[28px] w-[140px] px-3 rounded-lg text-sm font-mono text-center select-none border focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2 transition-all duration-150 max-[520px]:w-[112px] ${
            isRecording
              ? "border-accent bg-accent/10 text-accent font-bold"
              : "border-border-default bg-surface-hover text-text-primary cursor-pointer hover:border-border-strong"
          }`}
        />
        {shortcut && (
          <button
            type="button"
            onClick={() => onClear(target)}
            aria-label={`${isEnglish ? "Clear " : "清除"}${label}${isEnglish ? " shortcut" : "快捷键"}`}
            className="h-[28px] w-[28px] rounded-lg border border-border-default hover:border-error hover:bg-error/5 text-text-muted hover:text-error transition-all flex items-center justify-center"
            title={isEnglish ? "Clear" : "清除"}
          >
            <X size={12} />
          </button>
        )}
      </div>
    );
  };

  return (
    <div className="settings-content-grid">
      <section className="spatial-panel p-3 space-y-2 settings-span-full" aria-labelledby="application-shortcut-settings-title">
        <h2 id="application-shortcut-settings-title" className="text-xs font-bold text-text-primary">
          {isEnglish ? "D2RHub main window" : "D2RHub 主面板"}
        </h2>
        <p className="text-2xs text-text-muted mb-2">
          {isEnglish
            ? "These global shortcuts work while D2RHub is running, including from the tray."
            : "D2RHub 运行或位于托盘时均可使用；快捷键不会从未运行状态启动软件。"}
        </p>

        <div className="space-y-2.5 pt-1">
          {applicationShortcuts.map((item) => (
            <div key={item.target} className="flex items-center justify-between gap-3 py-1 max-[640px]:items-start">
              <div className="min-w-0">
                <span className="text-sm font-semibold text-text-secondary">{item.label}</span>
                <p className="text-2xs text-text-muted">{item.detail}</p>
              </div>
              {shortcutControl(item.target, item.shortcut, item.label)}
            </div>
          ))}
        </div>
      </section>

      <section className="spatial-panel p-3 space-y-2 settings-span-full" aria-labelledby="shortcut-settings-title">
        <h2 id="shortcut-settings-title" className="text-xs font-bold text-text-primary">
          {isEnglish ? "Game-window focus shortcuts" : "游戏窗口聚焦快捷键"}
        </h2>
        <p className="text-2xs text-text-muted mb-2">
          {isEnglish
            ? "Focus the game window assigned to each physical account position."
            : "按下对应组合键，一键聚焦指定物理位置的账号游戏窗口。"}
        </p>

        <div className="space-y-2.5 pt-1">
          {accounts.map((account, index) => {
            const position = String(index + 1);
            const shortcut = bindings[position] || "";
            const target = `account:${position}`;
            return (
              <div key={account.id} className="flex items-center justify-between gap-3 py-1 max-[640px]:items-start">
                <div className="min-w-0">
                  <span className="text-sm font-semibold text-text-secondary">
                    {isEnglish ? `Position #${position}` : `位置 #${position}`}: {account.display_name || account.id}
                  </span>
                  <p className="text-2xs text-text-muted">
                    {isEnglish ? `Current physical focus position #${position}` : `当前一键聚焦的物理位置 #${position}`}
                  </p>
                </div>
                {shortcutControl(target, shortcut, isEnglish ? `Position ${position}` : `位置 ${position}`)}
              </div>
            );
          })}
          {accounts.length === 0 && (
            <p className="text-center text-xs text-text-muted py-4">
              {isEnglish ? "No accounts yet" : "还没有账号"}
            </p>
          )}
        </div>
      </section>
    </div>
  );
}
