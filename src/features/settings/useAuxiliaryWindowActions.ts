import { useState } from "react";
import { showToast } from "../../components/ui/Toast";
import {
  locateAuxiliaryWindow,
  recoverAuxiliaryWindows,
  type AuxiliaryWindowLabel,
} from "../../utils/windowPlacement";

export function useAuxiliaryWindowActions(language?: string) {
  const [windowPlacementBusy, setWindowPlacementBusy] = useState<string | null>(null);

  const locateWindow = async (label: AuxiliaryWindowLabel) => {
    const names = language === "en-US"
      ? { overlay: "Terror Zone Broadcast", "stats-overlay": "Run Statistics", "bongo-cat": "Cat Overlay" }
      : { overlay: "邪恶区域播报窗口", "stats-overlay": "场景统计窗口", "bongo-cat": "猫咪悬浮窗" };
    const name = names[label];
    setWindowPlacementBusy(label);
    try {
      await locateAuxiliaryWindow(label);
      showToast(
        "success",
        language === "en-US" ? `${name} was moved to this display` : `${name}已移到当前屏幕`,
      );
    } catch (error) {
      showToast(
        "error",
        language === "en-US" ? `Failed to locate ${name}: ${error}` : `定位${name}失败: ${error}`,
      );
    } finally {
      setWindowPlacementBusy(null);
    }
  };

  const recoverAllWindows = async () => {
    setWindowPlacementBusy("all");
    try {
      const recovered = await recoverAuxiliaryWindows("main");
      if (recovered.length === 0) {
        showToast("info", language === "en-US" ? "No overlay windows are enabled" : "当前没有已启用的悬浮窗");
      } else {
        showToast(
          "success",
          language === "en-US"
            ? `Moved ${recovered.length} overlay windows to this display`
            : `已将 ${recovered.length} 个悬浮窗移到当前屏幕`,
        );
      }
    } catch (error) {
      showToast(
        "error",
        language === "en-US" ? `Failed to recover overlay windows: ${error}` : `找回悬浮窗失败: ${error}`,
      );
    } finally {
      setWindowPlacementBusy(null);
    }
  };

  return { windowPlacementBusy, locateWindow, recoverAllWindows };
}
