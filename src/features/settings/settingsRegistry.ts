import {
  Cat,
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
  | "mod-processing"
  | "room-automation"
  | "pet"
  | "shortcuts"
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
    appearance: { label: "外观与界面", description: "语言、主题、字体与主界面透明度" },
    overlays: { label: "桌面悬浮窗", description: "邪恶区域与场景统计悬浮窗口" },
    automation: { label: "识别与统计", description: "掉落识别、运行统计与协议诊断" },
    "mod-processing": { label: "Mod 加工", description: "保留已有功能并增补 D2RHub 模块" },
    "room-automation": { label: "自动跟房", description: "主账号建房与跟随账号分阶段加入" },
    pet: { label: "桌面伴随", description: "桌宠及轻量状态反馈" },
  },
  "en-US": {
    accounts: { label: "Accounts & Instances", description: "Identity, launch options, windows, and game settings" },
    paths: { label: "Runtime Paths", description: "Game, Battle.net, browser, and saved-game locations" },
    agent: { label: "Launch Strategy", description: "Battle.net Agent and instance launch timing" },
    shortcuts: { label: "Window Shortcuts", description: "Focus and switch between game instances" },
    advanced: { label: "Maintenance & Transfer", description: "Logs, setup assistant, and account transfer" },
    appearance: { label: "Appearance", description: "Language, theme, typography, and main window opacity" },
    overlays: { label: "Desktop Overlays", description: "Terror Zone and run statistics overlay windows" },
    automation: { label: "Recognition & Stats", description: "Audio recognition, run statistics, and diagnostics" },
    "mod-processing": { label: "Mod Processing", description: "Preserve installed features and add D2RHub modules" },
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
    "optional-features": { label: "可选功能", note: "按需启用" },
  },
  "en-US": {
    "multi-instance": { label: "Multi-instance", note: "Always on" },
    application: { label: "Application", note: "Required" },
    "optional-features": { label: "Optional features", note: "On demand" },
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
    id: "advanced",
    icon: ShieldAlert,
    kind: "platform",
    group: "application",
  },
  {
    id: "shortcuts",
    icon: Settings,
    kind: "optional",
    group: "optional-features",
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
    id: "overlays",
    icon: Monitor,
    kind: "optional",
    group: "optional-features",
    capabilityIds: ["terror-zone-overlay", "statistics-overlay"],
    isConfigured: (config) => config.enable_tz_overlay || config.enable_stats_overlay,
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
    id: "mod-processing",
    icon: PackageOpen,
    kind: "optional",
    group: "optional-features",
  },
  {
    id: "room-automation",
    icon: Route,
    kind: "optional",
    group: "optional-features",
    capabilityIds: ["room-automation"],
  },
  {
    id: "pet",
    icon: Cat,
    kind: "optional",
    group: "optional-features",
    capabilityIds: ["desktop-pet"],
  },
] as const;

export function isSettingsTabId(value: string | null | undefined): value is SettingsTabId {
  return SETTINGS_FEATURES.some((feature) => feature.id === value);
}

export function getSettingsFeaturesByKind(kind: SettingsCapabilityKind): readonly SettingsFeatureDefinition[] {
  return SETTINGS_FEATURES.filter((feature) => feature.kind === kind);
}
