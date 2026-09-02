import {
  Cat,
  Activity,
  Blocks,
  Folder,
  Monitor,
  Palette,
  PackageOpen,
  Play,
  Route,
  ScanEye,
  Settings,
  ShieldAlert,
  User,
  type LucideIcon,
} from "lucide-react";
import type { GlobalConfig } from "../../store/types";

export type SettingsTabId =
  | "paths"
  | "accounts"
  | "agent"
  | "appearance"
  | "overlays"
  | "automation"
  | "module-management"
  | "mod-processing"
  | "room-automation"
  | "pet"
  | "shortcuts"
  | "tasks"
  | "advanced";

export type SettingsCapabilityKind = "core" | "platform" | "optional";
export type SettingsFeatureGroup = "multi-instance" | "application" | "optional-features";

export interface SettingsFeatureDefinition {
  id: SettingsTabId;
  icon: LucideIcon;
  kind: SettingsCapabilityKind;
  group: SettingsFeatureGroup;
  /** Stable backend lifecycle IDs. Their observed status is never inferred from config. */
  capabilityIds?: readonly string[];
  /** Compatibility-only configuration intent for capabilities not yet supervised. */
  isConfigured?: (config: GlobalConfig) => boolean;
}

export const SETTINGS_GROUPS: ReadonlyArray<{
  id: SettingsFeatureGroup;
}> = [
  { id: "multi-instance" },
  { id: "application" },
  { id: "optional-features" },
];

export const SETTINGS_CAPABILITY_LABELS: Record<SettingsCapabilityKind, string> = {
  core: "多开",
  platform: "应用",
  optional: "可选功能",
};

export type SettingsLanguage = "zh-CN" | "en-US";

export const OPTIONAL_SETTINGS_TABS = [
  "overlays",
  "pet",
  "automation",
  "room-automation",
] as const satisfies readonly SettingsTabId[];

export type OptionalModuleTabId = typeof OPTIONAL_SETTINGS_TABS[number];

export function normalizeInstalledOptionalModules(
  modules: readonly string[] | null | undefined,
): OptionalModuleTabId[] {
  const normalized = OPTIONAL_SETTINGS_TABS.filter((id) => modules?.includes(id));
  if (normalized.includes("automation") && !normalized.includes("overlays")) {
    normalized.unshift("overlays");
  }
  return normalized;
}

export function optionalModulesAfterInstall(
  current: readonly OptionalModuleTabId[],
  requested: OptionalModuleTabId,
): OptionalModuleTabId[] {
  const next = new Set(current);
  next.add(requested);
  if (requested === "automation") {
    next.add("overlays");
  }
  return OPTIONAL_SETTINGS_TABS.filter((id) => next.has(id));
}

export function optionalModulesAfterUninstall(
  current: readonly OptionalModuleTabId[],
  requested: OptionalModuleTabId,
): OptionalModuleTabId[] {
  const next = new Set(current);
  next.delete(requested);
  if (requested === "overlays") {
    next.delete("automation");
  }
  return OPTIONAL_SETTINGS_TABS.filter((id) => next.has(id));
}

export const SETTINGS_OPTIONAL_HUB_COPY: Record<SettingsLanguage, {
  label: string;
  description: string;
  badge: string;
}> = {
  "zh-CN": {
    label: "模块管理",
    description: "按需添加或卸载桌面扩展",
    badge: `${OPTIONAL_SETTINGS_TABS.length} 个`,
  },
  "en-US": {
    label: "Module Management",
    description: "Add or remove desktop extensions on demand",
    badge: `${OPTIONAL_SETTINGS_TABS.length}`,
  },
};

export const SETTINGS_COPY: Record<SettingsLanguage, Record<SettingsTabId, {
  label: string;
  description: string;
}>> = {
  "zh-CN": {
    accounts: { label: "账号与实例", description: "账号身份、启动参数、窗口与游戏配置" },
    paths: { label: "运行环境", description: "游戏、战网、浏览器与存档位置" },
    agent: { label: "启动策略", description: "战网 Agent 与实例启动等待策略" },
    shortcuts: { label: "窗口快捷键", description: "快速聚焦并切换多开实例" },
    advanced: { label: "维护与迁移", description: "日志、路径向导与账号迁移" },
    tasks: { label: "后台任务", description: "进度、取消、重试与诊断时间线" },
    appearance: { label: "外观与界面", description: "语言、主题、字体与主界面透明度" },
    overlays: { label: "桌面悬浮窗", description: "邪恶区域与场景统计悬浮窗口" },
    automation: { label: "识别与统计", description: "掉落识别、运行统计与协议诊断" },
    "module-management": { label: "模块管理", description: "添加、卸载并组织可选功能" },
    "mod-processing": { label: "Mod 管理", description: "扫描、选择、编辑共享 Mod，并加工功能模块" },
    "room-automation": { label: "自动跟房", description: "主账号建房与跟随账号分阶段加入" },
    pet: { label: "桌宠", description: "桌面伴随角色及轻量状态反馈" },
  },
  "en-US": {
    accounts: { label: "Accounts & Instances", description: "Identity, launch options, windows, and game settings" },
    paths: { label: "Runtime Paths", description: "Game, Battle.net, browser, and saved-game locations" },
    agent: { label: "Launch Strategy", description: "Battle.net Agent and instance launch timing" },
    shortcuts: { label: "Window Shortcuts", description: "Focus and switch between game instances" },
    advanced: { label: "Maintenance & Transfer", description: "Logs, setup assistant, and account transfer" },
    tasks: { label: "Background Tasks", description: "Progress, cancellation, retries, and diagnostic timelines" },
    appearance: { label: "Appearance", description: "Language, theme, typography, and main window opacity" },
    overlays: { label: "Desktop Overlays", description: "Terror Zone and run statistics overlay windows" },
    automation: { label: "Recognition & Stats", description: "Audio recognition, run statistics, and diagnostics" },
    "module-management": { label: "Module Management", description: "Add, remove, and organize optional features" },
    "mod-processing": { label: "Mod Management", description: "Scan, edit, share, and process installed Mods" },
    "room-automation": { label: "Room Automation", description: "Primary room creation and staged follower joining" },
    pet: { label: "Desktop Companion", description: "Optional desktop pet and status feedback" },
  },
};

