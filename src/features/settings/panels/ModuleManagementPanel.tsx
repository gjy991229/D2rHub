import { useEffect, useState } from "react";
import {
  ArrowUpRight,
  Cat,
  Link2,
  Monitor,
  Plus,
  Route,
  ScanEye,
  Trash2,
  type LucideIcon,
} from "lucide-react";

import { Button } from "../../../components/ui/Button";
import type { GlobalConfig } from "../../../store/types";
import { roomAutomationGateway } from "../../roomAutomation/gateway";
import {
  SETTINGS_COPY,
  normalizeSettingsLanguage,
  type OptionalModuleTabId,
  type SettingsLanguage,
} from "../settingsRegistry";

interface ModuleManagementPanelProps {
  config: GlobalConfig;
  installedModules: readonly OptionalModuleTabId[];
  onInstall: (module: OptionalModuleTabId) => Promise<void> | void;
  onUninstall: (module: OptionalModuleTabId) => Promise<void> | void;
  onOpen: (module: OptionalModuleTabId) => void;
}

interface ModuleDefinition {
  id: OptionalModuleTabId;
  icon: LucideIcon;
  linked?: boolean;
}

interface ModuleCopy {
  description: string;
  capabilities: readonly string[];
  effect: string;
}

interface ModuleManagementCopy {
  description: string;
  countUnit: string;
  relationshipNote: string;
  availableModulesLabel: string;
  includedCapabilitiesLabel: string;
  linkedLabel: string;
  installed: string;
  previousSettings: string;
  available: string;
  open: string;
  remove: string;
  add: string;
  removeOverlayDependencyTitle: string;
  removeAutomationTitle: string;
  installAutomationTitle: string;
  installOverlaysTitle: string;
}

const MODULES: readonly ModuleDefinition[] = [
  {
    id: "overlays",
    icon: Monitor,
    linked: true,
  },
  {
    id: "pet",
    icon: Cat,
  },
  {
    id: "automation",
    icon: ScanEye,
    linked: true,
  },
  {
    id: "room-automation",
    icon: Route,
  },
] as const;

const MODULE_COPY: Record<SettingsLanguage, Record<OptionalModuleTabId, ModuleCopy>> = {
  "zh-CN": {
    overlays: {
      description: "把邪恶区域与刷图统计放到游戏之外。两个窗口独立开关，邪恶区域可单独使用。",
      capabilities: ["邪恶区域", "统计悬浮窗", "窗口定位"],
      effect: "添加后默认开启 TZ 播报",
    },
    pet: {
      description: "在桌面显示轻量伴随角色，并根据鼠标、键盘与运行状态做出反馈。",
      capabilities: ["输入反馈", "桌面状态", "皮肤与缩放"],
      effect: "添加后由你选择是否开启",
    },
    automation: {
      description: "监听指定 D2R 进程的声纹，识别场景与掉落并沉淀刷图统计。",
      capabilities: ["声纹识别", "掉落记录", "刷图统计"],
      effect: "联动添加悬浮窗，并开启统计与 TZ",
    },
    "room-automation": {
      description: "编排主号建房、跟随账号入房与房间序号，保留手动接管能力。",
      capabilities: ["主号建房", "跟随入房", "房名序列"],
      effect: "添加后完成账号与房间配置再开启",
    },
  },
  "en-US": {
    overlays: {
      description: "Keep Terror Zone alerts and run statistics visible outside the game. Each window is controlled independently, and Terror Zone works on its own.",
      capabilities: ["Terror Zone", "Stats overlay", "Window placement"],
      effect: "Adds with Terror Zone alerts enabled",
    },
    pet: {
      description: "Show a lightweight desktop companion that responds to keyboard, mouse, and application activity.",
      capabilities: ["Input feedback", "Desktop status", "Skins and scale"],
      effect: "Choose whether to turn it on after adding",
    },
    automation: {
      description: "Listen to the selected D2R process, recognize scenes and drops, and build a history of your runs.",
      capabilities: ["Audio recognition", "Drop history", "Run statistics"],
      effect: "Also adds overlays and turns on statistics and Terror Zone",
    },
    "room-automation": {
      description: "Coordinate room creation, follower joins, and room-name sequences while keeping manual control available.",
      capabilities: ["Primary room", "Follower joins", "Room sequence"],
      effect: "Configure accounts and room rules before turning it on",
    },
  },
};

const PANEL_COPY: Record<SettingsLanguage, ModuleManagementCopy> = {
  "zh-CN": {
    description: "只添加真正需要的功能。添加后才会出现在顶部导航，也可以随时在这里卸载。",
    countUnit: "已添加",
    relationshipNote: "添加识别与统计时会补齐悬浮窗模块，并同时开启场景统计与 TZ 播报；单独添加悬浮窗模块时只默认开启 TZ。",
    availableModulesLabel: "可用模块",
    includedCapabilitiesLabel: "包含能力",
    linkedLabel: "联动模块",
    installed: "已添加",
    previousSettings: "旧配置已保留",
    available: "未添加",
    open: "打开",
    remove: "卸载",
    add: "添加模块",
    removeOverlayDependencyTitle: "识别与统计依赖悬浮窗；卸载时会一并移除",
    removeAutomationTitle: "会关闭识别与场景统计；悬浮窗模块和独立 TZ 状态保持不变",
    installAutomationTitle: "会补齐悬浮窗模块，并开启场景统计与 TZ 播报",
    installOverlaysTitle: "只添加悬浮窗模块，并默认开启 TZ 播报",
  },
  "en-US": {
    description: "Add only the tools you use. Added modules appear in the top navigation and can be removed here at any time.",
    countUnit: "added",
    relationshipNote: "Adding Recognition & Stats also adds Desktop Overlays and turns on both statistics and Terror Zone. Adding Desktop Overlays alone turns on only Terror Zone.",
    availableModulesLabel: "Available modules",
    includedCapabilitiesLabel: "Included capabilities",
    linkedLabel: "Linked module",
    installed: "Added",
    previousSettings: "Previous settings kept",
    available: "Not added",
    open: "Open",
    remove: "Remove",
    add: "Add module",
    removeOverlayDependencyTitle: "Recognition & Stats depends on Desktop Overlays and will be removed with it",
    removeAutomationTitle: "Turns off recognition and statistics; Desktop Overlays and the independent Terror Zone setting remain unchanged",
    installAutomationTitle: "Also adds Desktop Overlays and turns on statistics and Terror Zone",
    installOverlaysTitle: "Adds only Desktop Overlays and turns on Terror Zone",
  },
};

