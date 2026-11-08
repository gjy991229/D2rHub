import type { ComponentProps } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { AccountMeta, GlobalConfig } from "../../../store/types";
import { useGlobalConfig } from "../../../store/globalConfig";
import { AccountsPanel } from "./AccountsPanel";

afterEach(() => { cleanup(); useGlobalConfig.setState({ config: null }); });

function props(overrides: Partial<ComponentProps<typeof AccountsPanel>> = {}): ComponentProps<typeof AccountsPanel> {
  const account = { id: "one", display_name: "Main", initialized: true } as AccountMeta;
  return {
    accounts: [account], selectedAccountId: "one", selectedAccount: account, setSelectedAccountId: vi.fn(),
    accountHasChanges: false, saveAccount: vi.fn(async () => true), toggleCustomizedSettings: vi.fn(async () => {}),
    accountRegionLabel: () => "国服", gameSettingsTab: "launch", setGameSettingsTab: vi.fn(),
    snapshotSystemSettings: vi.fn(async () => {}), accountNicknameDraft: "Main", setAccountNicknameDraft: vi.fn(),
    onOpenModManager: vi.fn(), accountWinXDraft: null, setAccountWinXDraft: vi.fn(), accountWinYDraft: null, setAccountWinYDraft: vi.fn(),
    gameSettings: {}, gameSettingsLoading: false, gameSettingsLoadError: null, updateGameSetting: vi.fn(),
    loadGameSettings: vi.fn(async () => {}), ...overrides,
  };
}

it("keeps account metadata usable while explaining unavailable game settings", () => {
  const callbacks = props({ gameSettingsLoading: true });
  const view = render(<AccountsPanel {...callbacks} />);
  const fps = screen.getByRole("spinbutton", { name: "FPS" });
  expect((fps.closest("fieldset") as HTMLFieldSetElement).disabled).toBe(true);
  expect((screen.getByRole("textbox", { name: "昵称" }) as HTMLInputElement).disabled).toBe(false);
  expect(screen.getByRole("status").textContent).toContain("正在读取游戏配置");
  view.rerender(<AccountsPanel {...callbacks} gameSettingsLoading={false} gameSettingsLoadError="Settings.json 不存在" />);
  expect(screen.getByText("尚未找到画质配置")).toBeTruthy();
  expect((fps.closest("fieldset") as HTMLFieldSetElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "重新读取" }));
  expect(callbacks.loadGameSettings).toHaveBeenCalledExactlyOnceWith("one");
});


it("renders English account actions, Mod details, and position capture without DOM translation", () => {
  useGlobalConfig.setState({ config: { app_language: "en-US" } as GlobalConfig });
  render(<AccountsPanel {...props()} />);
  expect(screen.getByText("Current Mod")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Manage Mods" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Save current position" })).toBeTruthy();
  expect(screen.getByText("Saved as a new account position preset")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Capture system settings" })).toBeTruthy();
  expect(screen.getByRole("textbox", { name: "Nickname" })).toBeTruthy();
  expect(screen.getByRole("tab", { name: "Launch" })).toBeTruthy();
});
