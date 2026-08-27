import type { GlobalConfig } from "../store/types";

const INSTALLATION_PATH_KEYS = [
  "cn_battle_net_path",
  "cn_game_path",
  "cn_saved_games_path",
  "global_game_path",
  "global_saved_games_path",
] as const satisfies readonly (keyof GlobalConfig)[];

export function installationPathsChanged(
  previous: GlobalConfig,
  next: GlobalConfig,
): boolean {
  return INSTALLATION_PATH_KEYS.some((key) => previous[key] !== next[key]);
}

export function hasValidEditionPathPairs(config: GlobalConfig): boolean {
  return Boolean(config.cn_game_path.trim() || config.global_game_path.trim());
}

export function installationPathEditsAreInvalid(
  previous: GlobalConfig | null,
  next: GlobalConfig,
): boolean {
  return previous !== null
    && installationPathsChanged(previous, next)
    && !hasValidEditionPathPairs(next);
}
