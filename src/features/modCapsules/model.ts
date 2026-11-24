import type {
  ModCapsule,
  ModCapsuleAccountSelection,
  ModCapsulePool,
} from "../../store/types";

export const AUDIO_TELEMETRY_CAPSULE_FEATURE = "audio_telemetry";
export const ROOM_TOOLS_CAPSULE_FEATURE = "in_game_room_tools";
export const ESC_NEXT_GAME_CAPSULE_FEATURE = "esc_next_game";
export const AUTO_EXIT_ON_DEATH_CAPSULE_FEATURE = "auto_exit_on_death";

export const PARTY_LOOT_CAPSULE_FEATURE = "party_loot";
const JCY_INVENTORY_CAPSULE_FEATURE = "jcy_inventory";
export const SCENE_SHORTCUTS_CAPSULE_FEATURE = "scene_shortcuts";

export const HOSTILE_RESTART_CAPSULE_FEATURE = "hostile_auto_restart";

const FEATURE_LABELS: Record<string, readonly [string, string]> = {
  [AUDIO_TELEMETRY_CAPSULE_FEATURE]: ["声纹识别", "Audio recognition"],
  [ROOM_TOOLS_CAPSULE_FEATURE]: ["局内房间工具", "In-game room tools"],
  [HOSTILE_RESTART_CAPSULE_FEATURE]: ["敌对自动重开", "Hostile auto-restart"],
  [PARTY_LOOT_CAPSULE_FEATURE]: ["左侧玩家尸体拾取按钮", "Player loot buttons"],
  [JCY_INVENTORY_CAPSULE_FEATURE]: ["背包与仓库", "Inventory and stash"],
  [SCENE_SHORTCUTS_CAPSULE_FEATURE]: ["场景重开", "Scene restart"],
  [ESC_NEXT_GAME_CAPSULE_FEATURE]: ["双击 Esc 下一局地狱", "Double Esc: next Hell game"],
  [AUTO_EXIT_ON_DEATH_CAPSULE_FEATURE]: ["死亡自动退房", "Auto-exit on death"],
};

export function capsuleFeatureLabels(
  capsule: ModCapsule,
  isEnglish = false,
  minimalMode = false,
): string[] {
  const visibleFeatures = minimalMode
    ? capsule.feature_groups.filter((feature) => feature === AUTO_EXIT_ON_DEATH_CAPSULE_FEATURE || feature === ESC_NEXT_GAME_CAPSULE_FEATURE || feature === SCENE_SHORTCUTS_CAPSULE_FEATURE || feature === PARTY_LOOT_CAPSULE_FEATURE || feature === HOSTILE_RESTART_CAPSULE_FEATURE || feature === JCY_INVENTORY_CAPSULE_FEATURE)
    : capsule.feature_groups;
  return visibleFeatures.map((feature) => {
    if (feature === AUTO_EXIT_ON_DEATH_CAPSULE_FEATURE
      && capsule.auto_exit_on_death_enabled === false) {
      return isEnglish ? "Auto-exit on death (off)" : "死亡自动退房（已停用）";
    }
    return FEATURE_LABELS[feature]?.[isEnglish ? 1 : 0] ?? feature;
  });
}

export function capsuleBaseModLabel(capsule: ModCapsule, isEnglish = false): string {
  const sourceName = capsule.source_mod_name?.trim();
  if (!sourceName) return isEnglish ? "Original game" : "原版";
  if (sourceName.toLocaleLowerCase() === capsule.name.trim().toLocaleLowerCase()) {
    return isEnglish ? "Source unverified" : "来源待确认";
  }
  return sourceName;
}

export function capsuleSelectionForAccount(
  pool: ModCapsulePool | null,
  accountId: string,
): ModCapsuleAccountSelection | null {
  return pool?.accounts.find((entry) => entry.account_id === accountId) ?? null;
}

export function selectedCapsuleForAccount(
  pool: ModCapsulePool | null,
  accountId: string,
): ModCapsule | null {
  const selection = capsuleSelectionForAccount(pool, accountId);
  if (!selection?.selected_capsule_id) return null;
  return pool?.capsules.find((capsule) => capsule.id === selection.selected_capsule_id) ?? null;
}

export function compatibleCapsulesForAccount(
  pool: ModCapsulePool | null,
  accountId: string,
): ModCapsule[] {
  const edition = capsuleSelectionForAccount(pool, accountId)?.edition;
  if (!edition) return [];
  return (pool?.capsules ?? []).filter((capsule) => capsule.edition === edition && capsule.ready);
}

export function accountUsesCapsuleFeature(
  pool: ModCapsulePool | null,
  accountId: string,
  featureId: string,
): boolean {
  const capsule = selectedCapsuleForAccount(pool, accountId);
  return !!capsule?.ready && capsule.feature_groups.includes(featureId);
}

export function accountsMissingCapsuleFeature(
  pool: ModCapsulePool | null,
  accountIds: readonly string[],
  featureId: string,
): string[] {
  return accountIds.filter((accountId) => !accountUsesCapsuleFeature(pool, accountId, featureId));
}
