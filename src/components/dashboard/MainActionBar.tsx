import { Check, ListChecks, PackageOpen, Trash2, UserPlus } from "lucide-react";

import type { LaunchGroupController } from "../../hooks/useLaunchGroupController";
import { useAccounts } from "../../store/accounts";
import { useGlobalConfig } from "../../store/globalConfig";
import { ActionBar } from "./ActionBar";
import { FavoriteLaunchGroups } from "./FavoriteLaunchGroups";
import { LaunchButton } from "./LaunchButton";
import { LaunchGroupMenu } from "./LaunchGroupMenu";
import { RoomAutomationQuickEdit } from "./RoomAutomationQuickEdit";

interface MainActionBarProps {
  launching: boolean;
  launchableAccountIds: string[];
  launchGroups: LaunchGroupController;
  onCancelLaunch: () => void;
  onStartLaunch: (accountIds: string[]) => void;
  onAddAccount: () => void;
  onRequestKillAll: () => void;
  launchGroupPanelOpen: boolean;
  onToggleLaunchGroupPanel: () => void;
  onOpenModManager: () => void;
  onOpenRoomAutomation: () => void;
  showOptionalFeatures?: boolean;
}

export function MainActionBar({
  launching,
  launchableAccountIds,
  launchGroups,
  onCancelLaunch,
  onStartLaunch,
  onAddAccount,
  onRequestKillAll,
  launchGroupPanelOpen,
  onToggleLaunchGroupPanel,
  onOpenModManager,
  onOpenRoomAutomation,
  showOptionalFeatures = true,
}: MainActionBarProps) {
  const { config, saving } = useGlobalConfig();
  const { accounts } = useAccounts();
  const draft = launchGroups.draft;
  const draftReady = Boolean(
    draft?.members.length
    && draft.name.trim()
    && draft.members.every(member =>
      member.graphics_configured && member.resolution && member.fps != null),
  );

  return (
    <ActionBar>
      {launching ? (
        <button onClick={onCancelLaunch} className="danger-cta">
          取消操作
        </button>
      ) : draft ? (
        <>
          <div className="launch-group-editor flex min-w-0 items-center gap-2">
            <span className="launch-group-editor-label">
              <ListChecks size={13} strokeWidth={1.9} aria-hidden="true" />
              {draft.id ? "编辑启动方案" : "新建启动方案"}
            </span>
            <input
              type="text"
              className="line-input launch-group-name-input px-2.5"
              value={draft.name}
              maxLength={32}
              aria-label="启动方案名称"
              placeholder="启动方案名称"
              autoFocus
              disabled={saving}
              onChange={event => launchGroups.renameDraft(event.target.value)}
              onKeyDown={event => {
                if (event.key === "Enter") void launchGroups.saveDraft();
                if (event.key === "Escape") launchGroups.cancelDraft();
              }}
            />
            <button
              disabled={saving || !draftReady}
              onClick={() => void launchGroups.saveDraft()}
              className="primary-cta"
            >
              <Check size={13} strokeWidth={2} />
              保存方案 ({draft.members.length})
            </button>
            <button onClick={launchGroups.selectAll} disabled={saving} className="control-btn">
              全选
            </button>
            <button
              onClick={launchGroups.clearSelection}
              disabled={saving || draft.members.length === 0}
              className="control-btn"
            >
              清空已选
            </button>
            <button onClick={launchGroups.cancelDraft} disabled={saving} className="control-btn">
              取消
            </button>
          </div>
          <div className="flex-1" />
          {draft.id && (
            <button
              type="button"
              className="control-btn danger-control"
              disabled={saving}
              onClick={launchGroups.requestDraftDelete}
            >
              <Trash2 size={12} strokeWidth={1.8} aria-hidden="true" />
              删除方案
            </button>
          )}
        </>
      ) : (
        <>
        <div className="flex min-w-0 items-center gap-2">
          <LaunchButton
            count={launchableAccountIds.length}
            loading={launching}
            onClick={() => onStartLaunch(launchableAccountIds)}
          />
          <FavoriteLaunchGroups
            groups={config?.launch_groups ?? []}
            favoriteGroupIds={config?.favorite_launch_group_ids}
            accounts={accounts}
            config={config}
            disabled={launching || saving}
            onLaunch={launchGroups.launch}
            onToggleFavorite={group => void launchGroups.toggleFavorite(group)}
          />
          <button
            onClick={onRequestKillAll}
            title="一键关闭所有暗黑2进程"
            className="control-btn danger-control ml-1 min-w-[72px]"
          >
            一键关闭
          </button>
          {showOptionalFeatures && (
            <RoomAutomationQuickEdit
              active={config?.installed_optional_modules?.includes("room-automation") === true}
              language={config?.app_language}
              onOpenSettings={onOpenRoomAutomation}
            />
          )}
        </div>
        <div className="flex-1" />
        <button type="button" className="control-btn" onClick={onOpenModManager}>
          <PackageOpen size={13} strokeWidth={1.9} aria-hidden="true" />
          Mod 管理
        </button>
        <LaunchGroupMenu
          count={config?.launch_groups.length ?? 0}
          open={launchGroupPanelOpen}
          disabled={launching || saving}
          onToggle={onToggleLaunchGroupPanel}
        />
        </>
      )}
      {!draft && (
        <>
          <button onClick={onAddAccount} className="control-btn add-account-cta">
            <UserPlus size={13} strokeWidth={1.9} />
            添加账号
          </button>
        </>
      )}
    </ActionBar>
  );
}
