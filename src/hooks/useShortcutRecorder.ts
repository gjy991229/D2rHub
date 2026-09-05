import { useEffect, useState } from "react";
import { invokeCommand } from "../platform/tauri";
import { normalizeShortcut } from "../utils/shortcut";

/**
 * 从键盘事件中解析快捷键组合字符串。
 * 返回 null 表示仅按下了修饰键（Ctrl/Alt/Shift），应忽略。
 * 不支持 Win 键。
 */
export function parseShortcutFromKeyEvent(e: React.KeyboardEvent<HTMLElement>): string | null {
  const key = e.key;

  // 忽略纯修饰键
  if (key === "Control" || key === "Alt" || key === "Shift" || key === "Meta") {
    return null;
  }

  const ctrl = e.ctrlKey;
  const alt = e.altKey;
  const shift = e.shiftKey;

  const parts: string[] = [];
  if (ctrl) parts.push("Ctrl");
  if (alt) parts.push("Alt");
  if (shift) parts.push("Shift");

  // 完整按键名映射（与后端 vk_to_key_string 保持一致）
  let keyName: string;
  if (e.code === "NumpadAdd") {
    keyName = "Num+";
  } else if (e.code === "NumpadMultiply") {
    keyName = "Num*";
  } else if (e.code === "NumpadSubtract") {
    keyName = "Num-";
  } else if (e.code === "NumpadDecimal") {
    keyName = "Num.";
  } else if (e.code === "NumpadDivide") {
    keyName = "Num/";
  } else if (key === " ") {
    keyName = "Space";
  } else if (key.length === 1) {
    keyName = key.toUpperCase();
  } else if (key.startsWith("F") && /^F\d+$/.test(key)) {
    keyName = key;
  } else if (key === "Enter") {
    keyName = "Enter";
  } else if (key === "Tab") {
    keyName = "Tab";
  } else if (key === "Escape") {
    keyName = "Escape";
  } else if (key === "Backspace") {
    keyName = "Backspace";
  } else if (key === "Delete") {
    keyName = "Delete";
  } else if (key === "Insert") {
    keyName = "Insert";
  } else if (key === "Home") {
    keyName = "Home";
  } else if (key === "End") {
    keyName = "End";
  } else if (key === "PageUp") {
    keyName = "PageUp";
  } else if (key === "PageDown") {
    keyName = "PageDown";
  } else if (key === "ArrowUp") {
    keyName = "Up";
  } else if (key === "ArrowDown") {
    keyName = "Down";
  } else if (key === "ArrowLeft") {
    keyName = "Left";
  } else if (key === "ArrowRight") {
    keyName = "Right";
  } else {
    keyName = key.toUpperCase();
  }

  parts.push(keyName);
  const combo = parts.join("+");
  return normalizeShortcut(combo);
}

/**
 * 快捷键录制状态管理 hook。
 * 管理当前正在录制的账号位置 (recordingPos)。
 */
export function useShortcutRecorder() {
  const [recordingPos, setRecordingPos] = useState<string | null>(null);

  useEffect(() => {
    const stopRecording = () => setRecordingPos(null);
    window.addEventListener("blur", stopRecording);
    return () => window.removeEventListener("blur", stopRecording);
  }, []);

  useEffect(() => {
    void invokeCommand("set_shortcut_capture_active", { active: recordingPos !== null }).catch(() => {});
    return () => {
      if (recordingPos !== null) {
        void invokeCommand("set_shortcut_capture_active", { active: false }).catch(() => {});
      }
    };
  }, [recordingPos]);

  return { recordingPos, setRecordingPos };
}
