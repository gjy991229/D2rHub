import { create } from "zustand";
import { invokeCommand } from "../platform/tauri";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useGlobalConfig } from "./globalConfig";

import { isThemeKey, type ThemeKey } from "./themeCatalog";
export type { ThemeKey } from "./themeCatalog";

interface ThemeState {
  theme: ThemeKey;
  previewTheme: (t: ThemeKey) => void;
  setTheme: (t: ThemeKey) => void;
}

let label = "main";
try {
  label = getCurrentWindow().label;
} catch {}

const STORAGE_KEY = `d2rhub-theme-${label}`;

function applyTheme(t: ThemeKey) {
  try {
    if (typeof document !== "undefined" && document.documentElement) {
      document.documentElement.setAttribute("data-theme", t);
    }
    localStorage.setItem(STORAGE_KEY, t);
  } catch {}
}

function loadTheme(): ThemeKey {
  // 先尝试 localStorage 缓存，再回退到 config；config 加载后会通过 syncThemeFromConfig 纠正
  try {
    const saved = localStorage.getItem(STORAGE_KEY) as ThemeKey | null;
    if (isThemeKey(saved)) {
      applyTheme(saved);
      return saved;
    }
  } catch {}
  try {
    applyTheme("light");
  } catch {}
  return "light";
}

/** 从 config 同步主题（配置作为真相源，启动时调用一次） */
export function syncThemeFromConfig(configTheme: string | undefined) {
  if (!isThemeKey(configTheme)) return;
  const t = configTheme as ThemeKey;
  const current = useTheme.getState().theme;
  if (t !== current) {
    applyTheme(t);
    useTheme.setState({ theme: t });
  }
}

export const useTheme = create<ThemeState>((set) => ({
  theme: loadTheme(),
  previewTheme: (t) => {
    applyTheme(t);
    set({ theme: t });
  },
  setTheme: (t) => {
    applyTheme(t);
    set({ theme: t });
    invokeCommand("save_theme", { theme: t }).catch(() => {});

    // 同步到 useGlobalConfig 以防状态重置循环
    const configState = useGlobalConfig.getState();
    if (configState.config) {
      useGlobalConfig.setState({
        config: {
          ...configState.config,
          [label === "overlay" || label === "stats-overlay" ? "theme_overlay" : "theme"]: t
        }
      });
    }
  },
}));

if (typeof window !== "undefined") {
  window.addEventListener("storage", (e) => {
    if (e.key === STORAGE_KEY && e.newValue) {
      const newTheme = e.newValue as ThemeKey;
      if (isThemeKey(newTheme)) {
        applyTheme(newTheme);
        useTheme.setState({ theme: newTheme });
      }
    }
  });
}
