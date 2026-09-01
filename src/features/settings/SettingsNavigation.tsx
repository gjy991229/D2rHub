import { useEffect, useRef, useState, type KeyboardEvent } from "react";
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
    const accepted = onSelect(next.id) !== false;
    buttonRefs.current.get(accepted ? next.id : activeTab)?.focus();
  };

  const selectFromClick = (next: SettingsTabId) => {
    if (onSelect(next) === false) {
      buttonRefs.current.get(activeTab)?.focus();
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
                    {feature.kind === "optional" && (
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
