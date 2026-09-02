import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import type { GlobalConfig } from "../../store/types";
import { SettingsNavigation } from "./SettingsNavigation";
import { OptionalFeaturesNavigation } from "./OptionalFeaturesNavigation";
import {
  getSettingsFeaturesByKind,
  SETTINGS_FEATURES,
  type SettingsTabId,
} from "./settingsRegistry";

function Harness() {
  const [active, setActive] = useState<SettingsTabId>("accounts");
  return <SettingsNavigation activeTab={active} onSelect={setActive} />;
}

afterEach(cleanup);

describe("settings feature registry", () => {
  it("keeps the multi-instance core separate from optional capabilities", () => {
    expect(getSettingsFeaturesByKind("core").map((feature) => feature.id)).toEqual([
      "accounts",
      "shortcuts",
    ]);
    expect(getSettingsFeaturesByKind("platform").map((feature) => feature.id)).toEqual([
      "paths",
      "agent",
      "appearance",
      "tasks",
      "advanced",
    ]);
    expect(getSettingsFeaturesByKind("optional").map((feature) => feature.id)).toEqual([
      "overlays",
      "pet",
      "automation",
      "room-automation",
      "mod-processing",
    ]);
    expect(new Set(SETTINGS_FEATURES.map((feature) => feature.id)).size).toBe(SETTINGS_FEATURES.length);
  });

  it("maps supervised optional modules to backend capability ids", () => {
    const shortcuts = SETTINGS_FEATURES.find((feature) => feature.id === "shortcuts");
    const overlays = SETTINGS_FEATURES.find((feature) => feature.id === "overlays");

    expect(shortcuts?.isConfigured?.({ shortcut_bindings_json: '{"1":" Ctrl+1 ","2":""}' } as GlobalConfig)).toBe(true);
    expect(shortcuts?.isConfigured?.({ shortcut_bindings_json: '{"1":"   "}' } as GlobalConfig)).toBe(false);
    expect(shortcuts?.isConfigured?.({ shortcut_bindings_json: "invalid" } as GlobalConfig)).toBe(false);
    expect(overlays?.isConfigured?.({ enable_tz_overlay: false, enable_stats_overlay: true } as GlobalConfig)).toBe(true);
    expect(overlays?.isConfigured?.({ enable_tz_overlay: false, enable_stats_overlay: false } as GlobalConfig)).toBe(false);
    expect(overlays?.capabilityIds).toEqual(["terror-zone-overlay", "statistics-overlay"]);
    expect(SETTINGS_FEATURES.find((feature) => feature.id === "automation")?.capabilityIds).toEqual(["audio-telemetry"]);
    expect(SETTINGS_FEATURES.find((feature) => feature.id === "pet")?.capabilityIds).toEqual(["desktop-pet"]);
    expect(SETTINGS_FEATURES.find((feature) => feature.id === "room-automation")?.capabilityIds).toEqual(["room-automation"]);
  });
});

describe("SettingsNavigation", () => {
  it("selects a module and exposes tab semantics", () => {
    render(<Harness />);

    const coreTab = screen.getByRole("tab", { name: /账号与实例/ });
    const optionalTab = screen.getByRole("tab", { name: /可选功能/ });
    expect(coreTab.getAttribute("aria-selected")).toBe("true");

    fireEvent.click(optionalTab);

    expect(optionalTab.getAttribute("aria-selected")).toBe("true");
    expect(coreTab.getAttribute("aria-selected")).toBe("false");
  });

  it("supports arrow-key navigation across capability groups", () => {
    render(<Harness />);
    const coreTab = screen.getByRole("tab", { name: /账号与实例/ });

    fireEvent.keyDown(coreTab, { key: "ArrowRight" });

    expect(screen.getByRole("tab", { name: /窗口快捷键/ }).getAttribute("aria-selected")).toBe("true");
  });

  it("keeps focus on the active tab when a guarded selection is rejected", () => {
    render(<SettingsNavigation activeTab="room-automation" onSelect={() => false} />);
    const active = screen.getByRole("tab", { name: /可选功能/ }) as HTMLButtonElement;
    const next = screen.getByRole("tab", { name: /账号与实例/ }) as HTMLButtonElement;

    next.focus();
    fireEvent.click(next);
    expect(document.activeElement).toBe(active);

    fireEvent.keyDown(active, { key: "ArrowRight" });
    expect(document.activeElement).toBe(active);
    expect(active.getAttribute("aria-selected")).toBe("true");
  });

  it("renders the registry copy in English when the application language is English", () => {
    render(<SettingsNavigation activeTab="accounts" language="en-US" onSelect={() => {}} />);

    expect(screen.getByRole("tab", { name: /Accounts & Instances/ })).toBeTruthy();
    expect(screen.getByRole("tab", { name: /Appearance/ }).querySelector(".settings-navigation-badge")).toBeNull();
    expect(screen.getByRole("tab", { name: /Optional Features/ })).toBeTruthy();
    expect(screen.getByText("Extensions")).toBeTruthy();
    expect(screen.queryByRole("tab", { name: /Desktop Overlays/ })).toBeNull();
    expect(screen.queryByText("账号与实例")).toBeNull();
  });

});

describe("OptionalFeaturesNavigation", () => {
  it("moves optional modules into a horizontal top-level tab list", () => {
    render(<OptionalFeaturesNavigation activeTab="automation" onSelect={() => {}} />);

    expect(screen.getByRole("tab", { name: /识别与统计/ }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tab", { name: /Mod 管理/ })).toBeTruthy();
    expect(screen.getByRole("tab", { name: /自动跟房/ })).toBeTruthy();
    expect(screen.getByRole("tab", { name: /桌面悬浮窗/ })).toBeTruthy();
    expect(screen.getByRole("tab", { name: /桌面伴随/ })).toBeTruthy();
  });

  it("renders observed lifecycle state instead of inferring desktop pet health from config", () => {
    render(
      <OptionalFeaturesNavigation
        activeTab="pet"
        config={{ enable_bongo_cat: true } as GlobalConfig}
        capabilityStatus={{
          revision: 4,
          capabilities: [{
            id: "desktop-pet",
            requested_enabled: true,
            state: "failed",
            reason_code: "window-unavailable",
          }],
        }}
        onSelect={() => {}}
      />,
    );

    const tab = screen.getByRole("tab", { name: /桌面伴随 · 异常/ });
    expect(tab.querySelector(".optional-features-status-dot")?.getAttribute("data-state")).toBe("failed");
  });

  it("does not present a stale runtime snapshot when status synchronization is unavailable", () => {
    render(
      <OptionalFeaturesNavigation
        activeTab="pet"
        capabilityStatus={{
          revision: 9,
          capabilities: [{
            id: "desktop-pet",
            requested_enabled: true,
            state: "running",
            reason_code: null,
          }],
        }}
        capabilityStatusUnavailable
        onSelect={() => {}}
      />,
    );

    const tab = screen.getByRole("tab", { name: /桌面伴随 · 状态不可用/ });
    expect(tab.querySelector(".optional-features-status-dot")?.getAttribute("data-state")).toBe("unknown");
  });
});
