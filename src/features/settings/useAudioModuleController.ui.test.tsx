import { useState } from "react";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invokeCommand } from "../../platform/tauri";
import type { AccountMeta, AudioModSetupState, GlobalConfig } from "../../store/types";
import type { SettingsTabId } from "./settingsRegistry";
import { useAudioModuleController } from "./useAudioModuleController";
import { modState } from "../mods/workflow/testFixtures";

vi.mock("../../platform/tauri", () => ({ invokeCommand: vi.fn(), listenEvent: vi.fn(async () => () => {}) }));
const unreadyState = { ...modState, account_id: "main", installed_mods: [] };
function mount(tab: SettingsTabId, requestProcessing = vi.fn()) {
  return renderHook(({ activeTab }: { activeTab: SettingsTabId }) => useAudioModuleController({
    open: true, activeTab, config: { rune_audio_enabled: true } as GlobalConfig,
    initializedAccounts: [{ id: "main", initialized: true }] as AccountMeta[], trackingTargetId: "main",
    updateConfig: vi.fn(), persistConfig: vi.fn(async () => ({})), requestProcessing,
  }), { initialProps: { activeTab: tab } });
}
beforeEach(() => {
  vi.mocked(invokeCommand).mockReset().mockImplementation(async command => (command === "get_audio_mod_setup_state" ? unreadyState : null) as never);
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

describe("Recognition inspection and Mod workflow isolation", () => {
  it("freshly inspects an explicitly enabled account after its Mod was replaced", async () => {
    const requestProcessing = vi.fn();
    const persistConfig = vi.fn(async (next: GlobalConfig) => next);
    const { result } = renderHook(() => useAudioModuleController({
      open: true, activeTab: "automation", config: { rune_audio_enabled: false } as GlobalConfig,
      initializedAccounts: [{ id: "main", initialized: true }] as AccountMeta[], trackingTargetId: "main",
      updateConfig: vi.fn(), persistConfig, requestProcessing,
    }));
    await waitFor(() => expect(result.current.audioModState?.ready).toBe(false));
    const newlyAssigned = { ...unreadyState, ready: true, current_mod_name: "AudioReady", feature_groups: ["audio_telemetry"] };
    vi.mocked(invokeCommand).mockImplementation(async command => (command === "get_audio_mod_setup_state" ? newlyAssigned : null) as never);
    await act(() => result.current.handleAudioToggle(true));
    expect(persistConfig).toHaveBeenCalledWith(expect.objectContaining({ rune_audio_enabled: true, rune_audio_target_account: "main" }), true);
    expect(requestProcessing).not.toHaveBeenCalled();
    expect(vi.mocked(invokeCommand).mock.calls.filter(([command]) => command === "get_audio_mod_setup_state")).toHaveLength(1);
  });

  it("invalidates only a changed account's Mod and runtime inspection", async () => {
    const first = { id: "main", initialized: true, mod_args: "-mod Old", is_running: false } as AccountMeta;
    const { result, rerender } = renderHook(({ account }) => useAudioModuleController({
      open: true, activeTab: "automation", config: { rune_audio_enabled: true } as GlobalConfig,
      initializedAccounts: [account], trackingTargetId: account.id,
      updateConfig: vi.fn(), persistConfig: vi.fn(), requestProcessing: vi.fn(),
    }), { initialProps: { account: first } });
    await waitFor(() => expect(result.current.audioModState).toEqual(unreadyState));
    vi.mocked(invokeCommand).mockImplementation(async command => (command === "get_audio_mod_setup_state"
      ? { ...unreadyState, ready: true, current_mod_name: "New", running_pid: 42 } : null) as never);
    rerender({ account: { ...first, mod_args: "-mod New", is_running: true, running_pid: 42 } });
    await waitFor(() => expect(result.current.audioModState?.running_pid).toBe(42));
    expect(vi.mocked(invokeCommand).mock.calls.filter(([command]) => command === "get_audio_mod_setup_state")).toHaveLength(2);
  });

  it.each([
    { running: false, account_id: "main", active: false },
    { running: true, account_id: "another-account", active: false },
    { running: true, account_id: "main", active: true },
  ])("requires an observed running monitor for the selected account: $running / $account_id", async ({ running, account_id, active }) => {
    vi.mocked(invokeCommand).mockImplementation(async command => (command === "get_audio_mod_setup_state"
      ? { ...unreadyState, ready: true }
      : { running, account_id }) as never);
    const { result } = mount("automation");
    await waitFor(() => expect(result.current.audioStatus?.account_id).toBe(account_id));
    expect(result.current.isAudioRecognitionActive).toBe(active);
  });

  it("does not inspect or replace independent Mod management when recognition is inactive", async () => {
    const requestProcessing = vi.fn();
    const { result, rerender } = mount("mod-processing", requestProcessing);
    await act(async () => {});
    expect(result.current.audioModState).toBeNull();
    expect(invokeCommand).not.toHaveBeenCalled();
    rerender({ activeTab: "accounts" });
    rerender({ activeTab: "mod-processing" });
    expect(requestProcessing).not.toHaveBeenCalled();
  });

  it("reports missing recognition prerequisites without opening processing from a background scan", async () => {
    const requestProcessing = vi.fn();
    const { result } = mount("automation", requestProcessing);
    await waitFor(() => expect(result.current.audioModState).toEqual(unreadyState));
    expect(result.current.hasReadyAudioMod).toBe(false);
    expect(requestProcessing).not.toHaveBeenCalled();
    await act(() => result.current.handleAudioToggle(true));
    expect(requestProcessing).not.toHaveBeenCalled();
  });

  it("reuses a completed inspection when returning to recognition", async () => {
    const { result, rerender } = mount("automation");
    await waitFor(() => expect(result.current.audioModState).toEqual(unreadyState));
    rerender({ activeTab: "accounts" });
    rerender({ activeTab: "automation" });
    await waitFor(() => expect(result.current.audioModState).toEqual(unreadyState));
    expect(vi.mocked(invokeCommand).mock.calls.filter(([command]) => command === "get_audio_mod_setup_state")).toHaveLength(1);
  });

  it("ignores a recognition scan that finishes after leaving recognition settings", async () => {
    let finish!: (state: AudioModSetupState) => void;
    const pending = new Promise<AudioModSetupState>(resolve => { finish = resolve; });
    vi.mocked(invokeCommand).mockImplementation(async command => (command === "get_audio_mod_setup_state" ? pending : null) as never);
    const requestProcessing = vi.fn();
    const { result, rerender } = mount("automation", requestProcessing);
    rerender({ activeTab: "mod-processing" });
    await act(async () => { finish(unreadyState); await pending; });
    expect(result.current.audioModState).toBeNull();
    expect(requestProcessing).not.toHaveBeenCalled();
  });
});


describe("external recognition preserves account preferences", () => {
  const configured = { cn_game_path: "D:\\Game", global_game_path: "", rune_audio_target_account: "main", rune_audio_enabled: false,
    rune_audio_external_target: { edition: "CN", mod_name: "Audio" } } as GlobalConfig;
  function mountExternal(config = configured) {
    const persistConfig = vi.fn(async draft => draft);
    const requestProcessing = vi.fn();
    const hook = renderHook(() => {
      const [current, setConfig] = useState(config);
      return { config: current, ...useAudioModuleController({ open: true, activeTab: "automation", config: current,
        initializedAccounts: [], trackingTargetId: "", persistConfig, requestProcessing,
        updateConfig: update => setConfig(previous => { const next = structuredClone(previous); update(next); return next; }) }) };
    });
    return { ...hook, persistConfig, requestProcessing };
  }
  it("enables and disables external recognition without overwriting the saved account target", async () => {
    vi.mocked(invokeCommand).mockImplementation(async command => (command === "get_audio_mod_setup_state"
      ? { ...unreadyState, account_id: "", ready: true, feature_groups: ["audio_telemetry"] } : command === "get_external_audio_instances" ? [] : {}) as never);
    const { result, persistConfig } = mountExternal();
    await waitFor(() => expect(result.current.hasReadyAudioMod).toBe(true));
    await act(() => result.current.handleAudioToggle(true));
    expect(result.current.config.rune_audio_enabled).toBe(true);
    expect(persistConfig).toHaveBeenLastCalledWith(expect.objectContaining({ rune_audio_target_account: "main", rune_audio_external_target: configured.rune_audio_external_target, rune_audio_enabled: true }), true);
    await act(() => result.current.handleAudioToggle(false));
    expect(result.current.config.rune_audio_target_account).toBe("main");
    expect(result.current.config.rune_audio_external_target).toEqual(configured.rune_audio_external_target);
  });
  it("uses installation processing when the Mod is missing and ignores independent account receipts", async () => {
    vi.mocked(invokeCommand).mockImplementation(async command => (command === "get_audio_mod_setup_state"
      ? { ...unreadyState, account_id: "" } : command === "get_external_audio_instances" ? [] : {}) as never);
    const { result, requestProcessing, persistConfig } = mountExternal();
    await waitFor(() => expect(result.current.audioModState?.account_id).toBe(""));
    await act(() => result.current.handleAudioToggle(true));
    expect(requestProcessing).toHaveBeenCalledWith(expect.objectContaining({ origin: "recognition", edition: "CN", installationOnly: true }));
    await act(() => result.current.completeModProcessing({ origin: "library", accountId: "main", state: { ...unreadyState, ready: true } }));
    expect(persistConfig).not.toHaveBeenCalled();
    expect(result.current.config.rune_audio_external_target).toEqual(configured.rune_audio_external_target);
  });
  it("returns to account monitoring without changing its stored Mod configuration", async () => {
    vi.mocked(invokeCommand).mockImplementation(async (command, args) => (command === "get_audio_mod_setup_state"
      ? { ...unreadyState, account_id: (args as { accountId: string }).accountId, ready: true } : command === "get_external_audio_instances" ? [] : {}) as never);
    const { result } = mountExternal();
    await act(() => result.current.handleAudioTargetChange("main"));
    expect(result.current.config.rune_audio_external_target).toBeNull();
    expect(result.current.config.rune_audio_target_account).toBe("main");
    expect(invokeCommand).not.toHaveBeenCalledWith("get_audio_mod_setup_state", { accountId: "main" });
    expect(vi.mocked(invokeCommand).mock.calls.some(([command]) => command === "apply_audio_mod_to_account" || command === "assign_mod_capsule_to_account")).toBe(false);
  });
  it("only binds a discovered process after the user explicitly selects it", async () => {
    const game = { pid: 10, started_at: 30, mod_name: "AudioReady", ready: true, selected: false, message: "" };
    vi.mocked(invokeCommand).mockImplementation(async command => (command === "get_audio_mod_setup_state"
      ? { ...unreadyState, account_id: "", ready: true } : command === "get_external_audio_instances" ? [game] : {}) as never);
    const { result } = mountExternal();
    await waitFor(() => expect(result.current.externalInstances).toEqual([game]));
    expect(vi.mocked(invokeCommand).mock.calls.some(([command]) => command === "select_external_audio_instance")).toBe(false);
    await act(() => result.current.selectExternalInstance(game));
    expect(invokeCommand).toHaveBeenCalledWith("select_external_audio_instance", { identity: { pid: 10, started_at: 30 } });
    expect(result.current.externalInstances[0].selected).toBe(true);
  });
  it("ignores an older discovery result that completes after explicit process selection", async () => {
    vi.useFakeTimers();
    const game = { pid: 10, started_at: 30, mod_name: "AudioReady", ready: true, selected: false, message: "" };
    let finish!: (games: typeof game[]) => void;
    const pending = new Promise<typeof game[]>(resolve => { finish = resolve; });
    let reads = 0;
    vi.mocked(invokeCommand).mockImplementation(async command => (command === "get_external_audio_instances"
      ? (++reads === 1 ? [game] : pending) : command === "get_audio_mod_setup_state" ? { ...unreadyState, account_id: "", ready: true } : {}) as never);
    const { result } = mountExternal();
    await act(async () => {});
    expect(result.current.externalInstances).toEqual([game]);
    await act(async () => { vi.advanceTimersByTime(2500); });
    expect(reads).toBe(2);
    await act(() => result.current.selectExternalInstance(game));
    await act(async () => { finish([game]); await pending; });
    expect(result.current.externalInstances[0].selected).toBe(true);
  });
});
