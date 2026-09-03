import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AccountMeta } from "../../store/types";

const platform = vi.hoisted(() => ({
  invokeCommand: vi.fn(),
  emitEvent: vi.fn(async () => undefined),
  listenEvent: vi.fn(async () => () => undefined),
}));

vi.mock("../../platform/tauri", () => platform);
vi.mock("../../components/ui/Toast", () => ({ showToast: vi.fn() }));

import { useAccountSettingsController } from "./useAccountSettingsController";

const accounts = [{
  id: "one",
  display_name: "Main",
  initialized: true,
  window_x: 12,
  window_y: 32,
}] as AccountMeta[];

beforeEach(() => {
  vi.clearAllMocks();
  platform.invokeCommand.mockImplementation(async (command: string) => {
    if (command === "get_account_settings") return { "Sound Volume": 60 };
    return null;
  });
});

describe("account settings controller", () => {
  it("loads the selected account draft and game settings", async () => {
    const { result } = renderHook(() => useAccountSettingsController({
      accounts,
      loadAccounts: vi.fn(async () => undefined),
      renameAccount: vi.fn(async () => true),
    }));

    act(() => result.current.setSelectedAccountId("one"));

    await waitFor(() => expect(result.current.gameSettings).toEqual({ "Sound Volume": 60 }));
    expect(result.current.accountNicknameDraft).toBe("Main");
    expect(result.current.accountWinXDraft).toBe(12);
    expect(result.current.accountWinYDraft).toBe(32);
    expect(result.current.accountHasChanges).toBe(false);
  });

  it("persists changed game settings through the existing account command", async () => {
    const loadAccounts = vi.fn(async () => undefined);
    const { result } = renderHook(() => useAccountSettingsController({
      accounts,
      loadAccounts,
      renameAccount: vi.fn(async () => true),
    }));
    act(() => result.current.setSelectedAccountId("one"));
    await waitFor(() => expect(result.current.gameSettingsLoading).toBe(false));

    act(() => result.current.updateGameSetting("Sound Volume", 80));
    expect(result.current.accountHasChanges).toBe(true);

    await act(async () => {
      expect(await result.current.saveAccount(true)).toBe(true);
    });

    expect(platform.invokeCommand).toHaveBeenCalledWith("save_account_settings", {
      accountId: "one",
      settings: { "Sound Volume": 80 },
    });
    expect(platform.emitEvent).toHaveBeenCalledWith("account-settings-updated", { accountId: "one" });
    expect(loadAccounts).toHaveBeenCalled();
  });
});
