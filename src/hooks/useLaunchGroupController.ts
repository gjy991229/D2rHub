import { useEffect, useRef, useState } from "react";

import { showToast } from "../components/ui/Toast";
import { useAccounts } from "../store/accounts";
import { useGlobalConfig } from "../store/globalConfig";
import { useLaunch } from "../store/launch";
import { invokeCommand } from "../platform/tauri";
import type { RoomAutomationConfigSnapshot } from "../features/roomAutomation/types";
import { readFramerateCap } from "../utils/gameSettings";
import type { SchemeEnvironment, AccountMeta, LaunchGroup, LaunchGroupMember, ModCapsulePool } from "../store/types";
import {
  inspectLaunchGroup,
  launchGroupNameExists,
  MAX_FAVORITE_LAUNCH_GROUPS,
  materializeLaunchGroupMembers,
  nextLaunchGroupName,
  normalizeFavoriteLaunchGroupIds,
  toggleFavoriteLaunchGroupId,
} from "../utils/launchGroups";
import { requiresTokenMigration, normalizeAccountRegion, firstConfiguredRegion } from "../utils/regionPaths";

export interface LaunchGroupDraft {
  key: string;
  revision: number | null;
  environment: SchemeEnvironment;
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

export function useLaunchGroupController(modCapsulePool?: ModCapsulePool | null) {
  const { config, saving, patch } = useGlobalConfig();
  const { accounts } = useAccounts();
  const { startSavedScheme } = useLaunch();
  const [busy, setBusy] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const opening = useRef(0);
  const [draft, setDraft] = useState<LaunchGroupDraft | null>(null);
  const [pendingDelete, setPendingDelete] = useState<LaunchGroup | null>(null);

  const openDraft = async (group?: LaunchGroup, duplicate = false) => {
    const request = ++opening.current;
    setBusy(true); setLoadError(null);
    try {
      const environment = group?.environment ? structuredClone(group.environment)
        : await invokeCommand<SchemeEnvironment>("get_launch_scheme_defaults");
      if (group && !group.id && config) {
        if (config.installed_optional_modules?.includes("room-automation")) {
          const { config: room } = await invokeCommand<RoomAutomationConfigSnapshot>("room_automation_get_config");
          environment.room = { primary_account_id: room.primary_account_id, follower_account_ids: [...room.follower_account_ids] };
        }
        for (const key of Object.keys(environment.settings) as (keyof SchemeEnvironment["settings"])[]) {
          if (config[key] !== undefined) Object.assign(environment.settings, { [key]: structuredClone(config[key]) });
        }
        if (config.rune_audio_external_target) environment.settings.rune_audio_target_account = "";
        environment.layout_id = config.window_layout_enabled ? config.active_window_layout_id ?? null : null;
      }
      if (request !== opening.current) return;
      setDraft({ key: createLaunchGroupId(), id: duplicate ? null : group?.id || null,
        revision: duplicate ? null : group?.id ? group.revision ?? 0 : null,
        name: duplicate ? `${group!.name} 副本` : group?.name ?? nextLaunchGroupName(config?.launch_groups ?? []),
        members: group ? materializeLaunchGroupMembers(group, accounts) : [], environment });
    } catch (error) { showToast("error", `读取方案失败: ${error}`); }
    finally { if (request === opening.current) setBusy(false); }
  };
  const create = () => { void openDraft(); };
  const edit = (group: LaunchGroup) => { void openDraft(group); };
  const duplicate = () => {
    if (!draft) return;
    setDraft({ ...structuredClone(draft), key: createLaunchGroupId(), id: null, revision: null, name: `${draft.name} 副本` });
  };
  const missing = draft?.members.filter(member => !member.snapshot).map(member => member.account_id).join("|") ?? "";
  useEffect(() => {
    if (!draft || !missing) return;
    let cancelled = false;
    setLoadError(null);
    void Promise.allSettled(draft.members.filter(member => !member.snapshot).map(async member => {
      const account = accounts.find(account => account.id === member.account_id);
      if (!account) throw new Error(`账号 ${member.account_id} 已删除，请移除`);
      const settings = await invokeCommand<Record<string, unknown>>("get_account_settings", { accountId: member.account_id, defaultsOnly: true });
      if (!Object.keys(settings).length) throw new Error(`${account.display_name} 游戏设置为空`);
      const position = account.position_presets?.find(position => position.id === member.position_preset_id);
      return { ...member, graphics_configured: true,
        resolution: member.graphics_configured ? member.resolution : String(settings["Screen Resolution (Windowed)"] ?? "1280x720"),
        fps: member.graphics_configured ? member.fps : readFramerateCap(settings, 30),
        snapshot: { game_settings: {}, position: position ? [position.x, position.y] as [number, number] : null,
          region: normalizeAccountRegion(account.region) ?? firstConfiguredRegion(config, account.auth_mode === "token" ? "token" : "bnet"), language: account.language ?? null, voicelanguage: account.voicelanguage ?? null } };
    })).then(results => {
      if (cancelled) return;
      const loaded = results.filter(result => result.status === "fulfilled").map(result => result.value);
      const errors = results.filter(result => result.status === "rejected").map(result => String(result.reason));
      setLoadError(errors.length ? errors.join("；") : null);
      setDraft(current => current?.key === draft.key ? { ...current, members: current.members.map(member => {
        const result = loaded.find(result => result.account_id === member.account_id);
        return result && !member.snapshot ? { ...member, snapshot: result.snapshot,
          graphics_configured: true, resolution: member.graphics_configured ? member.resolution : result.resolution,
          fps: member.graphics_configured ? member.fps : result.fps } : member;
      }) } : current);
    });
    return () => { cancelled = true; };
  }, [draft?.key, missing]);

  const updateEnvironment = (update: (environment: SchemeEnvironment) => void) => setDraft(current => {
    if (!current) return current;
    const environment = structuredClone(current.environment); update(environment);
    return { ...current, environment };
  });
  const moveMember = (id: string, offset: number) => setDraft(current => {
    if (!current) return current;
    const members = [...current.members], index = members.findIndex(member => member.account_id === id), next = index + offset;
    if (index < 0 || next < 0 || next >= members.length) return current;
    [members[index], members[next]] = [members[next], members[index]];
    return { ...current, members };
  });

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
    setDraft(current => current ? { ...current, members: [], environment: { ...current.environment,
      room: { primary_account_id: "", follower_account_ids: [] },
      settings: { rune_audio_target_account: "", keep_battle_net_account_id: null },
    } } : current);
  };

