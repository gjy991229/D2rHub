import { AlertTriangle, Play } from "lucide-react";

import type { AccountMeta, GlobalConfig, LaunchGroup } from "../../store/types";
import {
  favoriteLaunchGroups,
  inspectLaunchGroup,
  launchGroupIssueDetails,
} from "../../utils/launchGroups";

interface FavoriteLaunchGroupsProps {
  groups: LaunchGroup[];
  favoriteGroupIds?: string[];
  accounts: AccountMeta[];
  config: GlobalConfig | null;
  disabled?: boolean;
  onLaunch: (group: LaunchGroup) => void;
}

export function FavoriteLaunchGroups({
  groups,
  favoriteGroupIds,
  accounts,
  config,
  disabled = false,
  onLaunch,
}: FavoriteLaunchGroupsProps) {
  const favorites = favoriteLaunchGroups(groups, favoriteGroupIds);
  if (favorites.length === 0) return null;

  return (
    <div className="favorite-launch-groups" role="group" aria-label="常用启动方案">
      {favorites.map(group => {
        const availability = inspectLaunchGroup(group, accounts, config);
        const unavailableReason = launchGroupIssueDetails(availability.issues)
          || "方案尚未选择账号";
        return (
          <button
            key={group.id}
            type="button"
            className="control-btn favorite-launch-group"
            data-warning={!availability.can_launch ? "true" : undefined}
            disabled={disabled || !availability.can_launch}
            title={availability.can_launch ? `启动常用方案“${group.name}”` : unavailableReason}
            onClick={() => onLaunch(group)}
          >
            {availability.can_launch
              ? <Play size={11} fill="currentColor" strokeWidth={1.7} aria-hidden="true" />
              : <AlertTriangle size={11} strokeWidth={1.8} aria-hidden="true" />}
            <span>{group.name}</span>
          </button>
        );
      })}
    </div>
  );
}
