import type {
  ModCapsule,
  ModCapsuleAccountSelection,
  ModCapsulePool,
} from "../../store/types";

export const AUDIO_TELEMETRY_CAPSULE_FEATURE = "audio_telemetry";
export const ROOM_TOOLS_CAPSULE_FEATURE = "in_game_room_tools";
export const AUTO_EXIT_ON_DEATH_CAPSULE_FEATURE = "auto_exit_on_death";

const FEATURE_LABELS: Record<string, string> = {
  [AUDIO_TELEMETRY_CAPSULE_FEATURE]: "声纹识别",
  [ROOM_TOOLS_CAPSULE_FEATURE]: "局内房间工具",
  [AUTO_EXIT_ON_DEATH_CAPSULE_FEATURE]: "死亡自动退房",
};

export function capsuleFeatureLabels(capsule: ModCapsule): string[] {
  return capsule.feature_groups.map((feature) => {
    if (feature === AUTO_EXIT_ON_DEATH_CAPSULE_FEATURE
      && capsule.auto_exit_on_death_enabled === false) {
      return "死亡自动退房（已停用）";
    }
    return FEATURE_LABELS[feature] ?? feature;
  });
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
