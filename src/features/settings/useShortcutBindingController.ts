import { useEffect, useRef, useState } from "react";
import {
  parseShortcutFromKeyEvent,
  useShortcutRecorder,
  validateShortcutAvailability,
} from "../../hooks/useShortcutRecorder";
import type { GlobalConfig } from "../../store/types";
import { showToast } from "../../components/ui/Toast";
import { normalizeShortcut } from "../../utils/shortcut";

interface ShortcutBindingControllerParams {
  open: boolean;
  activeTab: string;
  config: GlobalConfig | null;
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
}

/**
 * 快捷键录入：负责录制态、冲突校验、可用性检查与写回草稿。
 *
 * 校验是异步的，期间设置可能变化，因此所有写回都带请求序号与配置身份校验。
 */
export function useShortcutBindingController({
  open,
  activeTab,
  config,
  updateConfig,
}: ShortcutBindingControllerParams) {
  const { recordingPos, setRecordingPos } = useShortcutRecorder();
  const [shortcutErrors, setShortcutErrors] = useState<Record<string, string>>({});
  const [checkingShortcut, setCheckingShortcut] = useState<string | null>(null);
  const shortcutRequest = useRef(0);
  const configRef = useRef(config);
  configRef.current = config;

  useEffect(() => {
    setCheckingShortcut(null);
    setShortcutErrors({});
    return () => { shortcutRequest.current += 1; };
  }, [open, activeTab]);

  const handleShortcutKeyDown = async (e: React.KeyboardEvent<HTMLInputElement>, target: string) => {
    if (checkingShortcut || e.repeat) return;
    if (e.key === "Tab") {
      setRecordingPos(null);
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      setRecordingPos(null);
      e.currentTarget.blur();
      return;
    }

    e.preventDefault();
    e.stopPropagation();

    if (e.metaKey) return;
    const combo = parseShortcutFromKeyEvent(e);
    if (!combo) return;
    const isEnglish = config?.app_language === "en-US";
    const rejectShortcut = (message: string) => {
      setShortcutErrors(current => ({ ...current, [target]: message }));
      showToast("error", message);
    };
    setRecordingPos(null);
    e.currentTarget.blur();
    setShortcutErrors(current => ({ ...current, [target]: "" }));
    if (/(?:^|\+)F12$/i.test(combo)) {
      rejectShortcut(isEnglish ? "F12 is reserved by Windows. Choose another shortcut." : "F12 是 Windows 调试器保留键，请选择其他快捷键");
      return;
    }

    if ((target.startsWith("app:") || target.startsWith("scheme:"))
      && !/^(Ctrl|Alt|Shift)\+/.test(combo)
      && !/^F(?:[1-9]|1\d|2[0-4])$/.test(combo)) {
      rejectShortcut(isEnglish ? "Use Ctrl, Alt, Shift, or a function key for this global shortcut." : "全局快捷键必须包含 Ctrl、Alt、Shift，或使用 F1-F24 功能键");
      return;
    }

    if (config) {
      let bindings: Record<string, string> = {};
      try {
        bindings = config.shortcut_bindings_json ? JSON.parse(config.shortcut_bindings_json) : {};
      } catch {
        bindings = {};
      }

      const assigned = [
        ...(config.launch_scheme_shortcuts ?? []).map((binding, slot) => ({
          target: `scheme:${slot}`, shortcut: binding.shortcut,
          label: isEnglish ? `launch scheme slot ${slot + 1}` : `启动方案配置 ${slot + 1}`,
        })),
        ...Object.entries(bindings).map(([position, shortcut]) => ({
          target: `account:${position}`,
          shortcut,
          label: isEnglish ? `account position #${position}` : `账号位置 #${position}`,
        })),
        { target: "app:toggle", shortcut: config.show_main_window_shortcut || config.hide_main_window_shortcut || "", label: isEnglish ? "Toggle main window" : "切换主面板" },
      ];
      const conflict = assigned.find((entry) => entry.target !== target
        && normalizeShortcut(entry.shortcut).toLowerCase() === combo.toLowerCase());
      if (conflict) {
        rejectShortcut(isEnglish ? `${combo} is already assigned to ${conflict.label}. Previous binding kept.` : `快捷键 ${combo} 已用于${conflict.label}，原设置保持不变`);
        return;
      }

      const request = ++shortcutRequest.current;
      setCheckingShortcut(target);
      try {
        await validateShortcutAvailability(combo, true);
        if (request !== shortcutRequest.current) return;
        const latest = configRef.current;
        if (latest?.shortcut_bindings_json !== config.shortcut_bindings_json
          || JSON.stringify(latest?.launch_scheme_shortcuts) !== JSON.stringify(config.launch_scheme_shortcuts)
          || latest?.show_main_window_shortcut !== config.show_main_window_shortcut
          || latest?.hide_main_window_shortcut !== config.hide_main_window_shortcut) {
          throw new Error(isEnglish ? "Settings changed during the check. Record the shortcut again." : "检查期间设置已变化，请重新录入快捷键");
        }
        updateConfig(c => {
          if (target === "app:toggle") {
            c.show_main_window_shortcut = combo;
            c.hide_main_window_shortcut = "";
          } else if (target.startsWith("scheme:")) {
            const slot = Number(target.slice("scheme:".length));
            const binding = c.launch_scheme_shortcuts?.[slot];
            if (binding?.group_id) binding.shortcut = combo;
          } else if (target.startsWith("account:")) {
            bindings[target.slice("account:".length)] = combo;
            c.shortcut_bindings_json = JSON.stringify(bindings);
          }
        });
        showToast("success", isEnglish ? `Shortcut available: ${combo}. Save settings to apply.` : `快捷键 ${combo} 可用，保存设置后生效`);
      } catch (error) {
        if (request === shortcutRequest.current) {
          rejectShortcut(error instanceof Error ? error.message : String(error));
        }
      } finally {
        if (request === shortcutRequest.current) setCheckingShortcut(null);
      }
    }
  };

  const handleClearShortcut = (target: string) => {
    shortcutRequest.current += 1;
    setCheckingShortcut(null);
    setShortcutErrors(current => ({ ...current, [target]: "" }));
    if (config) {
      let bindings: Record<string, string> = {};
      try {
        bindings = config.shortcut_bindings_json ? JSON.parse(config.shortcut_bindings_json) : {};
      } catch {
        bindings = {};
      }
      updateConfig(c => {
        if (target === "app:toggle") {
          c.show_main_window_shortcut = "";
          c.hide_main_window_shortcut = "";
        } else if (target.startsWith("scheme:")) {
          const binding = c.launch_scheme_shortcuts?.[Number(target.slice("scheme:".length))];
          if (binding) binding.shortcut = "";
        } else if (target.startsWith("account:")) {
          delete bindings[target.slice("account:".length)];
          c.shortcut_bindings_json = JSON.stringify(bindings);
        }
      });
      showToast("info", "快捷键已清除");
    }
  };

  return {
    recordingPos,
    setRecordingPos,
    shortcutErrors,
    checkingShortcut,
    handleShortcutKeyDown,
    handleClearShortcut,
  };
}
