import type { AccountMeta, GlobalConfig, LaunchGroup } from "../store/types";
import { sortAccountsByCardOrder } from "./accountOrder";
import { requiresTokenMigration } from "./regionPaths";

export type LaunchGroupIssueReason = "missing" | "not_initialized" | "token_migration";

export interface LaunchGroupMemberIssue {
  account_id: string;
  account_name: string;
  reason: LaunchGroupIssueReason;
}

export interface LaunchGroupAvailability {
  can_launch: boolean;
  ordered_account_ids: string[];
  issues: LaunchGroupMemberIssue[];
}

export function inspectLaunchGroup(
  group: LaunchGroup,
  accounts: readonly AccountMeta[],
  config?: GlobalConfig | null,
): LaunchGroupAvailability {
  const accountsById = new Map(accounts.map(account => [account.id, account]));
  const issues: LaunchGroupMemberIssue[] = [];

  for (const accountId of group.account_ids) {
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
    }
  }

  const members = new Set(group.account_ids);
  const orderedAccountIds = sortAccountsByCardOrder(accounts)
    .filter(account => members.has(account.id))
    .map(account => account.id);

  return {
    can_launch: group.account_ids.length > 0 && issues.length === 0,
    ordered_account_ids: orderedAccountIds,
    issues,
  };
}

export function nextLaunchGroupName(groups: readonly LaunchGroup[]): string {
  const names = new Set(groups.map(group => group.name.trim().toLocaleLowerCase()));
  let index = 1;
  while (names.has(`启动组 ${index}`.toLocaleLowerCase())) index += 1;
  return `启动组 ${index}`;
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
