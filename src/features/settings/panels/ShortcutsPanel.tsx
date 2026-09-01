import { X } from "lucide-react";
import type { Dispatch, KeyboardEvent, SetStateAction } from "react";
import type { AccountMeta, GlobalConfig } from "../../../store/types";

interface ShortcutsPanelProps {
  config: GlobalConfig;
  accounts: AccountMeta[];
  recordingPosition: string | null;
  setRecordingPosition: Dispatch<SetStateAction<string | null>>;
  onKeyDown: (event: KeyboardEvent<HTMLInputElement>, position: string) => void;
  onClear: (position: string) => void;
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
  return (
    <div className="settings-content-grid">
      <section className="spatial-panel p-3 space-y-2 settings-span-full" aria-labelledby="shortcut-settings-title">
        <h2 id="shortcut-settings-title" className="text-xs font-bold text-text-primary">切换聚焦快捷键配置</h2>
        <p className="text-2xs text-text-muted mb-2">按下对应组合键可以一键聚焦并切换至指定的账号游戏窗口</p>

        <div className="space-y-2.5 pt-1">
          {accounts.map((account, index) => {
            const position = String(index + 1);
            let bindings: Record<string, string> = {};
            try {
              bindings = config.shortcut_bindings_json ? JSON.parse(config.shortcut_bindings_json) : {};
            } catch {
              bindings = {};
            }
            const shortcut = bindings[position] || "";
            const isRecording = recordingPosition === position;

            return (
              <div key={account.id} className="flex items-center justify-between gap-3 py-1 max-[640px]:items-start">
                <div className="min-w-0">
                  <span className="text-sm font-semibold text-text-secondary">
                    位置 #{position}: {account.display_name || account.id}
                  </span>
                  <p className="text-2xs text-text-muted">当前一键聚焦的物理位置 #{position}</p>
                </div>

                <div className="flex shrink-0 items-center gap-2">
                  <input
                    aria-label={`位置 ${position} 快捷键`}
                    type="text"
                    readOnly
                    value={isRecording
                      ? isEnglish ? "Press a key combo..." : "请按键输入组合..."
                      : shortcut || (isEnglish ? "None" : "无")}
                    onKeyDown={event => onKeyDown(event, position)}
                    onClick={() => setRecordingPosition(position)}
                    className={`h-[28px] w-[140px] px-3 rounded-lg text-sm font-mono text-center select-none border focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2 transition-all duration-150 max-[520px]:w-[112px] ${
                      isRecording
                        ? "border-accent bg-accent/10 text-accent font-bold"
                        : "border-border-default bg-surface-hover text-text-primary cursor-pointer hover:border-border-strong"
                    }`}
                  />
                  {shortcut && (
                    <button
                      type="button"
                      onClick={() => onClear(position)}
                      aria-label={`清除位置 ${position} 快捷键`}
                      className="h-[28px] w-[28px] rounded-lg border border-border-default hover:border-error hover:bg-error/5 text-text-muted hover:text-error transition-all flex items-center justify-center"
                      title="清除"
                    >
                      <X size={12} />
                    </button>
                  )}
                </div>
              </div>
            );
          })}
          {accounts.length === 0 && (
            <p className="text-center text-xs text-text-muted py-4">还没有账号</p>
          )}
        </div>
      </section>
    </div>
  );
}
