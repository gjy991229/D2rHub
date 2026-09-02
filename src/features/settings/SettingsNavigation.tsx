import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Blocks } from "lucide-react";
import type {
  CapabilityRuntimeState,
  CapabilityStatusSnapshot,
  GlobalConfig,
} from "../../store/types";
import { aggregateCapabilityStatuses } from "../capabilities";
import {
  SETTINGS_FEATURES,
  SETTINGS_COPY,
  SETTINGS_GROUP_COPY,
  SETTINGS_GROUPS,
  SETTINGS_OPTIONAL_HUB_COPY,
  isOptionalSettingsTab,
  normalizeSettingsLanguage,
  type SettingsTabId,
} from "./settingsRegistry";

interface SettingsNavigationProps {
  activeTab: SettingsTabId;
  config?: GlobalConfig | null;
  capabilityStatus?: CapabilityStatusSnapshot | null;
  capabilityStatusUnavailable?: boolean;
  language?: string | null;
  onSelect: (tab: SettingsTabId) => boolean | void;
}

const STATUS_COPY: Record<"zh-CN" | "en-US", Record<CapabilityRuntimeState, string>> = {
  "zh-CN": {
    disabled: "已停用",
    stopped: "待启动",
    starting: "启动中",
    running: "运行中",
    degraded: "受限",
    failed: "异常",
  },
  "en-US": {
    disabled: "Off",
    stopped: "Pending",
    starting: "Starting",
    running: "Running",
    degraded: "Limited",
    failed: "Error",
  },
};

export function SettingsNavigation({
  activeTab,
  config,
  capabilityStatus,
  capabilityStatusUnavailable = false,
  language,
  onSelect,
}: SettingsNavigationProps) {
  const buttonRefs = useRef(new Map<string, HTMLButtonElement>());
  const lastOptionalTab = useRef<SettingsTabId>("automation");
  const [compact, setCompact] = useState(false);
  const locale = normalizeSettingsLanguage(language);
  const activeNavigationKey = isOptionalSettingsTab(activeTab) ? "optional-features" : activeTab;
  const primaryEntries = SETTINGS_GROUPS.flatMap((group) => (
    group.id === "optional-features"
      ? [{ key: "optional-features", target: lastOptionalTab.current }]
      : SETTINGS_FEATURES
          .filter((feature) => feature.group === group.id)
          .map((feature) => ({ key: feature.id, target: feature.id }))
  ));

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const media = window.matchMedia("(max-width: 780px)");
    const update = () => setCompact(media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);

  useEffect(() => {
    if (isOptionalSettingsTab(activeTab)) lastOptionalTab.current = activeTab;
    const frame = window.requestAnimationFrame(() => {
      buttonRefs.current.get(activeNavigationKey)?.scrollIntoView?.({
        block: "nearest",
        inline: "nearest",
      });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [activeNavigationKey, activeTab]);

  const moveFocus = (event: KeyboardEvent<HTMLButtonElement>, current: string) => {
    if (!["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft", "Home", "End"].includes(event.key)) {
      return;
    }

    event.preventDefault();
    const currentIndex = primaryEntries.findIndex((entry) => entry.key === current);
    const nextIndex = event.key === "Home"
      ? 0
      : event.key === "End"
        ? primaryEntries.length - 1
        : event.key === "ArrowDown" || event.key === "ArrowRight"
          ? (currentIndex + 1) % primaryEntries.length
          : (currentIndex - 1 + primaryEntries.length) % primaryEntries.length;
    const next = primaryEntries[nextIndex];
    const target = next.key === "optional-features" ? lastOptionalTab.current : next.target;
    const accepted = onSelect(target) !== false;
    buttonRefs.current.get(accepted ? next.key : activeNavigationKey)?.focus();
  };

  const selectFromClick = (next: SettingsTabId, key: string = next) => {
    if (onSelect(next) === false) {
      buttonRefs.current.get(activeNavigationKey)?.focus();
    } else {
      buttonRefs.current.get(key)?.focus();
    }
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
        if (group.id === "optional-features") {
          const hubCopy = SETTINGS_OPTIONAL_HUB_COPY[locale];
          const selected = isOptionalSettingsTab(activeTab);
          const target = selected ? activeTab : lastOptionalTab.current;
          return (
            <section className="settings-navigation-group" key={group.id} role="presentation">
              <div className="settings-navigation-heading">
                <span>{groupCopy.label}</span>
                <span>{groupCopy.note}</span>
              </div>
              <div role="presentation">
                <button
                  ref={(element) => {
                    if (element) buttonRefs.current.set("optional-features", element);
                    else buttonRefs.current.delete("optional-features");
                  }}
                  type="button"
                  role="tab"
                  id="settings-tab-optional-features"
                  aria-selected={selected}
                  aria-controls={`settings-panel-${target}`}
                  tabIndex={selected ? 0 : -1}
                  className="settings-navigation-item"
                  data-active={selected ? "true" : "false"}
                  onClick={() => selectFromClick(target, "optional-features")}
                  onKeyDown={(event) => moveFocus(event, "optional-features")}
                >
                  <Blocks size={15} aria-hidden="true" />
                  <span className="min-w-0">
                    <span className="settings-navigation-label">{hubCopy.label}</span>
                    <span className="settings-navigation-description">{hubCopy.description}</span>
                  </span>
                  <span className="settings-navigation-badge">
                    {hubCopy.badge}
                  </span>
                </button>
              </div>
            </section>
          );
        }
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
                const copy = SETTINGS_COPY[locale][feature.id];
                const runtimeStatus = feature.capabilityIds && !capabilityStatusUnavailable
                  ? aggregateCapabilityStatuses(capabilityStatus ?? null, feature.capabilityIds)
                  : null;
                const configured = feature.isConfigured && config
                  ? feature.isConfigured(config)
                  : false;
                const badgeState = feature.capabilityIds
                  ? runtimeStatus?.state ?? "unknown"
                  : configured ? "configured" : "unconfigured";
                const badgeCopy = feature.capabilityIds
                  ? capabilityStatusUnavailable
                    ? locale === "en-US" ? "Unavailable" : "不可用"
                    : runtimeStatus
                      ? STATUS_COPY[locale][runtimeStatus.state]
                      : locale === "en-US" ? "Checking" : "读取中"
                  : configured
                    ? locale === "en-US" ? "Configured" : "已配置"
                    : locale === "en-US" ? "Not set" : "未配置";
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
                    onClick={() => selectFromClick(feature.id)}
                    onKeyDown={(event) => moveFocus(event, feature.id)}
                  >
                    <Icon size={15} aria-hidden="true" />
                    <span className="min-w-0">
                      <span className="settings-navigation-label">{copy.label}</span>
                      <span className="settings-navigation-description">{copy.description}</span>
                    </span>
                    {feature.kind === "optional" && (feature.capabilityIds || feature.isConfigured) && (
                      <span className="settings-navigation-badge" data-state={badgeState}>
                        {badgeCopy}
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
