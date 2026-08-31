import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import type { GlobalConfig } from "./types";
import type { GlobalConfigPatch } from "../utils/globalConfigPatch";

let mutationQueue: Promise<unknown> = Promise.resolve();
let pendingMutations = 0;

function enqueueMutation<T>(mutation: () => Promise<T>): Promise<T> {
  const result = mutationQueue.then(mutation, mutation);
  mutationQueue = result.then(() => undefined, () => undefined);
  return result;
}

interface GlobalConfigState {
  config: GlobalConfig | null;
  initialLoading: boolean;
  saving: boolean;
  error: string | null;

  load: () => Promise<void>;
  save: (config: GlobalConfig) => Promise<GlobalConfig>;
  patch: (patch: GlobalConfigPatch) => Promise<GlobalConfig>;
  detectSavedGamesPath: () => Promise<string | null>;
  detectGlobalSavedGamesPath: () => Promise<string | null>;
  detectProgramDataAgentPath: () => Promise<string | null>;
  detectAppDataRoamingBnetPath: () => Promise<string | null>;
  detectBrowserPath: () => Promise<[string, string] | null>;
}

export const useGlobalConfig = create<GlobalConfigState>((set, get) => ({
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
    pendingMutations += 1;
    set({ saving: true, error: null });
    return enqueueMutation(async () => {
      try {
        const saved = await invoke<GlobalConfig>("save_global_config", { config });
        set({ config: saved });
        await emit("global-config-updated", saved);
        return saved;
      } catch (e) {
        set({ error: String(e) });
        throw e;
      } finally {
        pendingMutations -= 1;
        set({ saving: pendingMutations > 0 });
      }
    });
  },

  patch: async (patch: GlobalConfigPatch) => {
    if (Object.keys(patch).length === 0) {
      const current = get().config;
      if (!current) throw new Error("全局配置尚未加载");
      return current;
    }
    pendingMutations += 1;
    set({ saving: true, error: null });
    return enqueueMutation(async () => {
      try {
        const saved = await invoke<GlobalConfig>("patch_global_config", { patch });
        set({ config: saved });
        await emit("global-config-updated", saved);
        return saved;
      } catch (e) {
        set({ error: String(e) });
        throw e;
      } finally {
        pendingMutations -= 1;
        set({ saving: pendingMutations > 0 });
      }
    });
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
