import { RecognitionQuickEdit } from "./RecognitionQuickEdit";
import { PackageOpen, Square, UserPlus } from "lucide-react";

import type { LaunchGroupController } from "../../hooks/useLaunchGroupController";
import { useAccounts } from "../../store/accounts";
import { useGlobalConfig } from "../../store/globalConfig";
import { ActionBar } from "./ActionBar";
import { FavoriteLaunchGroups } from "./FavoriteLaunchGroups";
import { LaunchButton } from "./LaunchButton";
import { LaunchGroupMenu } from "./LaunchGroupMenu";
import { RoomAutomationQuickEdit } from "./RoomAutomationQuickEdit";
import type { ModCapsulePool } from "../../store/types";
import type { BatchSelection } from "../../hooks/useAccountBatch";
import { useI18n } from "../../i18n";
import { AccountSelectionMenu } from "./AccountSelectionMenu";
import type { BatchMode } from "../../hooks/useAccountBatch";
import { WindowLayoutQuickControls } from "../../features/windowLayouts/WindowLayoutQuickControls";

interface MainActionBarProps {
  modNotice?: boolean;
  onReadModNotice?: () => void;
  batchSelection?: BatchSelection;
  batchSelectableIds?: Record<Exclude<BatchMode, null>, string[]>;
  onSelectAllBatch?: (mode: Exclude<BatchMode, null>) => void;
  onClearBatch?: () => void;
  batchBusy?: boolean;
  batchUncertain?: boolean;
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
  onOpenWindowLayouts?: () => void;
  showOptionalFeatures?: boolean;
  modCapsulePool?: ModCapsulePool | null;
}

export function MainActionBar({
  modNotice, onReadModNotice, batchSelection, batchBusy, batchUncertain,
  batchSelectableIds, onSelectAllBatch, onClearBatch,
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
  onOpenWindowLayouts,
  showOptionalFeatures = true,
  modCapsulePool,
}: MainActionBarProps) {
  const { config, saving } = useGlobalConfig();
  const { accounts } = useAccounts();
  const { t } = useI18n();
  const draft = launchGroups.draft;
  const closingSelection = batchSelection?.mode === "close";
  const closeAction = <button type="button" onClick={onRequestKillAll}
    disabled={!!batchBusy || (closingSelection && batchUncertain)}
    title={t(closingSelection ? "account.close.selectedHint" : "account.close.allHint")}
    data-i18n-skip className={closingSelection ? "danger-cta" : "control-btn danger-control ml-1 min-w-[72px]"}>
    {closingSelection && <Square size={13} aria-hidden="true" />}
    {batchBusy ? t("account.close.busy") : closingSelection
      ? t("account.close.selected", { count: batchSelection.ids.length }) : t("account.close.all")}
  </button>;

  return (
    <ActionBar disabled={!!draft}>
      {launching ? (
        <button onClick={onCancelLaunch} className="danger-cta">
          取消操作
        </button>
      ) : (
        <>
        <div className="dashboard-launch-controls" role="group" aria-label={config?.app_language === "en-US" ? "Launch and close accounts" : "账号启动与关闭"}>
          {closingSelection && closeAction}
          {batchSelection?.mode !== "close" && <LaunchButton
            count={batchSelection?.mode === "launch" ? batchSelection.ids.length : launchableAccountIds.length}
            selected={batchSelection?.mode === "launch"}
            loading={launching || !!batchBusy || !!batchUncertain || saving}
            onClick={() => onStartLaunch(batchSelection?.mode === "launch" ? batchSelection.ids : launchableAccountIds)}
          />}
          {batchSelectableIds && onSelectAllBatch && onClearBatch && <AccountSelectionMenu
            selection={batchSelection ?? { mode: null, ids: [] }}
            selectableIds={batchSelectableIds}
            disabled={!!batchBusy || saving || !!draft}
            uncertain={batchUncertain}
            onSelectAll={onSelectAllBatch}
            onClear={onClearBatch}
          />}
          {!batchSelection?.mode && <FavoriteLaunchGroups
            groups={config?.launch_groups ?? []}
            favoriteGroupIds={config?.favorite_launch_group_ids}
            accounts={accounts}
            config={config}
            modCapsulePool={modCapsulePool}
            disabled={launching || saving || !!batchSelection?.mode || !!batchBusy}
            onLaunch={launchGroups.launch}
            onToggleFavorite={group => void launchGroups.toggleFavorite(group)}
          />}
          <div className="dashboard-close-action">{!batchSelection?.mode && closeAction}</div>
        </div>
        <div className="dashboard-tool-controls">
        <div className="dashboard-quick-controls" role="group" aria-label={config?.app_language === "en-US" ? "Quick settings" : "功能快捷设置"}>
          {showOptionalFeatures && !batchSelection?.mode && (
            <RoomAutomationQuickEdit
              active={config?.installed_optional_modules?.includes("room-automation") === true}
              language={config?.app_language}
              onOpenSettings={onOpenRoomAutomation}
            />
          )}
          {showOptionalFeatures && !batchSelection?.mode && <RecognitionQuickEdit />}
          {onOpenWindowLayouts && <WindowLayoutQuickControls onOpenSettings={onOpenWindowLayouts} disabled={launching || !!batchBusy || !!draft} />}
        </div>
        <div className="dashboard-manage-controls" role="group" aria-label={config?.app_language === "en-US" ? "Management" : "管理工具"}>
        <button type="button" className="control-btn" onClick={() => { onReadModNotice?.(); onOpenModManager(); }}>
          <PackageOpen size={13} strokeWidth={1.9} aria-hidden="true" />
          {config?.app_language === "en-US" ? "Manage Mods" : "Mod 管理"}
          {modNotice && <small className="mod-update-hint">{config?.app_language === "en-US" ? "Update available" : "可更新"}</small>}
        </button>
        <LaunchGroupMenu
          count={config?.launch_groups.length ?? 0}
          open={launchGroupPanelOpen}
          disabled={launching || saving || !!batchBusy}
          onToggle={onToggleLaunchGroupPanel}
        />
          <button onClick={onAddAccount} className="control-btn add-account-cta">
            <UserPlus size={13} strokeWidth={1.9} />
            {config?.app_language === "en-US" ? "Add account" : "添加账号"}
          </button>
        </div>
        </div>
        </>
      )}
    </ActionBar>
  );
}
