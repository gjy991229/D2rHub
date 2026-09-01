import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import type { GlobalConfig } from "../../store/types";
import { SettingsNavigation } from "./SettingsNavigation";
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
    expect(getSettingsFeaturesByKind("core").map((feature) => feature.id)).toEqual(["accounts"]);
    expect(getSettingsFeaturesByKind("platform").map((feature) => feature.id)).toEqual([
      "paths",
      "agent",
      "appearance",
      "tasks",
      "advanced",
    ]);
    expect(getSettingsFeaturesByKind("optional").map((feature) => feature.id)).toEqual([
      "shortcuts",
      "overlays",
      "automation",
      "mod-processing",
      "room-automation",
      "pet",
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
    const automationTab = screen.getByRole("tab", { name: /识别与统计/ });
    expect(coreTab.getAttribute("aria-selected")).toBe("true");

    fireEvent.click(automationTab);

    expect(automationTab.getAttribute("aria-selected")).toBe("true");
    expect(coreTab.getAttribute("aria-selected")).toBe("false");
  });

  it("supports arrow-key navigation across capability groups", () => {
    render(<Harness />);
    const coreTab = screen.getByRole("tab", { name: /账号与实例/ });

    fireEvent.keyDown(coreTab, { key: "ArrowRight" });

    expect(screen.getByRole("tab", { name: /运行环境/ }).getAttribute("aria-selected")).toBe("true");
  });

  it("keeps focus on the active tab when a guarded selection is rejected", () => {
    render(<SettingsNavigation activeTab="room-automation" onSelect={() => false} />);
    const active = screen.getByRole("tab", { name: /自动跟房/ }) as HTMLButtonElement;
    const next = screen.getByRole("tab", { name: /桌面伴随/ }) as HTMLButtonElement;

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
    expect(screen.getByRole("tab", { name: /Desktop Overlays/ })).toBeTruthy();
    expect(screen.getByText("Optional features")).toBeTruthy();
    expect(screen.queryByText("账号与实例")).toBeNull();
  });

  it("renders observed lifecycle state instead of inferring desktop pet health from config", () => {
    render(
      <SettingsNavigation
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

    const badge = screen.getByRole("tab", { name: /桌面伴随/ })
      .querySelector(".settings-navigation-badge");
    expect(badge?.textContent).toBe("异常");
    expect(badge?.getAttribute("data-state")).toBe("failed");
  });

  it("does not present a stale runtime snapshot when status synchronization is unavailable", () => {
    render(
      <SettingsNavigation
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

    const badge = screen.getByRole("tab", { name: /桌面伴随/ })
      .querySelector(".settings-navigation-badge");
    expect(badge?.textContent).toBe("不可用");
    expect(badge?.getAttribute("data-state")).toBe("unknown");
  });
});
