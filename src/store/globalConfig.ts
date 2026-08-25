import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import type { GlobalConfig } from "./types";

interface GlobalConfigState {
  config: GlobalConfig | null;
  initialLoading: boolean;
  saving: boolean;
  error: string | null;

  load: () => Promise<void>;
  save: (config: GlobalConfig) => Promise<void>;
  detectSavedGamesPath: () => Promise<string | null>;
  detectGlobalSavedGamesPath: () => Promise<string | null>;
  detectProgramDataAgentPath: () => Promise<string | null>;
  detectAppDataRoamingBnetPath: () => Promise<string | null>;
  detectBrowserPath: () => Promise<[string, string] | null>;
}

export const useGlobalConfig = create<GlobalConfigState>((set) => ({
  config: null,
  initialLoading: true,
  saving: false,
  error: null,

  load: async () => {
    set({ initialLoading: true, error: null });
    try {
      const config = await invoke<GlobalConfig>("get_global_config");
      set({ config, initialLoading: false });
    } catch (e) {
      set({ error: String(e), initialLoading: false });
    }
  },

  save: async (config: GlobalConfig) => {
    set({ saving: true, error: null });
    try {
      await invoke("save_global_config", { config });
      set({ config, saving: false });
      await emit("global-config-updated", config);
    } catch (e) {
      set({ error: String(e), saving: false });
      throw e;
    }
  },


  detectSavedGamesPath: async () => {
    try {
      return await invoke<string | null>("detect_saved_games_path");
    } catch {
      return null;
    }
  },

  detectGlobalSavedGamesPath: async () => {
    try {
      return await invoke<string | null>("detect_global_saved_games_path");
    } catch {
      return null;
    }
  },

  detectProgramDataAgentPath: async () => {
    try {
      return await invoke<string | null>("detect_program_data_agent_path");
    } catch {
      return null;
    }
  },

  detectAppDataRoamingBnetPath: async () => {
    try {
      return await invoke<string | null>("detect_app_data_roaming_bnet_path");
    } catch {
      return null;
    }
  },

  detectBrowserPath: async () => {
    try {
      return await invoke<[string, string] | null>("detect_browser_path");
    } catch {
      return null;
    }
  },
}));

// 启动全局配置事件监听，返回取消监听的函数（各入口组件 useEffect 中调用）
export async function initConfigListener(): Promise<() => void> {
  try {
    return await listen<GlobalConfig>("global-config-updated", (event) => {
      useGlobalConfig.setState({ config: event.payload });
    });
  } catch (err) {
    console.error("Failed to listen to global-config-updated:", err);
    return () => {};
  }
}
