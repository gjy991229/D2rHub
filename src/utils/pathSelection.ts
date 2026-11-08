import { invokeCommand } from "../platform/tauri";

export const installationPathFields = [
  "cn_battle_net_path", "cn_game_path", "global_game_path",
  "cn_saved_games_path", "global_saved_games_path",
] as const;

export async function validatePathSelection(field: string, path: string): Promise<void> {
  if (!(installationPathFields as readonly string[]).includes(field)) return;
  await invokeCommand("validate_config_path_selection", { field, path });
}
