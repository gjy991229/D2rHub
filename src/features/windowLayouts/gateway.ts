import { invokeCommand } from "../../platform/tauri";
import type { LayoutMonitor, WindowLayout } from "../../store/types";

export interface LayoutRestoreResult {
  applied: string[];
  failures: string[];
}

export const getLayoutMonitors = () => invokeCommand<LayoutMonitor[]>("get_game_layout_monitors");
export const createGameWindowLayout = (layout: WindowLayout) => invokeCommand<void>("create_game_window_layout", { layout });
export const getLayoutAccountOrder = () => invokeCommand<string[]>("get_game_layout_account_order");
export const restoreGameLayout = () => invokeCommand<LayoutRestoreResult>("restore_game_window_layout");

export const captureGameLayout = () => invokeCommand<{ layout: WindowLayout; account_ids: string[] }>("capture_game_window_layout");
