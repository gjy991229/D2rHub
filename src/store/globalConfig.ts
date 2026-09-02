import { create } from "zustand";
import { invokeCommand, listenEvent } from "../platform/tauri";
import type { GlobalConfig } from "./types";
import type { GlobalConfigPatch } from "../utils/globalConfigPatch";
import { shouldApplyConfigCommandResponse } from "./configCommitOrdering";

let mutationQueue: Promise<unknown> = Promise.resolve();
let pendingMutations = 0;

const LEGACY_OPTIONAL_MODULE_STORAGE_KEY = "d2rhub-installed-optional-modules-v1";
const OPTIONAL_MODULE_IDS = ["overlays", "pet", "automation", "room-automation"] as const;

interface LegacyOptionalModuleState {
  present: boolean;
  modules: string[];
}

function readLegacyInstalledOptionalModules(): LegacyOptionalModuleState {
  try {
    const serialized = localStorage.getItem(LEGACY_OPTIONAL_MODULE_STORAGE_KEY);
    if (serialized === null) return { present: false, modules: [] };
    const parsed: unknown = JSON.parse(serialized);
    if (!Array.isArray(parsed)) return { present: false, modules: [] };
    return {
      present: true,
      modules: OPTIONAL_MODULE_IDS.filter((moduleId) => parsed.includes(moduleId)),
    };
  } catch {
    return { present: false, modules: [] };
  }
}

function clearLegacyInstalledOptionalModules(): void {
  try {
    localStorage.removeItem(LEGACY_OPTIONAL_MODULE_STORAGE_KEY);
  } catch {
    // The backend remains the source of truth even in a read-only webview.
  }
}

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
    const snapshotBeforeRequest = get().config;
    try {
      let config = await invokeCommand<GlobalConfig>("get_global_config");
      const legacyModuleState = readLegacyInstalledOptionalModules();
      const persistedModules = OPTIONAL_MODULE_IDS.filter((moduleId) =>
        config.installed_optional_modules?.includes(moduleId),
      );
      const needsLegacyMigration = legacyModuleState.present && (
        persistedModules.length !== legacyModuleState.modules.length
        || persistedModules.some((moduleId) => !legacyModuleState.modules.includes(moduleId))
      );
      if (needsLegacyMigration) {
        try {
          config = await invokeCommand<GlobalConfig>("patch_global_config", {
            patch: { installed_optional_modules: legacyModuleState.modules },
          });
          clearLegacyInstalledOptionalModules();
        } catch (error) {
          // Loading the application is more important than an optional one-time
          // migration. Keep the legacy key so a later launch can retry safely.
          console.warn("Failed to migrate legacy optional modules:", error);
        }
      } else {
        clearLegacyInstalledOptionalModules();
      }
      if (shouldApplyConfigCommandResponse(snapshotBeforeRequest, get().config)) {
        set({ config, initialLoading: false });
      } else {
        set({ initialLoading: false });
      }
    } catch (e) {
      set({ error: String(e), initialLoading: false });
    }
  },

  save: async (config: GlobalConfig) => {
    pendingMutations += 1;
    set({ saving: true, error: null });
    return enqueueMutation(async () => {
      try {
        const snapshotBeforeRequest = get().config;
        const saved = await invokeCommand<GlobalConfig>("save_global_config", { config });
        // The backend publishes committed snapshots in transaction order. Only
        // fall back to the command response when no event crossed this request;
        // otherwise a delayed response could overwrite a newer event.
        if (shouldApplyConfigCommandResponse(snapshotBeforeRequest, get().config)) {
          set({ config: saved });
        }
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
        const snapshotBeforeRequest = get().config;
        const saved = await invokeCommand<GlobalConfig>("patch_global_config", { patch });
        if (shouldApplyConfigCommandResponse(snapshotBeforeRequest, get().config)) {
          set({ config: saved });
        }
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
      return await invokeCommand<string | null>("detect_saved_games_path");
    } catch {
      return null;
    }
  },

  detectGlobalSavedGamesPath: async () => {
    try {
      return await invokeCommand<string | null>("detect_global_saved_games_path");
    } catch {
      return null;
    }
  },

  detectProgramDataAgentPath: async () => {
    try {
      return await invokeCommand<string | null>("detect_program_data_agent_path");
    } catch {
      return null;
    }
  },

  detectAppDataRoamingBnetPath: async () => {
    try {
      return await invokeCommand<string | null>("detect_app_data_roaming_bnet_path");
    } catch {
      return null;
    }
  },

  detectBrowserPath: async () => {
    try {
      return await invokeCommand<[string, string] | null>("detect_browser_path");
    } catch {
      return null;
    }
  },
}));

// 启动全局配置事件监听，返回取消监听的函数（各入口组件 useEffect 中调用）
export async function initConfigListener(): Promise<() => void> {
  try {
    return await listenEvent<GlobalConfig>("global-config-updated", (event) => {
      useGlobalConfig.setState({ config: event.payload });
    });
  } catch (err) {
    console.error("Failed to listen to global-config-updated:", err);
    return () => {};
  }
}

/** Registers the commit stream before reading the initial snapshot, closing
 * the bootstrap gap between a command response and cross-window events. */
export async function initConfigSync(): Promise<() => void> {
  const stopListening = await initConfigListener();
  try {
    await useGlobalConfig.getState().load();
    return stopListening;
  } catch (error) {
    stopListening();
    throw error;
  }
}
