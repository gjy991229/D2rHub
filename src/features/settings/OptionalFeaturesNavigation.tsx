import { useEffect, useRef, type KeyboardEvent } from "react";
import type {
  CapabilityRuntimeState,
  CapabilityStatusSnapshot,
  GlobalConfig,
} from "../../store/types";
import { aggregateCapabilityStatuses } from "../capabilities";
import {
  OPTIONAL_SETTINGS_TABS,
  SETTINGS_COPY,
  SETTINGS_FEATURES,
  normalizeSettingsLanguage,
  type SettingsTabId,
} from "./settingsRegistry";

interface OptionalFeaturesNavigationProps {
  activeTab: SettingsTabId;
  config?: GlobalConfig | null;
  capabilityStatus?: CapabilityStatusSnapshot | null;
  capabilityStatusUnavailable?: boolean;
  language?: string | null;
  onSelect: (tab: SettingsTabId) => boolean | void;
}

const STATUS_COPY: Record<"zh-CN" | "en-US", Record<CapabilityRuntimeState | "configured" | "unconfigured" | "unknown", string>> = {
  "zh-CN": {
    disabled: "已停用",
    stopped: "待启动",
    starting: "启动中",
    running: "运行中",
    degraded: "受限",
    failed: "异常",
    configured: "已配置",
    unconfigured: "未配置",
    unknown: "状态不可用",
  },
  "en-US": {
    disabled: "Off",
    stopped: "Pending",
    starting: "Starting",
    running: "Running",
    degraded: "Limited",
    failed: "Error",
    configured: "Configured",
    unconfigured: "Not configured",
    unknown: "Unavailable",
  },
};

export function OptionalFeaturesNavigation({
  activeTab,
  config,
  capabilityStatus,
  capabilityStatusUnavailable = false,
  language,
  onSelect,
}: OptionalFeaturesNavigationProps) {
  const locale = normalizeSettingsLanguage(language);
  const buttonRefs = useRef(new Map<SettingsTabId, HTMLButtonElement>());
  const features = OPTIONAL_SETTINGS_TABS.map((id) => (
    SETTINGS_FEATURES.find((feature) => feature.id === id)
  )).filter((feature): feature is NonNullable<typeof feature> => !!feature);

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      buttonRefs.current.get(activeTab)?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [activeTab]);

  const select = (tab: SettingsTabId) => {
    if (onSelect(tab) === false) buttonRefs.current.get(activeTab)?.focus();
    else buttonRefs.current.get(tab)?.focus();
  };

  const moveFocus = (event: KeyboardEvent<HTMLButtonElement>, current: SettingsTabId) => {
    if (!["ArrowRight", "ArrowLeft", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const currentIndex = features.findIndex((feature) => feature.id === current);
    const nextIndex = event.key === "Home"
      ? 0
      : event.key === "End"
        ? features.length - 1
        : event.key === "ArrowRight"
          ? (currentIndex + 1) % features.length
          : (currentIndex - 1 + features.length) % features.length;
    select(features[nextIndex].id);
  };

  return (
    <nav
      className="optional-features-navigation"
      aria-label={locale === "en-US" ? "Optional features" : "可选功能"}
    >
      <div role="tablist" aria-orientation="horizontal">
        {features.map((feature) => {
          const Icon = feature.icon;
          const selected = feature.id === activeTab;
          const copy = SETTINGS_COPY[locale][feature.id];
          const runtime = feature.capabilityIds && !capabilityStatusUnavailable
            ? aggregateCapabilityStatuses(capabilityStatus ?? null, feature.capabilityIds)
            : null;
          const configured = !!(feature.isConfigured && config && feature.isConfigured(config));
          const state = feature.capabilityIds
            ? capabilityStatusUnavailable
              ? "unknown"
              : runtime?.state ?? "unknown"
            : feature.isConfigured
              ? configured ? "configured" : "unconfigured"
              : null;
          const statusText = state ? STATUS_COPY[locale][state] : null;
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
              aria-label={statusText ? `${copy.label} · ${statusText}` : copy.label}
              tabIndex={selected ? 0 : -1}
              className="optional-features-navigation-item"
              data-active={selected ? "true" : "false"}
              data-state={state ?? undefined}
              onClick={() => select(feature.id)}
              onKeyDown={(event) => moveFocus(event, feature.id)}
            >
              <Icon size={15} aria-hidden="true" />
              <span>{copy.label}</span>
              {state && (
                <span
                  className="optional-features-status-dot"
                  data-state={state}
                  title={statusText ?? undefined}
                  aria-hidden="true"
                />
              )}
            </button>
          );
        })}
      </div>
    </nav>
  );
}