export const SETTINGS_GROUP_COPY: Record<SettingsLanguage, Record<SettingsFeatureGroup, {
  label: string;
  note: string;
}>> = {
  "zh-CN": {
    "multi-instance": { label: "多开", note: "始终启用" },
    application: { label: "应用", note: "多开所需" },
    "optional-features": { label: "可选功能", note: "按需添加" },
  },
  "en-US": {
    "multi-instance": { label: "Multi-instance", note: "Always on" },
    application: { label: "Application", note: "Required" },
    "optional-features": { label: "Optional Features", note: "Add on demand" },
  },
};

export function normalizeSettingsLanguage(language: string | null | undefined): SettingsLanguage {
  return language === "en-US" ? "en-US" : "zh-CN";
}

/**
 * Settings are registered by product responsibility rather than by visual order.
 * Optional modules own their enable/disable controls; the core and required
 * capabilities intentionally have no module-level off switch.
 */
export const SETTINGS_FEATURES: readonly SettingsFeatureDefinition[] = [
  {
    id: "accounts",
    icon: User,
    kind: "core",
    group: "multi-instance",
  },
  {
    id: "paths",
    icon: Folder,
    kind: "platform",
    group: "application",
  },
  {
    id: "agent",
    icon: Play,
    kind: "platform",
    group: "application",
  },
  {
    id: "appearance",
    icon: Palette,
    kind: "platform",
    group: "application",
  },
  {
    id: "tasks",
    icon: Activity,
    kind: "platform",
    group: "application",
  },
  {
    id: "advanced",
    icon: ShieldAlert,
    kind: "platform",
    group: "application",
  },
  {
    id: "shortcuts",
    icon: Settings,
    kind: "core",
    group: "multi-instance",
    isConfigured: (config) => {
      try {
        const bindings: unknown = JSON.parse(config.shortcut_bindings_json || "{}");
        return !!bindings
          && typeof bindings === "object"
          && !Array.isArray(bindings)
          && Object.values(bindings).some(
            (binding) => typeof binding === "string" && binding.trim().length > 0,
          );
      } catch {
        return false;
      }
    },
  },
  {
    id: "mod-processing",
    icon: PackageOpen,
    kind: "platform",
    group: "application",
  },
  {
    id: "module-management",
    icon: Blocks,
    kind: "optional",
    group: "optional-features",
  },
  {
    id: "overlays",
    icon: Monitor,
    kind: "optional",
    group: "optional-features",
    capabilityIds: ["terror-zone-overlay", "statistics-overlay"],
    isConfigured: (config) => config.enable_tz_overlay || config.enable_stats_overlay,
  },
  {
    id: "pet",
    icon: Cat,
    kind: "optional",
    group: "optional-features",
    capabilityIds: ["desktop-pet"],
  },
  {
    id: "automation",
    icon: ScanEye,
    kind: "optional",
    group: "optional-features",
    capabilityIds: ["audio-telemetry"],
    isConfigured: (config) => config.rune_audio_enabled,
  },
  {
    id: "room-automation",
    icon: Route,
    kind: "optional",
    group: "optional-features",
    capabilityIds: ["room-automation"],
  },
] as const;

export function isSettingsTabId(value: string | null | undefined): value is SettingsTabId {
  return SETTINGS_FEATURES.some((feature) => feature.id === value);
}

export function isOptionalSettingsTab(tab: SettingsTabId): boolean {
  return tab === "module-management"
    || (OPTIONAL_SETTINGS_TABS as readonly SettingsTabId[]).includes(tab);
}

export function isOptionalModuleTab(tab: SettingsTabId): tab is OptionalModuleTabId {
  return (OPTIONAL_SETTINGS_TABS as readonly SettingsTabId[]).includes(tab);
}

export function getSettingsFeaturesByKind(kind: SettingsCapabilityKind): readonly SettingsFeatureDefinition[] {
  return SETTINGS_FEATURES.filter((feature) => feature.kind === kind);
}
