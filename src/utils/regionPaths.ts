import type { GlobalConfig } from "../store/types";

export type AccountRegion = "CN" | "KR" | "NA" | "EU";
export type InternationalAccountRegion = Exclude<AccountRegion, "CN">;
export type AccountAuthMode = "bnet" | "token";

export const ACCOUNT_REGIONS: readonly AccountRegion[] = ["CN", "KR", "NA", "EU"];
export const INTERNATIONAL_ACCOUNT_REGIONS: readonly InternationalAccountRegion[] = ["KR", "NA", "EU"];

export const ACCOUNT_REGION_LABELS: Record<AccountRegion, string> = {
  CN: "国服",
  KR: "亚服",
  NA: "美服",
  EU: "欧服",
};

export function normalizeAccountRegion(
  region: string | null | undefined,
): AccountRegion | null {
  switch (region?.trim().toUpperCase()) {
    case "CN": return "CN";
    case "KR":
    case "GLOBAL":
    case "ASIA": return "KR";
    case "NA":
    case "US":
    case "AMERICAS": return "NA";
    case "EU":
    case "EUROPE": return "EU";
    default: return null;
  }
}

export function normalizeInternationalAccountRegion(
  region: string | null | undefined,
): InternationalAccountRegion | null {
  const normalized = normalizeAccountRegion(region);
  return normalized && normalized !== "CN" ? normalized : null;
}

export function accountRegionLabel(region: string | null | undefined): string {
  const normalized = normalizeAccountRegion(region);
  return normalized ? ACCOUNT_REGION_LABELS[normalized] : "国服";
}

export function isCnRegion(region: string | null | undefined): boolean {
  return region?.trim().toUpperCase() === "CN";
}

export function isInternationalRegion(region: string | null | undefined): boolean {
  return normalizeInternationalAccountRegion(region) !== null;
}

export function requiresTokenMigration(
  authMode: string | null | undefined,
  region: string | null | undefined,
  config?: GlobalConfig | null,
): boolean {
  if (authMode === "token") return false;
  if (isInternationalRegion(region)) return true;
  if (region?.trim() || !config) return false;

  const cnComplete = Boolean(config.cn_game_path?.trim());
  const globalComplete = Boolean(config.global_game_path?.trim());
  return globalComplete && !cnComplete;
}

export function hasConfiguredPathsForRegion(
  config: GlobalConfig | null | undefined,
  region: string | null | undefined,
  authMode: AccountAuthMode,
): boolean {
  if (!config) return false;
  const normalizedRegion = region?.trim().toUpperCase();
  const knownRegion = [
    "CN", "KR", "GLOBAL", "ASIA", "NA", "US", "AMERICAS", "EU", "EUROPE",
  ].includes(normalizedRegion || "");
  if (!knownRegion) return false;

  const gamePath = isCnRegion(region) ? config.cn_game_path : config.global_game_path;
  const hasGame = Boolean(gamePath?.trim());
  return authMode === "token"
    ? hasGame
    : Boolean(isCnRegion(region) && config.cn_battle_net_path?.trim() && hasGame);
}

export function firstConfiguredRegion(
  config: GlobalConfig | null | undefined,
  authMode: AccountAuthMode,
): AccountRegion | null {
  if (hasConfiguredPathsForRegion(config, "CN", authMode)) return "CN";
  if (hasConfiguredPathsForRegion(config, "KR", authMode)) return "KR";
  return null;
}
