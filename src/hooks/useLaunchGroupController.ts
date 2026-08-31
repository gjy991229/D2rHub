import { useState } from "react";

import { showToast } from "../components/ui/Toast";
import { useAccounts } from "../store/accounts";
import { useGlobalConfig } from "../store/globalConfig";
import { useLaunch } from "../store/launch";
import type { AccountMeta, LaunchGroup, LaunchGroupMember } from "../store/types";
import {
  inspectLaunchGroup,
  launchEntriesForGroup,
  launchGroupNameExists,
  materializeLaunchGroupMembers,
  nextLaunchGroupName,
  toggleFavoriteLaunchGroupId,
} from "../utils/launchGroups";
import { requiresTokenMigration } from "../utils/regionPaths";

export interface LaunchGroupDraft {
  id: string | null;
  name: string;
  members: LaunchGroupMember[];
}

function createLaunchGroupId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `launch-group-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

function createLaunchGroupMember(account: AccountMeta): LaunchGroupMember {
  return {
    account_id: account.id,
    mod_args: account.mod_args || "",
    position_preset_id: account.active_position_id ?? null,
    position_configured: true,
    graphics_configured: false,
    resolution: null,
    fps: null,
  };
}

export function useLaunchGroupController() {
  const { config, saving, patch } = useGlobalConfig();
  const { accounts } = useAccounts();
  const { startSchemeLaunch } = useLaunch();
  const [draft, setDraft] = useState<LaunchGroupDraft | null>(null);
  const [pendingDelete, setPendingDelete] = useState<LaunchGroup | null>(null);

  const create = () => {
    setDraft({
      id: null,
      name: nextLaunchGroupName(config?.launch_groups ?? []),
      members: [],
    });
  };

  const edit = (group: LaunchGroup) => {
    setDraft({
      id: group.id,
      name: group.name,
      members: materializeLaunchGroupMembers(group, accounts),
    });
  };

  const renameDraft = (name: string) => {
    setDraft(current => current ? { ...current, name } : current);
  };

  const selectAll = () => {
    const readyAccounts = accounts.filter(account =>
      account.initialized && !requiresTokenMigration(account.auth_mode, account.region, config)
    );
    setDraft(current => {
      if (!current) return current;
      const existing = new Map(current.members.map(member => [member.account_id, member]));
      return {
        ...current,
        members: readyAccounts.map(account =>
          existing.get(account.id) ?? createLaunchGroupMember(account)
        ),
      };
    });
  };

  const clearSelection = () => {
    setDraft(current => current ? { ...current, members: [] } : current);
  };

  const toggleAccount = (accountId: string) => {
    setDraft(current => {
      if (!current) return current;
      if (current.members.some(member => member.account_id === accountId)) {
        return {
          ...current,
          members: current.members.filter(member => member.account_id !== accountId),
        };
      }
      const account = accounts.find(candidate => candidate.id === accountId);
      if (!account?.initialized || requiresTokenMigration(account.auth_mode, account.region, config)) {
        return current;
      }
      return { ...current, members: [...current.members, createLaunchGroupMember(account)] };
    });
  };

  const updateMember = (accountId: string, memberPatch: Partial<LaunchGroupMember>) => {
    setDraft(current => current ? {
      ...current,
      members: current.members.map(member => member.account_id === accountId
        ? { ...member, ...memberPatch, account_id: accountId }
        : member),
    } : current);
  };

  const saveDraft = async () => {
    if (!config || !draft || saving) return;
    const name = draft.name.trim();
    if (!name) {
      showToast("warning", "请输入启动方案名称");
      return;
    }
    if (draft.members.length === 0) {
      showToast("warning", "启动方案至少需要选择一个账号");
      return;
    }
    if (draft.members.some(member =>
      !member.graphics_configured || !member.resolution || member.fps == null)) {
      showToast("warning", "请等待所有已选账号的分辨率与 FPS 加载完成");
      return;
    }
    if (launchGroupNameExists(config.launch_groups, name, draft.id)) {
      showToast("warning", `启动方案名称“${name}”已存在`);
      return;
    }

    const id = draft.id ?? createLaunchGroupId();
    const savedGroup: LaunchGroup = {
      id,
      name,
      account_ids: draft.members.map(member => member.account_id),
      members: draft.members.map(member => ({
        ...member,
        mod_args: member.mod_args ?? "",
        position_preset_id: member.position_preset_id ?? null,
        position_configured: true,
        graphics_configured: true,
        resolution: member.resolution,
        fps: member.fps,
      })),
    };
    const nextGroups = draft.id
      ? config.launch_groups.map(group => group.id === draft.id ? savedGroup : group)
      : [...config.launch_groups, savedGroup];

    try {
      await patch({ launch_groups: nextGroups });
      setDraft(null);
      showToast("success", `启动方案“${name}”已保存`);
    } catch (error) {
      showToast("error", `保存启动方案失败: ${error}`);
    }
  };

  const removePending = async () => {
    if (!config || !pendingDelete || saving) return;
    const group = pendingDelete;
    try {
      await patch({
        launch_groups: config.launch_groups.filter(candidate => candidate.id !== group.id),
        favorite_launch_group_ids: (config.favorite_launch_group_ids ?? [])
          .filter(groupId => groupId !== group.id),
      });
      if (draft?.id === group.id) setDraft(null);
      setPendingDelete(null);
      showToast("success", `启动方案“${group.name}”已删除`);
    } catch (error) {
      showToast("error", `删除启动方案失败: ${error}`);
    }
  };

  const toggleFavorite = async (group: LaunchGroup) => {
    if (!config || saving) return;
    try {
      await patch({
        favorite_launch_group_ids: toggleFavoriteLaunchGroupId(
          config.launch_groups,
          config.favorite_launch_group_ids,
          group.id,
        ),
      });
    } catch (error) {
      showToast("error", `更新常用启动方案失败: ${error}`);
    }
  };

  const launch = (group: LaunchGroup) => {
    const availability = inspectLaunchGroup(group, accounts, config);
    if (!availability.can_launch) {
      showToast("warning", `启动方案“${group.name}”配置不完整，请先修复后再启动`);
      return;
    }
    void startSchemeLaunch(launchEntriesForGroup(group, accounts));
  };

  const requestDraftDelete = () => {
    if (!draft?.id) return;
    const group = config?.launch_groups.find(candidate => candidate.id === draft.id);
    if (group) setPendingDelete(group);
  };

  return {
    draft,
    pendingDelete,
    create,
    edit,
    renameDraft,
    cancelDraft: () => setDraft(null),
    selectAll,
    clearSelection,
    toggleAccount,
    updateMember,
    saveDraft,
    requestDelete: setPendingDelete,
    requestDraftDelete,
    closeDelete: () => setPendingDelete(null),
    removePending,
    toggleFavorite,
    launch,
  };
}

export type LaunchGroupController = ReturnType<typeof useLaunchGroupController>;
