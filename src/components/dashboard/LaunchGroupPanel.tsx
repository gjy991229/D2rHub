import { AlertTriangle, Pencil, Play, Plus, Star, X } from "lucide-react";
import { useState } from "react";

import { useI18n } from "../../i18n";
import type { AccountMeta, GlobalConfig, LaunchGroup } from "../../store/types";
import {
  inspectLaunchGroup,
  launchGroupAccountIds,
  launchGroupIssueDetails,
  MAX_FAVORITE_LAUNCH_GROUPS,
  normalizeFavoriteLaunchGroupIds,
} from "../../utils/launchGroups";

interface LaunchGroupPanelProps {
  groups: LaunchGroup[];
  accounts: AccountMeta[];
  config: GlobalConfig | null;
  favoriteGroupIds?: string[];
  disabled?: boolean;
  onClose: () => void;
  onLaunch: (group: LaunchGroup) => void;
  onCreate: () => void;
  onEdit: (group: LaunchGroup) => void;
  onToggleFavorite: (group: LaunchGroup) => void;
}

export function LaunchGroupPanel({
  groups,
  accounts,
  config,
  favoriteGroupIds = [],
  disabled = false,
  onClose,
  onLaunch,
  onCreate,
  onEdit,
  onToggleFavorite,
}: LaunchGroupPanelProps) {
  const { t } = useI18n();
  const [selectedGroupId, setSelectedGroupId] = useState<string | null>(null);
  const favorites = new Set(normalizeFavoriteLaunchGroupIds(groups, favoriteGroupIds));
  const favoriteLimitReached = favorites.size >= MAX_FAVORITE_LAUNCH_GROUPS;
  const orderedGroups = groups
    .map((group, index) => ({ group, index }))
    .sort((a, b) => Number(favorites.has(b.group.id)) - Number(favorites.has(a.group.id)) || a.index - b.index)
    .map(({ group }) => group);

  return (
    <aside id="launch-group-panel" className="launch-group-panel" aria-label={t("launch.scheme.label")}>
      <header className="launch-group-panel-header">
        <div>
          <h2>{t("launch.scheme.label")}</h2>
          <p>{t("launch.scheme.subtitle")}</p>
        </div>
        <button type="button" className="icon-btn" aria-label={t("launch.scheme.closePanel")} onClick={onClose}>
          <X size={14} />
        </button>
      </header>

      <div className="launch-group-panel-list">
        {orderedGroups.length === 0 ? (
          <div className="launch-group-empty">
            <div>
              <p>{t("launch.scheme.empty.title")}</p>
              <span>{t("launch.scheme.empty.body")}</span>
            </div>
          </div>
        ) : orderedGroups.map((group) => {
          const availability = inspectLaunchGroup(group, accounts, config);
          const memberCount = launchGroupAccountIds(group).length;
          const isFavorite = favorites.has(group.id);
          const status = memberCount === 0
            ? t("launch.scheme.status.empty")
            : availability.issues.length > 0
              ? t("launch.scheme.status.unavailable", { count: availability.issues.length })
              : t("launch.scheme.status.ready", { count: memberCount });
          const issueTitle = launchGroupIssueDetails(availability.issues);
          const selected = selectedGroupId === group.id;
          return (
            <article
              className="launch-group-panel-row"
              data-selected={selected ? "true" : undefined}
              data-warning={!availability.can_launch ? "true" : undefined}
              key={group.id}
            >
              <button
                type="button"
                className="launch-group-panel-launch"
                disabled={disabled}
                aria-pressed={selected}
                aria-disabled={!availability.can_launch}
                title={issueTitle || t(selected ? "launch.scheme.launchSelectedTitle" : "launch.scheme.selectTitle", { name: group.name })}
                onClick={() => {
                  if (selected && availability.can_launch) onLaunch(group);
                  else setSelectedGroupId(group.id);
                }}
              >
                <span className="launch-group-play" aria-hidden="true">
                  {availability.can_launch
                    ? <Play size={12} fill="currentColor" />
                    : <AlertTriangle size={12} />}
                </span>
                <span className="launch-group-copy">
                  <strong data-i18n-skip>{group.name}</strong>
                  <span>{status}</span>
                </span>
              </button>
              <button
                type="button"
                className="launch-group-panel-favorite"
                data-active={isFavorite ? "true" : undefined}
                aria-label={t(isFavorite ? "launch.favorite.removeLabel" : "launch.favorite.addLabel", { name: group.name })}
                title={t(isFavorite
                  ? "launch.favorite.removeTitle"
                  : favoriteLimitReached
                    ? "launch.favorite.limitTitle"
                    : "launch.favorite.addTitle")}
                disabled={disabled || (!isFavorite && favoriteLimitReached)}
                onClick={() => onToggleFavorite(group)}
              >
                <Star size={13} fill={isFavorite ? "currentColor" : "none"} />
              </button>
              <button
                type="button"
                className="launch-group-panel-edit"
                aria-label={t("launch.scheme.editLabel", { name: group.name })}
                title={t("launch.scheme.editTitle", { name: group.name })}
                disabled={disabled}
                onClick={() => onEdit(group)}
              >
                <Pencil size={12} />
              </button>
            </article>
          );
        })}
      </div>

      <button type="button" className="launch-group-create" disabled={disabled} onClick={onCreate}>
        <Plus size={13} />
        {t("launch.scheme.create")}
      </button>
    </aside>
  );
}
