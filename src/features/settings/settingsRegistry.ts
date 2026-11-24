import {
  Cat,
  Activity,
  Blocks,
  Folder,
  Monitor,
  LayoutDashboard,
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

export type SettingsTabId =
  | "paths"
  | "accounts"
  | "window-layouts"
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
export type SettingsFeatureGroup = "game" | "extensions" | "application";

export interface SettingsFeatureDefinition {
  id: SettingsTabId;
  icon: LucideIcon;
  kind: SettingsCapabilityKind;
  group: SettingsFeatureGroup;
  /** Minimal mode allowlist. Omission means the feature is completely hidden. */
  availableInMinimal?: boolean;
  /** Stable backend lifecycle IDs. Their observed status is never inferred from config. */
  capabilityIds?: readonly string[];
}

export const SETTINGS_GROUPS: ReadonlyArray<{
  id: SettingsFeatureGroup;
}> = [
  { id: "game" },
  { id: "extensions" },
  { id: "application" },
];

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

export const SETTINGS_COPY: Record<SettingsLanguage, Record<SettingsTabId, {
  label: string;
  navigationLabel?: string;
  description: string;
}>> = {
  "zh-CN": {
    accounts: { label: "账号与实例", description: "账号身份、启动参数、窗口与游戏配置" },
    "window-layouts": { label: "窗口布局", description: "按窗口名称匹配布局，支持拖动、缩放与多显示器" },
    paths: { label: "运行环境", description: "游戏、战网、浏览器与存档位置" },
    agent: { label: "启动策略", description: "战网 Agent、多开后保留战网与应用行为" },
    shortcuts: { label: "窗口快捷键", description: "呼出主面板并快速聚焦多开实例" },
    advanced: { label: "维护与迁移", description: "日志、路径向导与账号迁移" },
    tasks: { label: "后台任务", description: "进度、取消、重试与诊断时间线" },
    appearance: { label: "外观与界面", description: "语言、主题、字体与主界面透明度" },
    overlays: { label: "桌面悬浮窗", description: "邪恶区域与场景统计悬浮窗口" },
    automation: { label: "识别与统计", description: "掉落识别、运行统计与协议诊断" },
    "module-management": { label: "扩展功能", description: "选择需要的工具，再按自己的习惯配置" },
    "mod-processing": { label: "Mod 管理", description: "管理游戏 Mod、下载资源与添加功能" },
    "room-automation": { label: "自动跟房", description: "主账号建房与跟随账号分阶段加入" },
    pet: { label: "桌宠", description: "桌面伴随角色及轻量状态反馈" },
  },
  "en-US": {
    accounts: { label: "Accounts & Instances", navigationLabel: "Accounts", description: "Identity, launch options, windows, and game settings" },
    "window-layouts": { label: "Window Layouts", navigationLabel: "Layouts", description: "Arrange game windows by title across your displays" },
    paths: { label: "Runtime Paths", navigationLabel: "Game paths", description: "Game, Battle.net, browser, and saved-game locations" },
    agent: { label: "Launch Strategy", navigationLabel: "Launch", description: "Battle.net Agent, client retention after launch, and application behavior" },
    shortcuts: { label: "Window Shortcuts", navigationLabel: "Shortcuts", description: "Show D2RHub or focus a game instance" },
    advanced: { label: "Maintenance & Transfer", navigationLabel: "Maintenance", description: "Logs, setup assistant, and account transfer" },
    tasks: { label: "Background Tasks", navigationLabel: "Tasks", description: "Progress, cancellation, retries, and diagnostic timelines" },
    appearance: { label: "Appearance", description: "Language, theme, typography, and main window opacity" },
    overlays: { label: "Desktop Overlays", navigationLabel: "Overlays", description: "Terror Zone and run statistics overlay windows" },
    automation: { label: "Recognition & Stats", navigationLabel: "Recognition", description: "Audio recognition, run statistics, and diagnostics" },
    "module-management": { label: "Extensions", description: "Choose the tools you need and make them your own" },
    "mod-processing": { label: "Mod Management", navigationLabel: "Mods", description: "Scan, edit, share, and process installed Mods" },
    "room-automation": { label: "Room Automation", navigationLabel: "Auto join", description: "Primary room creation and staged follower joining" },
    pet: { label: "Desktop Companion", navigationLabel: "Companion", description: "Optional desktop pet and status feedback" },
  },
};

export const SETTINGS_GROUP_COPY: Record<SettingsLanguage, Record<SettingsFeatureGroup, {
  label: string;
}>> = {
  "zh-CN": {
    game: { label: "游戏" },
    extensions: { label: "扩展" },
    application: { label: "应用" },
  },
  "en-US": {
    game: { label: "Game" },
    extensions: { label: "Tools" },
    application: { label: "Application" },
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
    id: "window-layouts",
    icon: LayoutDashboard,
    kind: "core",
    group: "game",
    availableInMinimal: true,
  },
  {
    id: "accounts",
    icon: User,
    kind: "core",
    group: "game",
    availableInMinimal: true,
  },
  {
    id: "paths",
    icon: Folder,
    kind: "platform",
    group: "game",
    availableInMinimal: true,
  },
  {
    id: "agent",
    icon: Play,
    kind: "platform",
    group: "application",
    availableInMinimal: true,
  },
  {
    id: "appearance",
    icon: Palette,
    kind: "platform",
    group: "application",
    availableInMinimal: true,
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
    availableInMinimal: true,
  },
  {
    id: "shortcuts",
    icon: Settings,
    kind: "core",
    group: "application",
    availableInMinimal: true,
  },
  {
    id: "mod-processing",
    icon: PackageOpen,
    kind: "platform",
    group: "game",
    availableInMinimal: true,
  },
  {
    id: "module-management",
    icon: Blocks,
    kind: "optional",
    group: "extensions",
  },
  {
    id: "overlays",
    icon: Monitor,
    kind: "optional",
    group: "extensions",
    capabilityIds: ["terror-zone-overlay", "statistics-overlay"],
  },
  {
    id: "pet",
    icon: Cat,
    kind: "optional",
    group: "extensions",
    capabilityIds: ["desktop-pet"],
  },
  {
    id: "automation",
    icon: ScanEye,
    kind: "optional",
    group: "extensions",
    capabilityIds: ["audio-telemetry"],
  },
  {
    id: "room-automation",
    icon: Route,
    kind: "optional",
    group: "extensions",
    capabilityIds: ["room-automation"],
  },
] as const;

export function isSettingsTabId(value: string | null | undefined): value is SettingsTabId {
  return SETTINGS_FEATURES.some((feature) => feature.id === value);
}

export function isSettingsTabAvailableInMinimal(tab: SettingsTabId): boolean {
  return SETTINGS_FEATURES.find((feature) => feature.id === tab)?.availableInMinimal === true;
}

export function isOptionalModuleTab(tab: SettingsTabId): tab is OptionalModuleTabId {
  return (OPTIONAL_SETTINGS_TABS as readonly SettingsTabId[]).includes(tab);
}
