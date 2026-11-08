import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useGlobalConfig } from "../../store/globalConfig";
import type { AccountMeta, AudioModSetupState, GlobalConfig } from "../../store/types";
import { invokeCommand } from "../../platform/tauri";
import type { GlobalConfigPatch } from "../../utils/globalConfigPatch";
import { useGlobalSettingsDraft } from "./useGlobalSettingsDraft";
import { useAppearanceSettingsController } from "./useAppearanceSettingsController";
import { useAudioModuleController } from "./useAudioModuleController";

vi.mock("../../platform/tauri", () => ({ invokeCommand: vi.fn(), listenEvent: vi.fn() }));

const initial = {
  theme: "light", font_scale: "default", app_language: "zh-CN", main_opacity: 95,
  agent_delay_secs: 2, enable_tz_overlay: true, launch_groups: [], rune_audio_tracked_categories: [],
  cn_game_path: "C:\\Games\\D2R", global_game_path: "",
} as unknown as GlobalConfig;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function mount(persistPatch = vi.fn(async (patch: GlobalConfigPatch) => {
  const saved = { ...useGlobalConfig.getState().config!, ...patch };
  useGlobalConfig.setState({ config: saved });
  return saved;
})) {
  const hook = renderHook(({ open }) => {
    const committedConfig = useGlobalConfig(state => state.config);
    return useGlobalSettingsDraft({
      open, committedConfig, persistPatch,
      readCommitted: () => useGlobalConfig.getState().config,
    });
  }, { initialProps: { open: true } });
  return { ...hook, persistPatch };
}

beforeEach(() => useGlobalConfig.setState({ config: structuredClone(initial) }));
afterEach(cleanup);

describe("isolated settings edit session", () => {
  it("keeps nested edits out of the committed global store", () => {
    const { result } = mount();
    act(() => result.current.updateConfig(config => {
      config.theme = "onyx";
      config.rune_audio_tracked_categories.push("runes");
    }));
    expect(result.current.config?.theme).toBe("onyx");
    expect(result.current.config?.rune_audio_tracked_categories).toEqual(["runes"]);
    expect(useGlobalConfig.getState().config).toEqual(initial);
    expect(result.current.hasChanges).toBe(true);
  });

  it("rebases backend events without erasing dirty fields or resaving unrelated fields", async () => {
    const { result, persistPatch } = mount();
    act(() => result.current.updateConfig(config => { config.agent_delay_secs = 4; }));
    act(() => useGlobalConfig.setState({ config: {
      ...initial, theme: "forest", enable_tz_overlay: false,
      launch_groups: [{ id: "remote", name: "Remote", account_ids: [] }],
    } }));
    expect(result.current.config?.agent_delay_secs).toBe(4);
    expect(result.current.config?.theme).toBe("forest");
    expect(result.current.config?.launch_groups[0].id).toBe("remote");
    await act(async () => { await result.current.persistDraft(); });
    expect(persistPatch).toHaveBeenCalledWith({ agent_delay_secs: 4 });
    expect(result.current.hasChanges).toBe(false);
    expect(useGlobalConfig.getState().config?.theme).toBe("forest");
  });

  it("retains a failed save and incorporates backend changes that arrived during it", async () => {
    const request = deferred<GlobalConfig>();
    const persist = vi.fn((_patch: GlobalConfigPatch) => request.promise);
    const { result } = mount(persist);
    act(() => result.current.updateConfig(config => { config.theme = "onyx"; }));
    let saving!: Promise<GlobalConfig>;
    act(() => { saving = result.current.persistDraft(); });
    act(() => useGlobalConfig.setState({ config: { ...initial, agent_delay_secs: 7 } }));
    await act(async () => {
      request.reject(new Error("disk full"));
      await expect(saving).rejects.toThrow("disk full");
    });
    expect(result.current.config?.theme).toBe("onyx");
    expect(result.current.config?.agent_delay_secs).toBe(7);
    expect(result.current.hasChanges).toBe(true);
    expect(useGlobalConfig.getState().config?.theme).toBe("light");
  });

  it("keeps a newer edit when an earlier save completes", async () => {
    const request = deferred<GlobalConfig>();
    const { result } = mount(vi.fn((_patch: GlobalConfigPatch) => request.promise));
    act(() => result.current.updateConfig(config => { config.theme = "onyx"; }));
    let saving!: Promise<GlobalConfig>;
    act(() => { saving = result.current.persistDraft(); });
    act(() => result.current.updateConfig(config => { config.theme = "forest"; }));
    await act(async () => {
      const saved = { ...initial, theme: "onyx" };
      useGlobalConfig.setState({ config: saved });
      request.resolve(saved);
      await saving;
    });
    expect(result.current.config?.theme).toBe("forest");
    expect(result.current.hasChanges).toBe(true);
    expect(useGlobalConfig.getState().config?.theme).toBe("onyx");
  });

  it("preserves a user reverting to the old value while a save is pending", async () => {
    const request = deferred<GlobalConfig>();
    const { result } = mount(vi.fn((_patch: GlobalConfigPatch) => request.promise));
    act(() => result.current.updateConfig(config => { config.theme = "onyx"; }));
    let saving!: Promise<GlobalConfig>;
    act(() => { saving = result.current.persistDraft(); });
    act(() => result.current.updateConfig(config => { config.theme = "light"; }));
    await act(async () => {
      const saved = { ...initial, theme: "onyx" };
      useGlobalConfig.setState({ config: saved });
      request.resolve(saved);
      await saving;
    });
    expect(result.current.config?.theme).toBe("light");
    expect(result.current.hasChanges).toBe(true);
  });

  it("extracts only an async caller's intended fields from its older rendered snapshot", async () => {
    const { result, persistPatch } = mount();
    const persistFromOldRender = result.current.persistDraft;
    const oldDraft = result.current.config!;
    act(() => result.current.updateConfig(config => { config.agent_delay_secs = 5; }));
    act(() => useGlobalConfig.setState({ config: { ...initial, theme: "forest" } }));
    await act(async () => {
      await persistFromOldRender({ ...oldDraft, enable_tz_overlay: false });
    });
    expect(persistPatch).toHaveBeenCalledWith({ agent_delay_secs: 5, enable_tz_overlay: false });
    expect(result.current.config?.theme).toBe("forest");
    expect(result.current.hasChanges).toBe(false);
  });

  it("accepts a newer committed event instead of a delayed older save response", async () => {
    const request = deferred<GlobalConfig>();
    const { result } = mount(vi.fn((_patch: GlobalConfigPatch) => request.promise));
    act(() => result.current.updateConfig(config => { config.theme = "onyx"; }));
    let saving!: Promise<GlobalConfig>;
    act(() => { saving = result.current.persistDraft(); });
    await act(async () => {
      useGlobalConfig.setState({ config: { ...initial, theme: "forest" } });
      request.resolve({ ...initial, theme: "onyx" });
      await saving;
    });
    expect(result.current.config?.theme).toBe("forest");
    expect(result.current.hasChanges).toBe(false);
  });

  it("starts a fresh edit session when reopened and loads late configuration", () => {
    useGlobalConfig.setState({ config: null });
    const { result, rerender } = mount();
    expect(result.current.config).toBeNull();
    act(() => useGlobalConfig.setState({ config: initial }));
    act(() => result.current.updateConfig(config => { config.theme = "onyx"; }));
    rerender({ open: false });
    rerender({ open: true });
    expect(result.current.config?.theme).toBe("light");
    expect(result.current.hasChanges).toBe(false);
  });
});

