import type { GlobalConfig } from "../store/types";

export type GlobalConfigPatch = Partial<GlobalConfig>;

const SERVER_MANAGED_CONFIG_KEYS = new Set<keyof GlobalConfig>([
  "version",
  "accounts_dir",
  "legacy_path_migration",
]);

function valuesEqual(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true;
  return JSON.stringify(left) === JSON.stringify(right);
}

export function diffGlobalConfig(
  original: GlobalConfig | null | undefined,
  current: GlobalConfig,
): GlobalConfigPatch {
  if (!original) return {};
  const patch: GlobalConfigPatch = {};
  for (const key of Object.keys(current) as Array<keyof GlobalConfig>) {
    if (SERVER_MANAGED_CONFIG_KEYS.has(key)) continue;
    if (!valuesEqual(original[key], current[key])) {
      (patch as Record<string, unknown>)[key] = current[key];
    }
  }
  return patch;
}

export function hasGlobalConfigPatch(patch: GlobalConfigPatch): boolean {
  return Object.keys(patch).length > 0;
}
