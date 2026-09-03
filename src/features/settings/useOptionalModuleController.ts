import { useEffect, useState } from "react";

import { showToast } from "../../components/ui/Toast";
import { invokeCommand } from "../../platform/tauri";
import { useGlobalConfig } from "../../store/globalConfig";
import type { GlobalConfig } from "../../store/types";
import {
  acceptModuleDisclosure,
  hasAcceptedModuleDisclosure,
} from "../disclosures/disclosureStorage";
import { roomAutomationGateway } from "../roomAutomation/gateway";
import type { RoomAutomationConfig } from "../roomAutomation/types";
import {
  isOptionalModuleTab,
  optionalModulesAfterInstall,
  optionalModulesAfterUninstall,
  type OptionalModuleTabId,
  type SettingsLanguage,
  type SettingsTabId,
} from "./settingsRegistry";

interface OptionalModuleControllerOptions {
  open: boolean;
  installedModules: readonly OptionalModuleTabId[];
  language: SettingsLanguage;
  activeTab: SettingsTabId;
  persistGlobalDraft: (draft: GlobalConfig, quiet?: boolean) => Promise<GlobalConfig | null>;
  setActiveTab: (tab: SettingsTabId) => void;
}

function installSuccessMessage(language: SettingsLanguage, module: OptionalModuleTabId): string {
  if (language === "en-US") {
    if (module === "automation") {
      return "Recognition & Stats and its required Desktop Overlays module were added. Statistics and Terror Zone windows are now on.";
    }
    if (module === "overlays") {
      return "Desktop Overlays was added. Terror Zone is now on; the statistics window keeps its current setting.";
    }
    return `${module === "pet" ? "Desktop Companion" : "Room Automation"} was added to Optional Features.`;
  }
  if (module === "automation") {
    return "已添加“识别与统计”和所需的悬浮窗模块；场景统计与 TZ 播报窗口已开启";
  }
  if (module === "overlays") {
    return "已添加“桌面悬浮窗”；TZ 播报窗口已开启，场景统计窗口保持原状态";
  }
  return `“${module === "pet" ? "桌宠" : "自动跟房"}”已添加到可选功能导航`;
}

function uninstallSuccessMessage(
  language: SettingsLanguage,
  module: OptionalModuleTabId,
  cascadesRecognition: boolean,
): string {
  if (language === "en-US") {
    if (module === "automation") {
      return "Recognition & Stats was removed. Recognition and statistics are off; Terror Zone keeps its current setting.";
    }
    if (cascadesRecognition) {
      return "Desktop Overlays and dependent Recognition & Stats were removed. Their running features are now off.";
    }
    if (module === "overlays") {
      return "Desktop Overlays was removed. Terror Zone is now off.";
    }
    return "The module was removed. Its settings are kept for the next time you add it.";
  }
  if (module === "automation") {
    return "“识别与统计”已卸载，识别与场景统计已关闭；TZ 播报保持当前状态";
  }
  if (cascadesRecognition) {
    return "悬浮窗依赖已移除，“识别与统计”也已卸载，相关运行功能均已关闭";
  }
  if (module === "overlays") {
    return "“桌面悬浮窗”已卸载，TZ 播报窗口已关闭";
  }
  return "模块已卸载，配置内容会保留供下次添加时继续使用";
}

function actionFailureMessage(
  language: SettingsLanguage,
  action: "install" | "uninstall",
  error: unknown,
): string {
  if (language === "en-US") {
    return action === "install"
      ? "The module could not be added. Try again; if the problem continues, review the application logs."
      : "The module could not be removed. Try again; if the problem continues, review the application logs.";
  }
  return `${action === "install" ? "添加" : "卸载"}模块失败：${error}`;
}

