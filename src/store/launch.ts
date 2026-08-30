import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import type { LaunchAccountEntry, LaunchProgress, LaunchResult } from "./types";
import { showToast } from "../components/ui/Toast";

export interface LaunchLog {
  timestamp: string;
  account_id: string;
  step: string;
  status: string;
  message: string;
}

interface LaunchState {
  launching: boolean;
  progress: Record<string, LaunchProgress>;
  results: LaunchResult[];
  error: string | null;
  logs: LaunchLog[];
  drawerOpen: boolean;

  startLaunch: (accountIds: string[]) => Promise<void>;
  startSchemeLaunch: (entries: LaunchAccountEntry[]) => Promise<void>;
  startBattleNetOnly: (accountIds: string[]) => Promise<void>;
  cancelLaunch: () => Promise<void>;
  updateProgress: (p: LaunchProgress) => void;
  addProgressAndLog: (p: LaunchProgress) => void;
  reset: () => void;
  addLog: (entry: LaunchLog) => void;
  clearLogs: () => void;
  toggleDrawer: () => void;
}

export const useLaunch = create<LaunchState>((set, _get) => ({
  launching: false,
  progress: {},
  results: [],
  error: null,
  logs: [],
  drawerOpen: false,

  startLaunch: async (accountIds: string[]) => {
    set({ launching: true, progress: {}, results: [], error: null });
    try {
      const results = await invoke<LaunchResult[]>("launch_accounts", {
        accountIds,
      });
      set({ results, launching: false });

      const hasFailed = results.some((r) => !r.success);
      results
        .filter((result) => result.success && result.error)
        .forEach((result) => showToast("warning", result.error as string));
      emit("launch-ended", { success: !hasFailed });
    } catch (e) {
      set({ error: String(e), launching: false });
      showToast("error", `启动失败: ${e}`);
      emit("launch-ended", { success: false });
    }
  },

  startSchemeLaunch: async (entries: LaunchAccountEntry[]) => {
    set({ launching: true, progress: {}, results: [], error: null });
    try {
      const results = await invoke<LaunchResult[]>("launch_accounts", { entries });
      set({ results, launching: false });

      const hasFailed = results.some((result) => !result.success);
      results
        .filter((result) => result.success && result.error)
        .forEach((result) => showToast("warning", result.error as string));
      emit("launch-ended", { success: !hasFailed });
    } catch (e) {
      set({ error: String(e), launching: false });
      showToast("error", `启动方案失败: ${e}`);
      emit("launch-ended", { success: false });
    }
  },

  startBattleNetOnly: async (accountIds: string[]) => {
    set({ launching: true, progress: {}, results: [], error: null });
    try {
      const results = await invoke<LaunchResult[]>("launch_battle_net_only", {
        accountIds,
      });
      set({ results, launching: false });

      const hasFailed = results.some((r) => !r.success);
      emit("launch-ended", { success: !hasFailed });
    } catch (e) {
      set({ error: String(e), launching: false });
      showToast("error", `启动战网失败: ${e}`);
      emit("launch-ended", { success: false });
    }
  },

  cancelLaunch: async () => {
    try {
      await invoke("cancel_launch");
    } catch {}
  },

  updateProgress: (p: LaunchProgress) => {
    set((state) => ({
      progress: { ...state.progress, [p.account_id]: p },
    }));
  },

  addProgressAndLog: (payload: LaunchProgress) => {
    set((state) => {
      const newLogs = [
        ...state.logs,
        {
          timestamp: new Date().toISOString(),
          account_id: payload.account_id,
          step: payload.step,
          status: payload.status,
          message: payload.message,
        },
      ];
      return {
        progress: {
          ...state.progress,
          [payload.account_id]: payload,
        },
        logs: newLogs.length > 200 ? newLogs.slice(-200) : newLogs,
      };
    });
  },

  reset: () => {
    invoke("cancel_launch").catch(() => {});
    set({ launching: false, progress: {}, results: [], error: null });
  },

  addLog: (entry: LaunchLog) => {
    set((state) => ({ logs: [...state.logs, entry] }));
  },

  clearLogs: () => {
    set({ logs: [] });
  },

  toggleDrawer: () => {
    set((state) => ({ drawerOpen: !state.drawerOpen }));
  },
}));
