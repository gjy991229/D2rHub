import type {
  AccountMeta,
  GlobalConfig,
  LaunchAccountEntry,
  LaunchGroup,
  LaunchGroupMember,
} from "../store/types";
import { sortAccountsByCardOrder } from "./accountOrder";
import { requiresTokenMigration } from "./regionPaths";

export type LaunchGroupIssueReason =
  | "missing"
  | "not_initialized"
  | "token_migration"
  | "missing_mod"
  | "missing_position";

export interface LaunchGroupMemberIssue {
  account_id: string;
  account_name: string;
  reason: LaunchGroupIssueReason;
  detail?: string;
}

export interface LaunchGroupAvailability {
  can_launch: boolean;
  ordered_account_ids: string[];
  issues: LaunchGroupMemberIssue[];
}

export const MAX_FAVORITE_LAUNCH_GROUPS = 3;

export function launchGroupIssueReason(issue: LaunchGroupMemberIssue): string {
  if (issue.reason === "missing") return "账号已删除";
  if (issue.reason === "not_initialized") return "尚未初始化";
  if (issue.reason === "token_migration") return "需要迁移 Token";
  if (issue.reason === "missing_mod") return "Mod 已删除";
  return "位置已删除";
}

export function launchGroupIssueDetails(issues: readonly LaunchGroupMemberIssue[]): string {
  return issues.map(issue => `${issue.account_name}：${launchGroupIssueReason(issue)}`).join("；");
}

export function normalizeFavoriteLaunchGroupIds(
  groups: readonly LaunchGroup[],
  favoriteIds: readonly string[] | null | undefined,
): string[] {
  const existingIds = new Set(groups.map(group => group.id));
  const seen = new Set<string>();
  return (favoriteIds ?? [])
    .map(id => id.trim())
    .filter(id => {
      if (id.length === 0 || !existingIds.has(id) || seen.has(id)) return false;
      seen.add(id);
      return true;
    })
    .slice(0, MAX_FAVORITE_LAUNCH_GROUPS);
}

export function favoriteLaunchGroups(
  groups: readonly LaunchGroup[],
  favoriteIds: readonly string[] | null | undefined,
): LaunchGroup[] {
  const groupsById = new Map(groups.map(group => [group.id, group]));
  return normalizeFavoriteLaunchGroupIds(groups, favoriteIds)
    .map(id => groupsById.get(id))
    .filter((group): group is LaunchGroup => !!group);
}

export function toggleFavoriteLaunchGroupId(
  groups: readonly LaunchGroup[],
  favoriteIds: readonly string[] | null | undefined,
  groupId: string,
): string[] {
  const normalized = normalizeFavoriteLaunchGroupIds(groups, favoriteIds);
  return normalized.includes(groupId)
    ? normalized.filter(id => id !== groupId)
    : normalized.length >= MAX_FAVORITE_LAUNCH_GROUPS
      ? normalized
      : normalizeFavoriteLaunchGroupIds(groups, [...normalized, groupId]);
}

export function launchGroupAccountIds(group: LaunchGroup): string[] {
  const source = group.members?.length
    ? group.members.map(member => member.account_id)
    : group.account_ids;
  return [...new Set(source.map(id => id.trim()).filter(Boolean))];
}

function explicitMember(group: LaunchGroup, accountId: string): LaunchGroupMember | undefined {
  return group.members?.find(member => member.account_id === accountId);
}

export function materializeLaunchGroupMembers(
  group: LaunchGroup,
  accounts: readonly AccountMeta[],
): LaunchGroupMember[] {
  const accountsById = new Map(accounts.map(account => [account.id, account]));
  return launchGroupAccountIds(group).map(accountId => {
    const account = accountsById.get(accountId);
    const configured = explicitMember(group, accountId);
    return {
      account_id: accountId,
      mod_args: configured?.mod_args ?? account?.mod_args ?? "",
      position_preset_id: configured?.position_configured
        ? configured.position_preset_id ?? null
        : account?.active_position_id ?? null,
      position_configured: true,
      graphics_configured: configured?.graphics_configured ?? false,
      resolution: configured?.graphics_configured ? configured.resolution ?? null : null,
      fps: configured?.graphics_configured ? configured.fps ?? null : null,
    };
  });
}

export function inspectLaunchGroup(
  group: LaunchGroup,
  accounts: readonly AccountMeta[],
  config?: GlobalConfig | null,
): LaunchGroupAvailability {
  const accountsById = new Map(accounts.map(account => [account.id, account]));
  const issues: LaunchGroupMemberIssue[] = [];
  const accountIds = launchGroupAccountIds(group);

  for (const accountId of accountIds) {
    const account = accountsById.get(accountId);
    if (!account) {
      issues.push({ account_id: accountId, account_name: accountId, reason: "missing" });
      continue;
    }
    if (!account.initialized) {
      issues.push({
        account_id: accountId,
        account_name: account.display_name || account.id,
        reason: "not_initialized",
      });
      continue;
    }
    if (requiresTokenMigration(account.auth_mode, account.region, config)) {
      issues.push({
        account_id: accountId,
        account_name: account.display_name || account.id,
        reason: "token_migration",
      });
      continue;
    }

    const member = explicitMember(group, accountId);
    if (member?.mod_args != null && member.mod_args.trim()) {
      const exists = (account.mod_list || []).some(mod => mod.trim() === member.mod_args?.trim());
      if (!exists) {
        issues.push({
          account_id: accountId,
          account_name: account.display_name || account.id,
          reason: "missing_mod",
          detail: member.mod_args,
        });
      }
    }
    if (member?.position_configured && member.position_preset_id) {
      const exists = (account.position_presets || [])
        .some(position => position.id === member.position_preset_id);
      if (!exists) {
        issues.push({
          account_id: accountId,
          account_name: account.display_name || account.id,
          reason: "missing_position",
          detail: member.position_preset_id,
        });
      }
    }
  }

  const members = new Set(accountIds);
  const orderedAccountIds = sortAccountsByCardOrder(accounts)
    .filter(account => members.has(account.id))
    .map(account => account.id);

  return {
    can_launch: accountIds.length > 0 && issues.length === 0,
    ordered_account_ids: orderedAccountIds,
    issues,
  };
}

export function launchEntriesForGroup(
  group: LaunchGroup,
  accounts: readonly AccountMeta[],
): LaunchAccountEntry[] {
  const membersById = new Map(
    materializeLaunchGroupMembers(group, accounts).map(member => [member.account_id, member]),
  );
  return sortAccountsByCardOrder(accounts)
    .filter(account => membersById.has(account.id))
    .map(account => {
      const member = membersById.get(account.id)!;
      return {
        account_id: account.id,
        overrides: {
          mod_args: member.mod_args ?? "",
          position_preset_id: member.position_preset_id ?? null,
          ...(member.graphics_configured ? {
            resolution: member.resolution ?? null,
            fps: member.fps ?? null,
          } : {}),
        },
      };
    });
}

export function nextLaunchGroupName(groups: readonly LaunchGroup[]): string {
  const names = new Set(groups.map(group => group.name.trim().toLocaleLowerCase()));
  let index = 1;
  while (names.has(`启动方案 ${index}`.toLocaleLowerCase())) index += 1;
  return `启动方案 ${index}`;
}

export function launchGroupNameExists(
  groups: readonly LaunchGroup[],
  name: string,
  excludingId?: string | null,
): boolean {
  const normalizedName = name.trim().toLocaleLowerCase();
  return groups.some(group =>
    group.id !== excludingId && group.name.trim().toLocaleLowerCase() === normalizedName
  );
}