export function useOptionalModuleController({
  open,
  installedModules,
  language,
  activeTab,
  persistGlobalDraft,
  setActiveTab,
}: OptionalModuleControllerOptions) {
  const [pendingDisclosureModule, setPendingDisclosureModule] = useState<OptionalModuleTabId | null>(null);
  const [disclosureInstallBusy, setDisclosureInstallBusy] = useState(false);

  useEffect(() => {
    if (!open) {
      setPendingDisclosureModule(null);
      setDisclosureInstallBusy(false);
    }
  }, [open]);

  const installModule = async (module: OptionalModuleTabId): Promise<boolean> => {
    try {
      const current = useGlobalConfig.getState().config;
      if (!current) throw new Error("全局配置尚未加载");
      const nextModules = optionalModulesAfterInstall(installedModules, module);
      const candidate: GlobalConfig = {
        ...current,
        installed_optional_modules: nextModules,
        enable_tz_overlay: module === "automation" || module === "overlays"
          ? true
          : current.enable_tz_overlay,
        enable_stats_overlay: module === "automation" ? true : current.enable_stats_overlay,
        enable_overlay: module === "automation" || module === "overlays"
          ? true
          : current.enable_overlay,
      };
      const saved = await persistGlobalDraft(candidate, true);
      if (!saved) return false;
      showToast("success", installSuccessMessage(language, module));
      return true;
    } catch (error) {
      showToast("error", actionFailureMessage(language, "install", error));
      return false;
    }
  };

  const requestInstallModule = async (module: OptionalModuleTabId) => {
    if (hasAcceptedModuleDisclosure(module)) {
      await installModule(module);
      return;
    }
    setPendingDisclosureModule(module);
  };

  const acceptAndInstallModule = async () => {
    if (!pendingDisclosureModule || disclosureInstallBusy) return;
    const module = pendingDisclosureModule;
    setDisclosureInstallBusy(true);
    try {
      const installed = await installModule(module);
      if (installed) {
        acceptModuleDisclosure(module);
        setPendingDisclosureModule(null);
      }
    } finally {
      setDisclosureInstallBusy(false);
    }
  };

  const handleUninstallModule = async (module: OptionalModuleTabId) => {
    const removesRecognitionGroup = module === "automation"
      || (module === "overlays" && installedModules.includes("automation"));
    let roomRollback: { generation: number; config: RoomAutomationConfig } | null = null;
    try {
      const current = useGlobalConfig.getState().config;
      if (!current) throw new Error("全局配置尚未加载");
      const nextModules = optionalModulesAfterUninstall(installedModules, module);
      if (module === "room-automation") {
        const snapshot = await roomAutomationGateway.getConfig();
        if (snapshot.config.enabled) {
          const disabled = await roomAutomationGateway.saveConfig(snapshot.generation, {
            ...snapshot.config,
            enabled: false,
          });
          roomRollback = {
            generation: disabled.snapshot.generation,
            config: snapshot.config,
          };
        }
      }
      const candidate: GlobalConfig = {
        ...current,
        installed_optional_modules: nextModules,
        rune_audio_enabled: removesRecognitionGroup ? false : current.rune_audio_enabled,
        enable_tz_overlay: module === "overlays" ? false : current.enable_tz_overlay,
        enable_stats_overlay: removesRecognitionGroup ? false : current.enable_stats_overlay,
        enable_bongo_cat: module === "pet" ? false : current.enable_bongo_cat,
      };
      candidate.enable_overlay = candidate.enable_tz_overlay || candidate.enable_stats_overlay;
      const saved = await persistGlobalDraft(candidate, true);
      if (!saved) {
        if (roomRollback) {
          try {
            await roomAutomationGateway.saveConfig(roomRollback.generation, roomRollback.config);
          } catch (rollbackError) {
            showToast("error", language === "en-US"
              ? "The module change failed and Room Automation could not be restored. Reopen its settings before continuing."
              : `模块卸载未提交，且自动跟房状态恢复失败：${rollbackError}`);
          }
        }
        return;
      }
      if (removesRecognitionGroup) {
        await invokeCommand("stop_rune_audio_monitor").catch(() => undefined);
      }
      if (isOptionalModuleTab(activeTab) && !nextModules.includes(activeTab)) setActiveTab("module-management");
      showToast(
        "success",
        uninstallSuccessMessage(language, module, removesRecognitionGroup && module === "overlays"),
      );
    } catch (error) {
      if (roomRollback) {
        try {
          await roomAutomationGateway.saveConfig(roomRollback.generation, roomRollback.config);
        } catch (rollbackError) {
          showToast("error", language === "en-US"
            ? "Room Automation could not be restored after the failed module change. Reopen its settings before continuing."
            : `卸载失败后无法恢复自动跟房状态：${rollbackError}`);
        }
      }
      showToast("error", actionFailureMessage(language, "uninstall", error));
    }
  };

  return {
    pendingDisclosureModule,
    disclosureInstallBusy,
    requestInstallModule,
    acceptAndInstallModule,
    handleUninstallModule,
    dismissPendingDisclosure: () => {
      if (!disclosureInstallBusy) setPendingDisclosureModule(null);
    },
  };
}
