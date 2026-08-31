import type { AccountMeta } from "../store/types";

export interface AccountWorkspacePartition {
  active: AccountMeta[];
  standby: AccountMeta[];
  standbyIds: string[];
}

export type AccountWorkspaceContainer = "active" | "standby";

export function workspaceContainerForId(
  id: string,
  activeIds: readonly string[],
  standbyIds: readonly string[],
  launchpadDropId: string,
  standbyDropId: string,
): AccountWorkspaceContainer | null {
  if (id === launchpadDropId || activeIds.includes(id)) return "active";
  if (id === standbyDropId || standbyIds.includes(id)) return "standby";
  return null;
}

export function prioritizeWorkspaceCollisionIds(
  draggedId: string,
  collisionIds: readonly string[],
  activeIds: readonly string[],
  standbyIds: readonly string[],
  launchpadDropId: string,
  standbyDropId: string,
): string[] {
  const source = workspaceContainerForId(
    draggedId,
    activeIds,
    standbyIds,
    launchpadDropId,
    standbyDropId,
  );
  const uniqueCandidates = collisionIds.filter((id, index) => (
    id !== draggedId && collisionIds.indexOf(id) === index
  ));
  if (!source) return uniqueCandidates;

  const target: AccountWorkspaceContainer = source === "active" ? "standby" : "active";
  const isContainerDropZone = (id: string) => id === launchpadDropId || id === standbyDropId;
  const inContainer = (id: string, container: AccountWorkspaceContainer) => (
    workspaceContainerForId(id, activeIds, standbyIds, launchpadDropId, standbyDropId) === container
  );

  return [
    ...uniqueCandidates.filter(id => inContainer(id, target) && !isContainerDropZone(id)),
    ...uniqueCandidates.filter(id => inContainer(id, target) && isContainerDropZone(id)),
    ...uniqueCandidates.filter(id => inContainer(id, source) && !isContainerDropZone(id)),
    ...uniqueCandidates.filter(id => inContainer(id, source) && isContainerDropZone(id)),
    ...uniqueCandidates.filter(id => !workspaceContainerForId(
      id,
      activeIds,
      standbyIds,
      launchpadDropId,
      standbyDropId,
    )),
  ];
}

export function partitionAccountWorkspace(
  orderedAccounts: readonly AccountMeta[],
  configuredStandbyIds: readonly string[],
): AccountWorkspacePartition {
  const accountsById = new Map(orderedAccounts.map(account => [account.id, account]));
  const seen = new Set<string>();
  const standbyIds = configuredStandbyIds.filter(accountId => {
    if (!accountsById.has(accountId) || seen.has(accountId)) return false;
    seen.add(accountId);
    return true;
  });
  const standbySet = new Set(standbyIds);
  return {
    active: orderedAccounts.filter(account => !standbySet.has(account.id)),
    standby: standbyIds.map(accountId => accountsById.get(accountId)!),
    standbyIds,
  };
}

export function insertAccountId(
  orderedIds: readonly string[],
  accountId: string,
  beforeId?: string | null,
): string[] {
  const next = orderedIds.filter(candidate => candidate !== accountId);
  const targetIndex = beforeId ? next.indexOf(beforeId) : -1;
  next.splice(targetIndex >= 0 ? targetIndex : next.length, 0, accountId);
  return next;
}

export function moveIdWithinList(
  orderedIds: readonly string[],
  accountId: string,
  overId: string,
): string[] {
  const oldIndex = orderedIds.indexOf(accountId);
  const newIndex = orderedIds.indexOf(overId);
  if (oldIndex < 0 || newIndex < 0 || oldIndex === newIndex) return [...orderedIds];
  const next = [...orderedIds];
  const [moved] = next.splice(oldIndex, 1);
  next.splice(newIndex, 0, moved);
  return next;
}

export function completeWorkspaceOrder(
  activeIds: readonly string[],
  standbyIds: readonly string[],
): string[] {
  return [...activeIds, ...standbyIds];
}
