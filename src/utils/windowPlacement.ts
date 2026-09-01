import { invokeCommand } from "../platform/tauri";

export type AuxiliaryWindowLabel = "overlay" | "stats-overlay" | "bongo-cat";
export type WindowPlacementTarget = "preserve" | "main" | "cursor" | "primary";

export interface WindowPlacementOutcome {
  label: AuxiliaryWindowLabel;
  moved: boolean;
  recovered: boolean;
  usedFallback: boolean;
  monitorName: string | null;
}

export interface LegacyWindowGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
}

export async function restoreWindowPlacement(
  label: AuxiliaryWindowLabel,
  legacyGeometry?: LegacyWindowGeometry | null,
): Promise<WindowPlacementOutcome> {
  return invokeCommand<WindowPlacementOutcome>("restore_window_placement", {
    label,
    legacyGeometry: legacyGeometry ?? null,
  });
}

export async function setAuxiliaryWindowVisible(
  label: AuxiliaryWindowLabel,
  visible: boolean,
  target: WindowPlacementTarget = "preserve",
): Promise<WindowPlacementOutcome> {
  return invokeCommand<WindowPlacementOutcome>("set_auxiliary_window_visible", {
    label,
    visible,
    target,
  });
}

export async function locateAuxiliaryWindow(
  label: AuxiliaryWindowLabel,
): Promise<WindowPlacementOutcome> {
  return setAuxiliaryWindowVisible(label, true, "main");
}

export async function recoverAuxiliaryWindows(
  target: Exclude<WindowPlacementTarget, "preserve"> = "main",
): Promise<AuxiliaryWindowLabel[]> {
  return invokeCommand<AuxiliaryWindowLabel[]>("recover_auxiliary_windows", { target });
}
