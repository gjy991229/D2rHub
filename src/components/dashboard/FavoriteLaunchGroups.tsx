import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { AlertTriangle, Play, Plus } from "lucide-react";

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
  onToggleFavorite: (group: LaunchGroup) => void;
}

export function FavoriteLaunchGroups({
  groups,
  favoriteGroupIds,
  accounts,
  config,
  disabled = false,
  onLaunch,
  onToggleFavorite,
}: FavoriteLaunchGroupsProps) {
  const { t } = useI18n();
  const favorites = favoriteLaunchGroups(groups, favoriteGroupIds);
  const availableChoices = groups.filter((group) => !favorites.some((favorite) => favorite.id === group.id));
  const [pickerOpen, setPickerOpen] = useState(false);
  const [position, setPosition] = useState<{ left: number; top: number; opensUpward: boolean } | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const pickerRef = useRef<HTMLDivElement | null>(null);
  const pickerId = `favorite-launch-picker-${useId().replaceAll(":", "")}`;

  const updatePosition = useCallback(() => {
    const trigger = triggerRef.current;
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const width = 250;
    const height = Math.min(286, 48 + Math.max(1, availableChoices.length) * 42);
    const gap = 6;
    const padding = 8;
    const opensUpward = window.innerHeight - rect.bottom < height + gap + padding
      && rect.top > height + gap + padding;
    setPosition({
      left: Math.min(window.innerWidth - width - padding, Math.max(padding, rect.left)),
      top: opensUpward ? Math.max(padding, rect.top - height - gap) : rect.bottom + gap,
      opensUpward,
    });
  }, [availableChoices.length]);

  const closePicker = useCallback((restoreFocus = false) => {
    setPickerOpen(false);
    setPosition(null);
    if (restoreFocus) window.requestAnimationFrame(() => triggerRef.current?.focus());
  }, []);

  useLayoutEffect(() => {
    if (pickerOpen) updatePosition();
  }, [pickerOpen, updatePosition]);

  useEffect(() => {
    if (!pickerOpen || !position) return;
    const frame = window.requestAnimationFrame(() => {
      pickerRef.current?.querySelector<HTMLButtonElement>('button:not([disabled])')?.focus();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [pickerOpen, position]);

  const handlePickerKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const items = Array.from(pickerRef.current?.querySelectorAll<HTMLButtonElement>(
      'button:not([disabled])',
    ) ?? []);
    const currentIndex = items.indexOf(document.activeElement as HTMLButtonElement);
    const nextIndex = event.key === "Home"
      ? 0
      : event.key === "End"
        ? items.length - 1
        : event.key === "ArrowDown"
          ? (currentIndex + 1 + items.length) % items.length
          : event.key === "ArrowUp"
            ? (currentIndex - 1 + items.length) % items.length
            : null;
    if (nextIndex === null || items.length === 0) return;
    event.preventDefault();
    items[nextIndex]?.focus();
  };

  useEffect(() => {
    if (!pickerOpen || !position) return;
    const outside = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (triggerRef.current?.contains(target) || pickerRef.current?.contains(target)) return;
      closePicker();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") closePicker(true);
    };
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("keydown", escape);
    window.addEventListener("resize", updatePosition);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("keydown", escape);
      window.removeEventListener("resize", updatePosition);
    };
  }, [closePicker, pickerOpen, position, updatePosition]);

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
      <button
        ref={triggerRef}
        type="button"
        className="control-btn favorite-launch-add"
        disabled={disabled}
        aria-label={t("launch.favorite.manage")}
        title={t("launch.favorite.manage")}
        aria-haspopup="menu"
        aria-expanded={pickerOpen}
        aria-controls={pickerOpen ? pickerId : undefined}
        onClick={() => setPickerOpen((open) => !open)}
        onKeyDown={(event) => {
          if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
          event.preventDefault();
          setPickerOpen(true);
        }}
      >
        <Plus size={12} strokeWidth={2} aria-hidden="true" />
      </button>
      {pickerOpen && position && createPortal(
        <div
          ref={pickerRef}
          id={pickerId}
          role="menu"
          aria-label={t("launch.favorite.manage")}
          className="favorite-launch-picker"
          data-placement={position.opensUpward ? "top" : "bottom"}
          style={{ left: position.left, top: position.top }}
          onKeyDown={handlePickerKeyDown}
        >
          <div className="favorite-launch-picker-heading">{t("launch.favorite.pickerTitle")}</div>
          {availableChoices.length === 0 ? (
            <p className="favorite-launch-picker-empty">{t("launch.favorite.pickerEmpty")}</p>
          ) : availableChoices.map((group) => (
            <button
              key={group.id}
              type="button"
              role="menuitem"
              className="favorite-launch-picker-option"
              onClick={() => {
                onToggleFavorite(group);
                closePicker(true);
              }}
            >
              <Plus size={12} aria-hidden="true" />
              <span data-i18n-skip>{group.name}</span>
              <small>{t("launch.favorite.accountCount", { count: group.account_ids.length })}</small>
            </button>
          ))}
        </div>,
        document.body,
      )}
    </div>
  );
}
