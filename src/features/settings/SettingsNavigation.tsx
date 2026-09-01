import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { GlobalConfig } from "../../store/types";
import {
  SETTINGS_FEATURES,
  SETTINGS_COPY,
  SETTINGS_GROUP_COPY,
  SETTINGS_GROUPS,
  normalizeSettingsLanguage,
  type SettingsTabId,
} from "./settingsRegistry";

interface SettingsNavigationProps {
  activeTab: SettingsTabId;
  config?: GlobalConfig | null;
  language?: string | null;
  onSelect: (tab: SettingsTabId) => void;
}

export function SettingsNavigation({ activeTab, config, language, onSelect }: SettingsNavigationProps) {
  const buttonRefs = useRef(new Map<SettingsTabId, HTMLButtonElement>());
  const [compact, setCompact] = useState(false);
  const locale = normalizeSettingsLanguage(language);

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const media = window.matchMedia("(max-width: 780px)");
    const update = () => setCompact(media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      buttonRefs.current.get(activeTab)?.scrollIntoView?.({
        block: "nearest",
        inline: "nearest",
      });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [activeTab]);

  const moveFocus = (event: KeyboardEvent<HTMLButtonElement>, current: SettingsTabId) => {
    if (!["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft", "Home", "End"].includes(event.key)) {
      return;
    }

    event.preventDefault();
    const currentIndex = SETTINGS_FEATURES.findIndex((feature) => feature.id === current);
    const nextIndex = event.key === "Home"
      ? 0
      : event.key === "End"
        ? SETTINGS_FEATURES.length - 1
        : event.key === "ArrowDown" || event.key === "ArrowRight"
          ? (currentIndex + 1) % SETTINGS_FEATURES.length
          : (currentIndex - 1 + SETTINGS_FEATURES.length) % SETTINGS_FEATURES.length;
    const next = SETTINGS_FEATURES[nextIndex];
    onSelect(next.id);
    buttonRefs.current.get(next.id)?.focus();
  };

  return (
    <nav
      className="settings-navigation"
      aria-label={locale === "en-US" ? "Settings categories" : "设置分类"}
    >
      <div role="tablist" aria-orientation={compact ? "horizontal" : "vertical"}>
      {SETTINGS_GROUPS.map((group) => {
        const features = SETTINGS_FEATURES.filter((feature) => feature.group === group.id);
        const groupCopy = SETTINGS_GROUP_COPY[locale][group.id];
        return (
          <section className="settings-navigation-group" key={group.id} role="presentation">
            <div className="settings-navigation-heading">
              <span>{groupCopy.label}</span>
              <span>{groupCopy.note}</span>
            </div>
            <div role="presentation">
              {features.map((feature) => {
                const Icon = feature.icon;
                const selected = feature.id === activeTab;
                const enabled = feature.isEnabled && config ? feature.isEnabled(config) : false;
                const copy = SETTINGS_COPY[locale][feature.id];
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
                    tabIndex={selected ? 0 : -1}
                    className="settings-navigation-item"
                    data-active={selected ? "true" : "false"}
                    onClick={() => onSelect(feature.id)}
                    onKeyDown={(event) => moveFocus(event, feature.id)}
                  >
                    <Icon size={15} aria-hidden="true" />
                    <span className="min-w-0">
                      <span className="settings-navigation-label">{copy.label}</span>
                      <span className="settings-navigation-description">{copy.description}</span>
                    </span>
                    {feature.kind === "optional" && (
                      <span className="settings-navigation-badge" data-enabled={enabled ? "true" : "false"}>
                        {locale === "en-US"
                          ? enabled ? "On" : "Off"
                          : enabled ? "已启用" : "未启用"}
                      </span>
                    )}
                  </button>
                );
              })}
            </div>
          </section>
        );
      })}
      </div>
    </nav>
  );
}