it("appearance failures leave the selected draft available without publishing an unsaved theme", async () => {
  const persist = vi.fn(async (_patch: GlobalConfigPatch): Promise<GlobalConfig> => { throw new Error("disk full"); });
  const { result } = renderHook(() => {
    const committedConfig = useGlobalConfig(state => state.config);
    const session = useGlobalSettingsDraft({
      open: true, committedConfig, persistPatch: persist,
      readCommitted: () => useGlobalConfig.getState().config,
    });
    const appearance = useAppearanceSettingsController({
      open: true, config: session.config, committedConfig, updateConfig: session.updateConfig,
      persistConfig: async candidate => session.persistDraft(candidate).catch(() => null),
    });
    return appearance;
  });
  act(() => result.current.setDraft(draft => ({ ...draft!, theme: "onyx" })));
  expect(useGlobalConfig.getState().config?.theme).toBe("light");
  await act(async () => { expect(await result.current.apply()).toBe(false); });
  expect(result.current.draft?.theme).toBe("onyx");
  expect(result.current.hasChanges).toBe(true);
  expect(result.current.applying).toBe(false);
  expect(useGlobalConfig.getState().config?.theme).toBe("light");
});

it("keeps recognition enabled while changing its listening target", async () => {
  useGlobalConfig.setState({ config: { ...initial, rune_audio_enabled: true, rune_audio_target_account: "main" } });
  const scan = deferred<AudioModSetupState>();
  vi.mocked(invokeCommand).mockReturnValue(scan.promise as never);
  const persist = vi.fn(async (patch: GlobalConfigPatch) => {
    const saved = { ...useGlobalConfig.getState().config!, ...patch };
    useGlobalConfig.setState({ config: saved });
    return saved;
  });
  const { result } = renderHook(() => {
    const committedConfig = useGlobalConfig(state => state.config);
    const session = useGlobalSettingsDraft({
      open: true, committedConfig, persistPatch: persist,
      readCommitted: () => useGlobalConfig.getState().config,
    });
    const audio = useAudioModuleController({
      open: true, activeTab: "accounts", config: session.config,
      initializedAccounts: [{ id: "main", initialized: true }, { id: "other", initialized: true }] as AccountMeta[],
      trackingTargetId: session.config?.rune_audio_target_account ?? "",
      updateConfig: session.updateConfig, persistConfig: session.persistDraft,
      requestProcessing: vi.fn(),
    });
    return { session, audio };
  });
  let changing!: Promise<void>;
  act(() => { changing = result.current.audio.handleAudioTargetChange("other"); });
  expect(result.current.session.config?.rune_audio_enabled).toBe(true);
  expect(useGlobalConfig.getState().config?.rune_audio_enabled).toBe(true);
  await act(async () => {
    scan.resolve({ account_id: "other", ready: true, installed_mods: [], feature_groups: ["audio_telemetry"] } as unknown as AudioModSetupState);
    await changing;
  });
  expect(useGlobalConfig.getState().config?.rune_audio_target_account).toBe("other");
  expect(useGlobalConfig.getState().config?.rune_audio_enabled).toBe(true);
  expect(result.current.session.hasChanges).toBe(false);
});
