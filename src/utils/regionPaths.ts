import type { GlobalConfig } from "../store/types";

export type AccountRegion = "CN" | "KR" | "NA" | "EU";
export type AccountAuthMode = "bnet" | "token";

export const ACCOUNT_REGIONS: readonly AccountRegion[] = ["CN", "KR", "NA", "EU"];

export function isCnRegion(region: string | null | undefined): boolean {
  return region?.trim().toUpperCase() === "CN";
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


  const battleNetPath = isCnRegion(region)
    ? config.cn_battle_net_path
    : config.global_battle_net_path;
  const gamePath = isCnRegion(region) ? config.cn_game_path : config.global_game_path;
  const savedGamesPath = isCnRegion(region)
    ? config.cn_saved_games_path
    : config.global_saved_games_path;
  const hasGameAndSaves = Boolean(gamePath?.trim() && savedGamesPath?.trim());
  return authMode === "token"
    ? hasGameAndSaves
    : Boolean(battleNetPath?.trim() && hasGameAndSaves);
}

export function firstConfiguredRegion(
  config: GlobalConfig | null | undefined,
  authMode: AccountAuthMode,
): AccountRegion | null {
  if (hasConfiguredPathsForRegion(config, "CN", authMode)) return "CN";
  if (hasConfiguredPathsForRegion(config, "KR", authMode)) return "KR";
  return null;
}
