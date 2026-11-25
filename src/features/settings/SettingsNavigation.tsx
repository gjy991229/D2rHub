import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { CapabilityStatusSnapshot, GlobalConfig } from "../../store/types";
import { aggregateCapabilityStatuses } from "../capabilities";
import { isMinimalMode } from "../profile/featureProfile";
import {
  SETTINGS_FEATURES,
  SETTINGS_COPY,
  SETTINGS_GROUP_COPY,
  SETTINGS_GROUPS,
  isSettingsTabAvailableInMinimal,
  isOptionalModuleTab,
  normalizeSettingsLanguage,
  type OptionalModuleTabId,
  type SettingsTabId,
} from "./settingsRegistry";

interface SettingsNavigationProps {
  activeTab: SettingsTabId;
  config?: GlobalConfig | null;
  capabilityStatus?: CapabilityStatusSnapshot | null;
  capabilityStatusUnavailable?: boolean;
  language?: string | null;
  installedModules?: readonly OptionalModuleTabId[];
  onSelect: (tab: SettingsTabId) => boolean | void | Promise<boolean | void>;
}

const NAVIGATION_ORDER: readonly SettingsTabId[] = [
  "accounts", "paths", "window-layouts", "mod-processing",
  "module-management", "overlays", "automation", "room-automation", "pet",
  "appearance", "shortcuts", "agent", "tasks", "advanced",
];

const COMPACT_WIDTH = "(max-width: 560px)";

export function SettingsNavigation({
  activeTab,
  config,
  capabilityStatus,
  capabilityStatusUnavailable = false,
  language,
  installedModules = [],
  onSelect,
}: SettingsNavigationProps) {
  const buttonRefs = useRef(new Map<SettingsTabId, HTMLButtonElement>());
  const selectionPending = useRef(false);
  const [selecting, setSelecting] = useState(false);
  const [compact, setCompact] = useState(() => window.matchMedia?.(COMPACT_WIDTH)?.matches ?? false);
  const locale = normalizeSettingsLanguage(language);
  const en = locale === "en-US";
  const minimalMode = isMinimalMode(config);
  const entries = NAVIGATION_ORDER
    .map((id) => SETTINGS_FEATURES.find((feature) => feature.id === id)!)
    .filter((feature) => !minimalMode || isSettingsTabAvailableInMinimal(feature.id))
    .filter((feature) => !isOptionalModuleTab(feature.id) || installedModules.includes(feature.id))
    .map((feature) => {
      const copy = SETTINGS_COPY[locale][feature.id];
      const runtime = feature.capabilityIds && !capabilityStatusUnavailable
        ? aggregateCapabilityStatuses(capabilityStatus ?? null, feature.capabilityIds)
        : null;
      const issue = runtime?.state === "failed" ? (en ? "Error" : "异常")
        : runtime?.state === "degraded" ? (en ? "Limited" : "受限") : null;
      return { ...feature, copy, label: copy.navigationLabel ?? copy.label, state: runtime?.state, issue };
    });
  const groups = SETTINGS_GROUPS.map(({ id }) => ({
    id,
    label: SETTINGS_GROUP_COPY[locale][id].label,
    features: entries.filter((feature) => feature.group === id),
  })).filter((group) => group.features.length);

  useEffect(() => {
    const media = window.matchMedia?.(COMPACT_WIDTH);
    if (!media) return;
    const resize = () => setCompact(media.matches);
    resize();
    media.addEventListener("change", resize);
    return () => media.removeEventListener("change", resize);
  }, []);

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      buttonRefs.current.get(activeTab)?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [activeTab, compact]);

  const select = async (tab: SettingsTabId) => {
    if (selectionPending.current) return;
    selectionPending.current = true;
    setSelecting(true);
    try {
      const accepted = await onSelect(tab) !== false;
      buttonRefs.current.get(accepted ? tab : activeTab)?.focus();
    } catch {
      buttonRefs.current.get(activeTab)?.focus();
    } finally {
      selectionPending.current = false;
      setSelecting(false);
    }
  };

  const moveFocus = (event: KeyboardEvent<HTMLButtonElement>, current: SettingsTabId) => {
    if (!["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const index = entries.findIndex((entry) => entry.id === current);
    const next = event.key === "Home" ? 0
      : event.key === "End" ? entries.length - 1
        : event.key === "ArrowDown" || event.key === "ArrowRight"
          ? (index + 1) % entries.length
          : (index - 1 + entries.length) % entries.length;
    void select(entries[next].id);
  };

  if (compact) {
    return (
      <nav className="settings-category-picker" aria-label={en ? "Settings categories" : "设置分类"}>
        <label>
          <span>{en ? "Category" : "设置分类"}</span>
          <select
            value={activeTab}
            aria-controls={`settings-panel-${activeTab}`}
            aria-busy={selecting}
            aria-disabled={selecting}
            onChange={(event) => void select(event.target.value as SettingsTabId)}
          >
            {groups.map(({ id, label, features }) => (
              <optgroup key={id} label={label}>
                {features.map((feature) => (
                  <option key={feature.id} id={`settings-tab-${feature.id}`} value={feature.id}>
                    {feature.issue ? `${feature.label} · ${feature.issue}` : feature.label}
                  </option>
                ))}
              </optgroup>
            ))}
          </select>
        </label>
      </nav>
    );
  }

  return (
    <nav className="settings-navigation settings-navigation-unified" aria-label={en ? "Settings categories" : "设置分类"}>
      <div role="tablist" aria-orientation="vertical" aria-busy={selecting}>
        {groups.map((group) => (
            <section className="settings-navigation-group" key={group.id} role="presentation" data-group={group.id}>
              <div className="settings-navigation-heading">{group.label}</div>
              {group.features.map((feature) => {
                const Icon = feature.icon;
                const selected = feature.id === activeTab;
                const { label, issue, copy } = feature;
                return (
                  <button
                    key={feature.id}
                    ref={(element) => {
                      if (element) buttonRefs.current.set(feature.id, element);
                      else buttonRefs.current.delete(feature.id);
                    }}
                    type="button"
                    role="tab"
                    id={`settings-tab-${feature.id}`}
                    aria-selected={selected}
                    aria-controls={`settings-panel-${feature.id}`}
                    aria-label={issue ? `${label} · ${issue}` : label}
                    aria-disabled={selecting}
                    title={copy.description}
                    tabIndex={selected ? 0 : -1}
                    className="settings-navigation-item"
                    data-active={selected ? "true" : "false"}
                    onClick={() => void select(feature.id)}
                    onKeyDown={(event) => moveFocus(event, feature.id)}
                  >
                    <span className="settings-nav-symbol" aria-hidden="true"><Icon size={15} strokeWidth={1.8} /></span>
                    <span className="settings-navigation-label">{label}</span>
                    {issue && <span className="settings-navigation-issue" data-state={feature.state}>{issue}</span>}
                  </button>
                );
              })}
            </section>
        ))}
      </div>
    </nav>
  );
}
