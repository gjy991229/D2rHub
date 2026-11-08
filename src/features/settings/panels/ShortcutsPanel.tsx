import { X } from "lucide-react";
import { useEffect } from "react";
import type { Dispatch, KeyboardEvent, SetStateAction } from "react";
import type { AccountMeta, GlobalConfig } from "../../../store/types";
import "./corePanels.css";

interface ShortcutsPanelProps {
  config: GlobalConfig;
  accounts: AccountMeta[];
  onSchemeChange: (slot: number, groupId: string) => void;
  recordingPosition: string | null;
  setRecordingPosition: Dispatch<SetStateAction<string | null>>;
  onKeyDown: (event: KeyboardEvent<HTMLInputElement>, target: string) => void;
  onClear: (target: string) => void;
  errors?: Record<string, string>;
  checkingTarget?: string | null;
}

export function ShortcutsPanel({
  config,
  accounts,
  onSchemeChange,
  recordingPosition,
  setRecordingPosition,
  onKeyDown,
  onClear,
  errors = {},
  checkingTarget = null,
}: ShortcutsPanelProps) {
  useEffect(() => () => setRecordingPosition(null), [setRecordingPosition]);

  const isEnglish = config.app_language === "en-US";
  let bindings: Record<string, string> = {};
  try {
    bindings = config.shortcut_bindings_json ? JSON.parse(config.shortcut_bindings_json) : {};
  } catch {
    bindings = {};
  }

  const applicationShortcuts = [
    {
      target: "app:toggle",
      label: isEnglish ? "Toggle main window" : "切换主面板",
      detail: isEnglish
        ? "Hide to tray when focused; otherwise restore, show, and focus D2RHub"
        : "主面板在前台时隐藏到托盘；未聚焦、已隐藏或已最小化时恢复并聚焦",
      shortcut: config.show_main_window_shortcut || config.hide_main_window_shortcut || "",
    },
  ];

  const shortcutControl = (target: string, shortcut: string, label: string) => {
    const isRecording = recordingPosition === target;
    const error = errors[target];
    const errorId = `shortcut-error-${target.replace(":", "-")}`;
    return (
      <div className="shortcut-control">
      <div className="shortcut-control-entry">
        <input
          aria-label={`${label}${isEnglish ? " shortcut" : "快捷键"}`}
          type="text"
          readOnly
          disabled={checkingTarget !== null}
          aria-invalid={!!error}
          aria-describedby={error ? errorId : undefined}
          aria-busy={checkingTarget === target}
          data-shortcut-recording={isRecording ? "true" : "false"}
          value={checkingTarget === target ? (isEnglish ? "Checking..." : "正在检查占用...") : isRecording
            ? isEnglish ? "Press a key combo..." : "请按键输入组合..."
            : shortcut || (isEnglish ? "Click to record" : "点击录入")}
          title={shortcut || (isEnglish ? "Click, then press a shortcut. Escape cancels recording." : "点击后按下组合键，Esc 取消录入。")}
          onKeyDown={event => {
            if (isRecording) onKeyDown(event, target);
          }}
          onFocus={() => setRecordingPosition(target)}
          onClick={() => setRecordingPosition(target)}
          onBlur={() => setRecordingPosition(current => current === target ? null : current)}
          className={`px-3 rounded-lg text-xs font-mono text-center select-none border focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2 transition-all duration-150 ${
            isRecording
              ? "border-accent bg-accent/10 text-accent font-bold"
              : error ? "border-error bg-error/5 text-error cursor-pointer" : "border-border-default bg-surface-hover text-text-primary cursor-pointer hover:border-border-strong"
          }`}
        />
        {shortcut && (
          <button
            type="button"
            disabled={checkingTarget !== null}
            onClick={() => onClear(target)}
            aria-label={`${isEnglish ? "Clear " : "清除"}${label}${isEnglish ? " shortcut" : "快捷键"}`}
            className="h-[28px] w-[28px] shrink-0 rounded-lg border border-border-default hover:border-error hover:bg-error/5 text-text-muted hover:text-error transition-all flex items-center justify-center"
            title={isEnglish ? "Clear" : "清除"}
          >
            <X size={12} />
          </button>
        )}
      </div>
      {error && <p id={errorId} role="alert" className="w-full break-words text-2xs text-error">{error}</p>}
      </div>
    );
  };

  return (
    <div className="settings-content-grid shortcut-settings-panel">
      <section className="spatial-panel p-3 space-y-2 settings-span-full" aria-labelledby="application-shortcut-settings-title">
        <h2 id="application-shortcut-settings-title" className="text-xs font-bold text-text-primary">
          {isEnglish ? "D2RHub main window" : "D2RHub 主面板"}
        </h2>
        <p className="text-2xs text-text-muted mb-2">
          {isEnglish
            ? "Use one global shortcut to show or hide D2RHub. F12 is reserved by Windows."
            : "使用同一个全局快捷键呼出或收起主面板。F12 为 Windows 保留键，不可使用。"}
        </p>

        <div className="space-y-2.5 pt-1">
          {applicationShortcuts.map((item) => (
            <div key={item.target} className="shortcut-setting-row">
              <div className="min-w-0">
                <span className="text-sm font-semibold text-text-secondary">{item.label}</span>
                <p className="text-2xs text-text-muted">{item.detail}</p>
              </div>
              {shortcutControl(item.target, item.shortcut, item.label)}
            </div>
          ))}
        </div>
      </section>

      <section className="spatial-panel p-3 space-y-2 settings-span-full" aria-labelledby="scheme-shortcut-settings-title">
        <h2 id="scheme-shortcut-settings-title" className="text-xs font-bold text-text-primary">
          {isEnglish ? "Launch scheme shortcuts" : "启动方案快捷键"}
        </h2>
        <p className="text-2xs text-text-muted">
          {isEnglish ? "Assign global shortcuts to up to three saved schemes. Save settings to apply." : "最多为三个已保存的启动方案配置全局快捷键，按下即可启动。保存设置后生效。"}
        </p>
        {[0, 1, 2].map(slot => {
          const binding = config.launch_scheme_shortcuts?.[slot];
          const label = isEnglish ? `Scheme ${slot + 1}` : `方案 ${slot + 1}`;
          return <div key={slot} className="shortcut-setting-row shortcut-scheme-row">
            <div className="shortcut-scheme-details">
              <label className="text-sm font-semibold text-text-secondary" htmlFor={`scheme-shortcut-${slot}`}>{label}</label>
              <select id={`scheme-shortcut-${slot}`} value={binding?.group_id ?? ""}
                disabled={checkingTarget !== null || config.launch_groups.length === 0}
                onChange={event => onSchemeChange(slot, event.target.value)}
                className="settings-input shortcut-scheme-select">
                <option value="">{isEnglish ? "Select a scheme" : "请选择启动方案"}</option>
                {config.launch_groups.map(group => <option key={group.id} value={group.id}
                  disabled={(config.launch_scheme_shortcuts ?? []).some((item, index) => index !== slot && item.group_id === group.id)}>{group.name}</option>)}
              </select>
            </div>
            {binding?.group_id && shortcutControl(`scheme:${slot}`, binding.shortcut, label)}
          </div>;
        })}
        {config.launch_groups.length === 0 && <p className="text-2xs text-text-muted">{isEnglish ? "Create a launch scheme on the dashboard first." : "请先在主界面创建启动方案。"}</p>}
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
              <div key={account.id} className="shortcut-setting-row">
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
