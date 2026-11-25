import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { GlobalConfig } from "../../store/types";
import { SettingsNavigation } from "./SettingsNavigation";
import { SETTINGS_FEATURES, type SettingsTabId } from "./settingsRegistry";

function Harness() {
  const [active, setActive] = useState<SettingsTabId>("accounts");
  return <SettingsNavigation activeTab={active} onSelect={setActive} installedModules={["automation", "overlays", "room-automation", "pet"]} />;
}

afterEach(cleanup);

describe("settings feature registry", () => {
  it("preserves feature identities and supervised capability ids", () => {
    expect(new Set(SETTINGS_FEATURES.map(feature => feature.id)).size).toBe(SETTINGS_FEATURES.length);
    expect(SETTINGS_FEATURES.find(feature => feature.id === "accounts")?.kind).toBe("core");
    expect(SETTINGS_FEATURES.find(feature => feature.id === "mod-processing")?.group).toBe("game");
    expect(SETTINGS_FEATURES.find(feature => feature.id === "overlays")?.capabilityIds).toEqual(["terror-zone-overlay", "statistics-overlay"]);
    expect(SETTINGS_FEATURES.find(feature => feature.id === "automation")?.capabilityIds).toEqual(["audio-telemetry"]);
    expect(SETTINGS_FEATURES.find(feature => feature.id === "pet")?.capabilityIds).toEqual(["desktop-pet"]);
    expect(SETTINGS_FEATURES.find(feature => feature.id === "room-automation")?.capabilityIds).toEqual(["room-automation"]);
  });
});

describe("SettingsNavigation", () => {
  it("keeps a rejected category change on the current page in a compact window", async () => {
    vi.stubGlobal("matchMedia", () => ({
      matches: true,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }));
    try {
      const onSelect = vi.fn().mockResolvedValue(false);
      render(<SettingsNavigation activeTab="accounts" onSelect={onSelect} />);
      const category = screen.getByRole<HTMLSelectElement>("combobox", { name: "设置分类" });
      fireEvent.change(category, { target: { value: "paths" } });
      await waitFor(() => expect(category.getAttribute("aria-busy")).toBe("false"));
      expect(onSelect).toHaveBeenCalledWith("paths");
      expect(category.value).toBe("accounts");
      expect(category.getAttribute("aria-controls")).toBe("settings-panel-accounts");
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("keeps every added tool in one sidebar, with no nested navigation", () => {
    render(<Harness />);
    expect(screen.getAllByRole("tablist")).toHaveLength(1);
    expect(screen.getByRole("tablist").getAttribute("aria-orientation")).toBe("vertical");
    expect(screen.getAllByRole("tab").map(tab => tab.id)).toEqual([
      "settings-tab-accounts", "settings-tab-paths", "settings-tab-window-layouts", "settings-tab-mod-processing",
      "settings-tab-module-management", "settings-tab-overlays", "settings-tab-automation", "settings-tab-room-automation", "settings-tab-pet",
      "settings-tab-appearance", "settings-tab-shortcuts", "settings-tab-agent", "settings-tab-tasks", "settings-tab-advanced",
    ]);
  });

  it("selects an extension directly and binds it to its own panel", async () => {
    render(<Harness />);
    const tab = screen.getByRole("tab", { name: "识别与统计" });
    fireEvent.click(tab);
    await waitFor(() => expect(tab.getAttribute("aria-selected")).toBe("true"));
    expect(tab.getAttribute("aria-controls")).toBe("settings-panel-automation");
    expect(screen.getByRole("tab", { name: "扩展功能" }).getAttribute("aria-selected")).toBe("false");
    expect(document.activeElement).toBe(tab);
  });

  it("supports arrows, Home, and End with one roving focus target", async () => {
    render(<Harness />);
    fireEvent.keyDown(screen.getByRole("tab", { name: "账号与实例" }), { key: "ArrowDown" });
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("tab", { name: "运行环境" })));
    fireEvent.keyDown(document.activeElement!, { key: "End" });
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("tab", { name: "维护与迁移" })));
    fireEvent.keyDown(document.activeElement!, { key: "Home" });
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("tab", { name: "账号与实例" })));
    expect(screen.getAllByRole("tab").filter(tab => tab.tabIndex === 0)).toHaveLength(1);
  });

  it("waits for a save and restores focus if navigation is rejected", async () => {
    let finish!: (accepted: boolean) => void;
    const onSelect = vi.fn(() => new Promise<boolean>(resolve => { finish = resolve; }));
    render(<SettingsNavigation activeTab="accounts" onSelect={onSelect} />);
    const original = screen.getByRole("tab", { name: "账号与实例" });
    const next = screen.getByRole("tab", { name: "运行环境" });
    fireEvent.click(next);
    expect(screen.getByRole("tablist").getAttribute("aria-busy")).toBe("true");
    fireEvent.click(screen.getByRole("tab", { name: "Mod 管理" }));
    expect(onSelect).toHaveBeenCalledTimes(1);
    await act(async () => finish(false));
    expect(document.activeElement).toBe(original);
    expect(original.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tablist").getAttribute("aria-busy")).toBe("false");
  });

  it("offers the extension overview without advertising tools that are not added", () => {
    render(<SettingsNavigation activeTab="accounts" language="en-US" onSelect={() => {}} />);
    expect(screen.getByRole("tab", { name: "Extensions" })).toBeTruthy();
    expect(screen.queryByRole("tab", { name: "Overlays" })).toBeNull();
    expect(screen.queryByText("账号与实例")).toBeNull();
  });

  it("keeps minimal mode focused on game and application settings", () => {
    render(<SettingsNavigation activeTab="accounts" installedModules={["automation", "pet"]}
      config={{ feature_profile: "minimal", feature_profile_prompt_revision: 1 } as GlobalConfig} onSelect={() => {}} />);
    expect(screen.getByRole("tab", { name: "Mod 管理" })).toBeTruthy();
    expect(screen.queryByRole("tab", { name: "扩展功能" })).toBeNull();
    expect(screen.queryByRole("tab", { name: "桌宠" })).toBeNull();
    expect(screen.queryByRole("tab", { name: "后台任务" })).toBeNull();
  });

  it("shows meaningful observed errors without claiming configured tools are healthy", () => {
    render(<SettingsNavigation activeTab="pet" installedModules={["pet"]} onSelect={() => {}}
      config={{ enable_bongo_cat: true } as GlobalConfig}
      capabilityStatus={{ revision: 4, capabilities: [{ id: "desktop-pet", requested_enabled: true, state: "failed", reason_code: "window-unavailable" }] }} />);
    expect(screen.getByRole("tab", { name: "桌宠 · 异常" })).toBeTruthy();
  });

  it("does not expose stale errors when runtime synchronization is unavailable", () => {
    render(<SettingsNavigation activeTab="pet" installedModules={["pet"]} onSelect={() => {}} capabilityStatusUnavailable
      capabilityStatus={{ revision: 4, capabilities: [{ id: "desktop-pet", requested_enabled: true, state: "failed", reason_code: "window-unavailable" }] }} />);
    expect(screen.getByRole("tab", { name: "桌宠" })).toBeTruthy();
    expect(screen.queryByText("异常")).toBeNull();
  });
});