  const toggleAccount = (accountId: string) => {
    setDraft(current => {
      if (!current) return current;
      if (current.members.some(member => member.account_id === accountId)) {
        const environment = structuredClone(current.environment);
        environment.room.follower_account_ids = environment.room.follower_account_ids.filter(id => id !== accountId);
        if (environment.room.primary_account_id === accountId) environment.room.primary_account_id = "";
        if (environment.settings.rune_audio_target_account === accountId) environment.settings.rune_audio_target_account = "";
        if (environment.settings.keep_battle_net_account_id === accountId) environment.settings.keep_battle_net_account_id = null;
        return { ...current, environment, members: current.members.filter(member => member.account_id !== accountId) };
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
    if (!config || !draft || saving || busy) return;
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
      !member.snapshot || !member.graphics_configured || !member.resolution || member.fps == null)) {
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
      environment: structuredClone(draft.environment),
      revision: draft.revision ?? 0,
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
    try {
      setBusy(true);
      await invokeCommand("save_launch_scheme", { scheme: savedGroup, expectedRevision: draft.revision });
      await useGlobalConfig.getState().load();
      setDraft(null);
      showToast("success", `启动方案“${name}”已保存`);
    } catch (error) {
      showToast("error", `保存启动方案失败: ${error}`);
    } finally { setBusy(false); }
  };

  const removePending = async () => {
    if (!config || !pendingDelete || saving) return;
    const group = pendingDelete;
    try {
      await invokeCommand("delete_launch_scheme", { id: group.id, expectedRevision: group.revision ?? 0 });
      await useGlobalConfig.getState().load();
      if (draft?.id === group.id) setDraft(null);
      setPendingDelete(null);
      showToast("success", `启动方案“${group.name}”已删除`);
    } catch (error) {
      showToast("error", `删除启动方案失败: ${error}`);
    }
  };

  const toggleFavorite = async (group: LaunchGroup) => {
    if (!config || saving) return;
    const favorites = normalizeFavoriteLaunchGroupIds(
      config.launch_groups,
      config.favorite_launch_group_ids,
    );
    if (!favorites.includes(group.id) && favorites.length >= MAX_FAVORITE_LAUNCH_GROUPS) {
      showToast("warning", config.app_language === "en-US"
        ? "You can keep up to 3 favorite launch schemes"
        : "常用启动方案最多保留 3 套");
      return;
    }
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
    const availability = inspectLaunchGroup(group, accounts, config, modCapsulePool);
    if (!availability.can_launch) {
      showToast("warning", `启动方案“${group.name}”配置不完整，请先修复后再启动`);
      return;
    }
    if (!group.environment) { edit(group); showToast("info", "请补全旧方案并保存，再按方案启动"); return; }
    void startSavedScheme(group.id, group.revision ?? 0);
  };

  const requestDraftDelete = () => {
    if (!draft?.id) return;
    const group = config?.launch_groups.find(candidate => candidate.id === draft.id);
    if (group) setPendingDelete(group);
  };

  return {
    draft,
    busy, loadError, duplicate, moveMember, updateEnvironment,
    pendingDelete,
    create,
    edit,
    renameDraft,
    cancelDraft: () => { opening.current++; setBusy(false); setDraft(null); },
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
