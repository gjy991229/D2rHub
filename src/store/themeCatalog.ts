export const THEME_OPTIONS = [
  { id: "onyx", label: "深色", desc: "纯黑分层，高对比", dark: true },
  { id: "light", label: "浅色", desc: "清晰明亮，低干扰", dark: false },
  { id: "midnight", label: "午夜蓝", desc: "深蓝底色，冰蓝点缀", dark: true },
  { id: "forest", label: "森林绿", desc: "墨绿层次，柔和青绿", dark: true },
  { id: "sand", label: "暖砂", desc: "暖白纸感，琥珀点缀", dark: false },
] as const;

export type ThemeKey = typeof THEME_OPTIONS[number]["id"];

export function isThemeKey(value: unknown): value is ThemeKey {
  return THEME_OPTIONS.some((option) => option.id === value);
}

export function normalizeTheme(value: unknown): ThemeKey {
  return isThemeKey(value) ? value : "light";
}

export function isDarkTheme(theme: ThemeKey): boolean {
  return THEME_OPTIONS.some((option) => option.id === theme && option.dark);
}
