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
      "advanced",
    ]);
    expect(getSettingsFeaturesByKind("optional").map((feature) => feature.id)).toEqual([
      "shortcuts",
      "overlays",
      "automation",
      "pet",
    ]);
    expect(new Set(SETTINGS_FEATURES.map((feature) => feature.id)).size).toBe(SETTINGS_FEATURES.length);
  });

  it("derives optional module state from existing persisted fields", () => {
    const shortcuts = SETTINGS_FEATURES.find((feature) => feature.id === "shortcuts");
    const overlays = SETTINGS_FEATURES.find((feature) => feature.id === "overlays");

    expect(shortcuts?.isEnabled?.({ shortcut_bindings_json: '{"1":" Ctrl+1 ","2":""}' } as GlobalConfig)).toBe(true);
    expect(shortcuts?.isEnabled?.({ shortcut_bindings_json: '{"1":"   "}' } as GlobalConfig)).toBe(false);
    expect(shortcuts?.isEnabled?.({ shortcut_bindings_json: "invalid" } as GlobalConfig)).toBe(false);
    expect(overlays?.isEnabled?.({ enable_tz_overlay: false, enable_stats_overlay: true } as GlobalConfig)).toBe(true);
    expect(overlays?.isEnabled?.({ enable_tz_overlay: false, enable_stats_overlay: false } as GlobalConfig)).toBe(false);
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

  it("renders the registry copy in English when the application language is English", () => {
    render(<SettingsNavigation activeTab="accounts" language="en-US" onSelect={() => {}} />);

    expect(screen.getByRole("tab", { name: /Accounts & Instances/ })).toBeTruthy();
    expect(screen.getByRole("tab", { name: /Appearance/ }).querySelector(".settings-navigation-badge")).toBeNull();
    expect(screen.getByRole("tab", { name: /Desktop Overlays/ })).toBeTruthy();
    expect(screen.getByText("Optional features")).toBeTruthy();
    expect(screen.queryByText("账号与实例")).toBeNull();
  });
});
