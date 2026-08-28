import type { GlobalConfig } from "../store/types";

export type LegacyPathResolution = "CN" | "Global" | "keep";

/** Apply an explicit user decision without touching unrelated configuration fields. */
export function resolveLegacyPathMigration(
  current: GlobalConfig,
  resolution: LegacyPathResolution,
): GlobalConfig {
  const candidate = current.legacy_path_migration;
  if (!candidate) return current;

  const resolved: GlobalConfig = { ...current, legacy_path_migration: null };
  if (resolution === "CN") {
    if (candidate.game_path) resolved.cn_game_path = candidate.game_path;
    if (candidate.saved_games_path) resolved.cn_saved_games_path = candidate.saved_games_path;
    if (candidate.battle_net_path) resolved.cn_battle_net_path = candidate.battle_net_path;
  } else if (resolution === "Global") {
    if (candidate.game_path) resolved.global_game_path = candidate.game_path;
    if (candidate.saved_games_path) resolved.global_saved_games_path = candidate.saved_games_path;
  }
  return resolved;
}
