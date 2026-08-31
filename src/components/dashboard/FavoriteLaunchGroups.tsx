import { AlertTriangle, Play } from "lucide-react";

import type { AccountMeta, GlobalConfig, LaunchGroup } from "../../store/types";
import {
  favoriteLaunchGroups,
  inspectLaunchGroup,
  launchGroupIssueDetails,
} from "../../utils/launchGroups";
import { useI18n } from "../../i18n";

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
  const { t } = useI18n();
  const favorites = favoriteLaunchGroups(groups, favoriteGroupIds);
  if (favorites.length === 0) return null;

  return (
    <div className="favorite-launch-groups" role="group" aria-label={t("launch.favorite.groupLabel")}>
      {favorites.map(group => {
        const availability = inspectLaunchGroup(group, accounts, config);
        const unavailableReason = launchGroupIssueDetails(availability.issues)
          || t("launch.favorite.emptyReason");
        return (
          <button
            key={group.id}
            type="button"
            className="control-btn favorite-launch-group"
            data-warning={!availability.can_launch ? "true" : undefined}
            disabled={disabled || !availability.can_launch}
            title={availability.can_launch
              ? t("launch.favorite.launchTitle", { name: group.name })
              : unavailableReason}
            onClick={() => onLaunch(group)}
          >
            {availability.can_launch
              ? <Play size={11} fill="currentColor" strokeWidth={1.7} aria-hidden="true" />
              : <AlertTriangle size={11} strokeWidth={1.8} aria-hidden="true" />}
            <span data-i18n-skip>{group.name}</span>
          </button>
        );
      })}
    </div>
  );
}