export function ModuleManagementPanel({
  config,
  installedModules,
  onInstall,
  onUninstall,
  onOpen,
}: ModuleManagementPanelProps) {
  const [busyModule, setBusyModule] = useState<OptionalModuleTabId | null>(null);
  const [roomAutomationConfigured, setRoomAutomationConfigured] = useState(false);
  const language = normalizeSettingsLanguage(config.app_language);
  const copy = PANEL_COPY[language];

  useEffect(() => {
    let disposed = false;
    void roomAutomationGateway.getConfig().then((snapshot) => {
      if (!disposed) setRoomAutomationConfigured(snapshot.config.enabled);
    }).catch(() => undefined);
    return () => { disposed = true; };
  }, []);

  const changeInstallation = async (module: OptionalModuleTabId, install: boolean) => {
    if (busyModule) return;
    setBusyModule(module);
    try {
      if (install) await onInstall(module);
      else await onUninstall(module);
      if (module === "room-automation") {
        const snapshot = await roomAutomationGateway.getConfig().catch(() => null);
        if (snapshot) setRoomAutomationConfigured(snapshot.config.enabled);
      }
    } finally {
      setBusyModule(null);
    }
  };

  return (
    <div className="module-management-panel">
      <header className="module-management-header">
        <div>
          <h2>{SETTINGS_COPY[language]["module-management"].label}</h2>
          <p>{copy.description}</p>
        </div>
        <div
          className="module-management-count"
          aria-label={`${installedModules.length} / ${MODULES.length} ${copy.countUnit}`}
        >
          <strong>{installedModules.length}</strong>
          <span>/{MODULES.length} {copy.countUnit}</span>
        </div>
      </header>

      <div className="module-management-note" role="note">
        <Link2 size={14} aria-hidden="true" />
        <span>{copy.relationshipNote}</span>
      </div>

      <section className="module-management-list" aria-label={copy.availableModulesLabel}>
        {MODULES.map((module) => {
          const moduleCopy = MODULE_COPY[language][module.id];
          const installed = installedModules.includes(module.id);
          const configured = module.id === "overlays"
            ? !!(config.enable_tz_overlay || config.enable_stats_overlay)
            : module.id === "automation"
              ? !!config.rune_audio_enabled
              : module.id === "pet"
                ? !!config.enable_bongo_cat
                : roomAutomationConfigured;
          const state = installed ? "installed" : configured ? "legacy" : "available";
          const cascadesRecognition = module.id === "overlays" && installedModules.includes("automation");
          const Icon = module.icon;
          return (
            <article className="module-management-row" data-state={state} key={module.id}>
              <span className="module-management-icon"><Icon size={18} aria-hidden="true" /></span>
              <div className="module-management-copy">
                <div className="module-management-title-line">
                  <h3>{SETTINGS_COPY[language][module.id].label}</h3>
                  <span data-state={state}>
                    {installed
                      ? copy.installed
                      : configured
                        ? copy.previousSettings
                        : copy.available}
                  </span>
                </div>
                <p>{moduleCopy.description}</p>
                <div
                  className="module-management-capabilities"
                  aria-label={copy.includedCapabilitiesLabel}
                >
                  {moduleCopy.capabilities.map((capability) => <span key={capability}>{capability}</span>)}
                  {module.linked && <span data-linked="true"><Link2 size={10} />{copy.linkedLabel}</span>}
                </div>
                <p className="module-management-effect">{moduleCopy.effect}</p>
              </div>
              <div className="module-management-actions">
                {installed ? (
                  <>
                    <Button size="sm" variant="secondary" onClick={() => onOpen(module.id)}>
                      <ArrowUpRight size={12} />{copy.open}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      loading={busyModule === module.id}
                      disabled={busyModule !== null}
                      title={cascadesRecognition
                        ? copy.removeOverlayDependencyTitle
                        : module.id === "automation"
                          ? copy.removeAutomationTitle
                          : undefined}
                      onClick={() => void changeInstallation(module.id, false)}
                    >
                      <Trash2 size={12} />{copy.remove}
                    </Button>
                  </>
                ) : (
                  <Button
                    size="sm"
                    variant="primary"
                    loading={busyModule === module.id}
                    disabled={busyModule !== null}
                    title={module.id === "automation"
                      ? copy.installAutomationTitle
                      : module.id === "overlays"
                        ? copy.installOverlaysTitle
                        : undefined}
                    onClick={() => void changeInstallation(module.id, true)}
                  >
                    <Plus size={12} />{copy.add}
                  </Button>
                )}
              </div>
            </article>
          );
        })}
      </section>
    </div>
  );
}
