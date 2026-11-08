import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invokeCommand } from "../../../platform/tauri";
import { useModWorkflow } from "./useModWorkflow";
import { modPool, modState, testAccount, testCatalog } from "./testFixtures";
import type { AccountMeta, AudioModSetupState } from "../../../store/types";

vi.mock("../../../platform/tauri", () => ({ invokeCommand: vi.fn(), listenEvent: vi.fn(async () => () => {}) }));
vi.mock("../../../components/ui/Toast", () => ({ showToast: vi.fn() }));
vi.mock("../../tasks/taskSync", () => ({ subscribeBeforeReadingTasks: vi.fn(async () => () => {}) }));
const otherAccount = { ...testAccount, id: "two", display_name: "Other" };
const globalAccount = { ...testAccount, id: "global", display_name: "Global" };
const pool = { ...modPool, accounts: [...modPool.accounts,
  { ...modPool.accounts[0], account_id: "two" }, { ...modPool.accounts[0], account_id: "global", edition: "Global" }] };
const ready = { ...modState, ready: true, current_mod_name: "New", feature_groups: ["audio_telemetry"] };

function mount(accounts: AccountMeta[] = [testAccount, otherAccount, globalAccount]) {
  const catalog = testCatalog(pool);
  const onNavigate = vi.fn();
  const onApplied = vi.fn(async () => {});
  const hook = renderHook(({ active, accounts: currentAccounts }) => useModWorkflow({ open: true, active, accounts: currentAccounts, catalog,
    language: "zh-CN", optionalFeaturesAvailable: true, onNavigate, onApplied }), { initialProps: { active: true, accounts } });
  return { ...hook, catalog, onNavigate, onApplied };
}
async function openDraft(result: ReturnType<typeof mount>["result"], accountId = "one") {
  act(() => result.current.actions.requestProcessing({ origin: "recognition", accountId }));
  await waitFor(() => expect(result.current.inspection.state?.account_id).toBe(accountId));
  act(() => {
    result.current.actions.changeRecipe({ kind: "create", source: null, name: "New" });
    result.current.actions.setProcessorReady(true);
  });
}
beforeEach(() => {
  vi.mocked(invokeCommand).mockReset().mockImplementation(async (command, args) => {
    if (command === "get_audio_mod_setup_state") return { ...modState, account_id: (args as { accountId: string }).accountId } as never;
    if (command === "prepare_audio_mod") return { mod_name: "New", feature_groups: [{ id: "audio_telemetry" }] } as never;
    return ready as never;
  });
});
afterEach(cleanup);

describe("Mod workflow", () => {
  it.each([true, false])("copies the displayed source scene-audio setting without editing the checkbox: %s", async enabled => {
    vi.mocked(invokeCommand).mockImplementation(async (command, args) => {
      if (command === "get_audio_mod_setup_state") return { ...modState, account_id: (args as { accountId: string }).accountId,
        installed_mods: [{ ...modState.installed_mods[0], scene_audio_enabled: enabled }] } as never;
      if (command === "prepare_audio_mod") return { mod_name: "New", feature_groups: [{ id: "audio_telemetry" }] } as never;
      return ready as never;
    });
    const { result } = mount();
    await openDraft(result);
    act(() => result.current.actions.changeRecipe({ kind: "create", source: "MyExistingMod", name: "New" }));
    expect(result.current.analysis?.selection.includeSceneAudio).toBe(enabled);
    await act(() => result.current.actions.prepare());
    expect(invokeCommand).toHaveBeenCalledWith("prepare_audio_mod", expect.objectContaining({ includeSceneAudio: enabled }));
  });
  it.each([true, false])("does not rebuild when scene audio is restored to its persisted value: %s", async enabled => {
    vi.mocked(invokeCommand).mockImplementation(async (command, args) => {
      if (command === "get_audio_mod_setup_state") return { ...modState, account_id: (args as { accountId: string }).accountId,
        installed_mods: [{ ...modState.installed_mods[0], scene_audio_enabled: enabled, source_mod_name: "MissingOriginal" }] } as never;
      return ready as never;
    });
    const { result } = mount();
    act(() => result.current.actions.requestProcessing({ origin: "recognition", accountId: "one", source: { name: "MyExistingMod", processed: true } }));
    await waitFor(() => expect(result.current.inspection.state?.account_id).toBe("one"));
    act(() => {
      result.current.actions.changeFeatures({ includeSceneAudio: !enabled });
      result.current.actions.setProcessorReady(true);
    });
    expect(result.current.analysis?.updating).toBe(true);
    expect(result.current.blockedReason).toContain("原始源 Mod");
    act(() => result.current.actions.changeFeatures({ includeSceneAudio: enabled }));
    expect(result.current.analysis?.updating).toBe(false);
    expect(result.current.blockedReason).toContain("已包含所选功能");
    act(() => result.current.actions.changeFeatures({ includePartyLoot: true }));
    expect(result.current.blockedReason).toBe("");
    await act(() => result.current.actions.prepare());
    const upgrade = vi.mocked(invokeCommand).mock.calls.find(([command]) => command === "upgrade_audio_mod");
    expect(upgrade).toBeDefined();
    expect(upgrade?.[1]).not.toHaveProperty("includeSceneAudio");
  });

  it("forwards an actual scene-audio change to the processor", async () => {
    const { result } = mount();
    await openDraft(result);
    act(() => result.current.actions.changeFeatures({ includeSceneAudio: false }));
    await act(() => result.current.actions.prepare());
    expect(invokeCommand).toHaveBeenCalledWith("prepare_audio_mod", expect.objectContaining({ includeAudioTelemetry: true, includeSceneAudio: false }));
  });

  it("forwards the disabled default-Hell processing preference", async () => {
    const { result } = mount();
    await openDraft(result);
    act(() => result.current.actions.changeFeatures({ includeRoomTools: true, roomToolsDefaultHell: false }));
    await act(() => result.current.actions.prepare());
    expect(invokeCommand).toHaveBeenCalledWith("prepare_audio_mod", expect.objectContaining({
      includeRoomTools: true, roomToolsDefaultHell: false,
    }));
  });
  it("forwards the source name and explicit replacement intent instead of the spare output name", async () => {
    const { result } = mount();
    await openDraft(result);
    act(() => result.current.actions.changeRecipe({ kind: "create", source: "MyExistingMod", name: "Unused", replaceSource: true }));
    act(() => result.current.actions.changeFeatures({ includePartyLoot: true }));
    await act(() => result.current.actions.prepare());
    expect(invokeCommand).toHaveBeenCalledWith("prepare_audio_mod", expect.objectContaining({
      modName: "MyExistingMod", sourceModName: "MyExistingMod", replaceSource: true, includePartyLoot: true,
    }));
  });
  it.each([
    { source: null, includeRoomTools: false },
    { source: "MyExistingMod", includeRoomTools: true },
  ])("requests only recognition by default while preserving source room tools: $source", async ({ source, includeRoomTools }) => {
    const { result } = mount();
    await openDraft(result);
    expect(result.current.draft?.features).toMatchObject({ includeAudioTelemetry: true, includeRoomTools: false });
    act(() => result.current.actions.changeRecipe({ kind: "create", source, name: "New" }));
    await act(() => result.current.actions.prepare());
    expect(invokeCommand).toHaveBeenCalledWith("prepare_audio_mod", expect.objectContaining({
      accountId: "one", modName: "New", sourceModName: source, includeAudioTelemetry: true, includeRoomTools,
    }));
  });

  it("does not apply a completed build when the account changes edition while the builder runs", async () => {
    const { result, rerender } = mount();
    await openDraft(result);
    let finish!: (value: unknown) => void;
    const build = new Promise(resolve => { finish = resolve; });
    vi.mocked(invokeCommand).mockImplementation(async (command, args) => command === "prepare_audio_mod"
      ? build as never : { ...modState, account_id: (args as { accountId: string }).accountId } as never);
    let preparing!: Promise<void>;
    act(() => { preparing = result.current.actions.prepare(); });
    rerender({ active: true, accounts: [{ ...testAccount, region: "Global" }] });
    await act(async () => { finish({ mod_name: "New" }); await preparing; });
    expect(vi.mocked(invokeCommand).mock.calls.some(([command]) => command === "apply_audio_mod_to_account")).toBe(false);
    expect(result.current.error).toContain("请重新选择账号");
    expect(result.current.prepared).toBe(true);
  });

  it.each(["removed", "uninitialized", "edition"] as const)("revalidates the live target before preparation when it is %s", async change => {
    const { result, rerender } = mount();
    await openDraft(result);
    rerender({ active: true, accounts: change === "removed" ? [] : [{ ...testAccount,
      initialized: change !== "uninitialized", region: change === "edition" ? "Global" : "CN" }] });
    vi.mocked(invokeCommand).mockClear();
    await act(() => result.current.actions.prepare());
    expect(vi.mocked(invokeCommand).mock.calls.some(([command]) => command === "prepare_audio_mod" || command === "apply_audio_mod_to_account" || command === "upgrade_audio_mod")).toBe(false);
    expect(result.current.error).toContain("请重新选择账号");
  });

  it("keeps a cancelled draft available for retry without presenting cancellation as a failure", async () => {
    const { result } = mount();
    await openDraft(result);
    vi.mocked(invokeCommand).mockRejectedValueOnce(new Error("Mod preparation cancelled"));
    await act(() => result.current.actions.prepare());
    expect(result.current.error).toBeNull();
    expect(result.current.notice).toBe("加工已取消，草稿已保留。");
    expect(result.current.draft?.recipe).toEqual({ kind: "create", source: null, name: "New" });
    expect(result.current.busy).toBe(false);
  });

  it("keeps the library passive and preserves an explicit room request through its cold scan", async () => {
    const { result, catalog, onNavigate } = mount();
    expect(invokeCommand).not.toHaveBeenCalled();
    act(() => result.current.actions.requestProcessing({ origin: "room-automation", accountId: "one" }));
    await waitFor(() => expect(result.current.inspection.state).toEqual(modState));
    expect(result.current.draft).toMatchObject({ origin: "room-automation", features: { includeRoomTools: true, includeAudioTelemetry: false } });
    expect(result.current.view).toBe("processing");
    expect(catalog.assign).not.toHaveBeenCalled();
    act(() => result.current.actions.back());
    expect(onNavigate).toHaveBeenLastCalledWith("room-automation");
  });

  it("never substitutes another account for a missing or uninitialized explicit target", async () => {
    const { result } = mount();
    act(() => result.current.actions.requestProcessing({ origin: "recognition", accountId: "deleted" }));
    expect(result.current.draft?.accountId).toBe("");
    expect(result.current.blockedReason).toContain("请选择");
    await act(() => result.current.actions.prepare());
    expect(invokeCommand).not.toHaveBeenCalled();
  });

  it("preserves source selection and typed drafts across library and inspection refreshes", async () => {
    const { result } = mount();
    await openDraft(result);
    const draft = result.current.draft;
    expect(result.current.draft).toEqual(draft);
    await act(() => result.current.actions.refresh());
    expect(result.current.draft).toEqual(draft);
    act(() => result.current.actions.openLibrary("Global"));
    expect(result.current.libraryEdition).toBe("Global");
    act(() => result.current.actions.resume());
    expect(result.current.draft).toEqual(draft);
  });

  it("restores each account draft and never prepares using another account's stale inspection", async () => {
    const { result } = mount();
    await openDraft(result);
    let finish!: (state: AudioModSetupState) => void;
    const pending = new Promise<AudioModSetupState>(resolve => { finish = resolve; });
    vi.mocked(invokeCommand).mockImplementation(async (command, args) => command === "get_audio_mod_setup_state"
      && (args as { accountId: string }).accountId === "global" ? pending : ready as never);
    act(() => result.current.actions.chooseTarget("global"));
    expect(result.current.inspection.state).toBeNull();
    expect(result.current.draft?.recipe).toEqual({ kind: "create", source: null, name: "" });
    await act(() => result.current.actions.prepare());
    expect(vi.mocked(invokeCommand).mock.calls.some(([command]) => command === "prepare_audio_mod")).toBe(false);
    act(() => result.current.actions.chooseTarget("one"));
    expect(result.current.draft?.recipe).toEqual({ kind: "create", source: null, name: "New" });
    await act(async () => { finish({ ...modState, account_id: "global" }); await pending; });
    expect(result.current.inspection.state?.account_id).toBe("one");
  });

  it("retries applying a successfully built Mod without rebuilding or switching accounts", async () => {
    const { result, onApplied, onNavigate } = mount();
    await openDraft(result);
    let applyCalls = 0;
    vi.mocked(invokeCommand).mockImplementation(async command => {
      if (command === "prepare_audio_mod") return { mod_name: "New" } as never;
      if (command === "apply_audio_mod_to_account" && applyCalls++ === 0) throw new Error("Game still running");
      return ready as never;
    });
    await act(() => result.current.actions.prepare());
    expect(result.current.error).toContain("Game still running");
    expect(result.current.prepared).toBe(true);
    expect(result.current.view).toBe("processing");
    await act(() => result.current.actions.prepare());
    expect(vi.mocked(invokeCommand).mock.calls.filter(([command]) => command === "prepare_audio_mod")).toHaveLength(1);
    expect(vi.mocked(invokeCommand).mock.calls.filter(([command]) => command === "apply_audio_mod_to_account")).toHaveLength(2);
    expect(onApplied).toHaveBeenCalledWith({ origin: "recognition", accountId: "one", state: ready });
    expect(onNavigate).toHaveBeenLastCalledWith("recognition");
    expect(result.current.draft).toBeNull();
  });

  it("recovers an inspection error with a rescan while keeping the form draft", async () => {
    vi.mocked(invokeCommand).mockRejectedValueOnce(new Error("Folder unavailable"));
    const { result } = mount();
    act(() => result.current.actions.requestProcessing({ origin: "recognition", accountId: "one" }));
    await waitFor(() => expect(result.current.inspection.error).toContain("Folder unavailable"));
    act(() => result.current.actions.changeRecipe({ kind: "create", source: null, name: "AfterRetry" }));
    await act(() => result.current.actions.refresh());
    expect(result.current.inspection.error).toBeNull();
    expect(result.current.draft?.recipe).toEqual({ kind: "create", source: null, name: "AfterRetry" });
  });

  it("retries completion without reapplying after the Mod was committed but preference persistence failed", async () => {
    const { result, onApplied } = mount();
    onApplied.mockRejectedValueOnce(new Error("Preferences unavailable"));
    await openDraft(result);
    await act(() => result.current.actions.prepare());
    expect(result.current.error).toContain("Preferences unavailable");
    expect(result.current.prepared).toBe(true);
    await act(() => result.current.actions.prepare());
    expect(onApplied).toHaveBeenCalledTimes(2);
    expect(vi.mocked(invokeCommand).mock.calls.filter(([command]) => command === "prepare_audio_mod")).toHaveLength(1);
    expect(vi.mocked(invokeCommand).mock.calls.filter(([command]) => command === "apply_audio_mod_to_account")).toHaveLength(1);
  });
});


it("drops a processing form that was only opened then left", async () => {
  const { result } = mount();
  act(() => result.current.actions.requestProcessing({ origin: "library", edition: "CN" }));
  await waitFor(() => expect(result.current.inspection.loading).toBe(false));
  act(() => result.current.actions.back());
  expect(result.current.view).toBe("library");
  expect(result.current.draft).toBeNull();
  expect(vi.mocked(invokeCommand).mock.calls.some(([name]) => name === "prepare_audio_mod")).toBe(false);
});
it("retains intentional edits as a draft and allows explicit discard without starting processing", async () => {
  const { result } = mount();
  act(() => result.current.actions.requestProcessing({ origin: "library", edition: "CN" }));
  act(() => result.current.actions.changeRecipe({ kind: "create", source: null, name: "My draft" }));
  act(() => result.current.actions.back());
  expect(result.current.draft?.recipe).toMatchObject({ name: "My draft" });
  act(() => result.current.actions.resume());
  expect(result.current.view).toBe("processing");
  act(() => result.current.actions.discardDraft());
  expect(result.current.draft).toBeNull();
  expect(result.current.view).toBe("library");
  expect(vi.mocked(invokeCommand).mock.calls.some(([name]) => name === "prepare_audio_mod")).toBe(false);
});


describe("installation-only Mod processing", () => {
  it("builds for a game installation without creating or configuring an account", async () => {
    vi.mocked(invokeCommand).mockImplementation(async (command, args) => {
      if (command === "get_audio_mod_setup_state") return { ...modState, account_id: "", ...(args && "modName" in args && args.modName === "New" ? { ...ready, account_id: "", launch_arguments: "-mod New -txt" } : {}) } as never;
      if (command === "prepare_audio_mod") return { mod_name: "New", feature_groups: [{ id: "audio_telemetry" }] } as never;
      return null as never;
    });
    const { result, onApplied } = mount([]);
    act(() => result.current.actions.requestProcessing({ origin: "recognition", edition: "CN", installationOnly: true }));
    await waitFor(() => expect(result.current.inspection.state?.account_id).toBe(""));
    act(() => { result.current.actions.changeRecipe({ kind: "create", source: null, name: "New" }); result.current.actions.setProcessorReady(true); });
    expect(result.current.blockedReason).toBe("");
    await act(() => result.current.actions.prepare());
    expect(invokeCommand).toHaveBeenCalledWith("prepare_audio_mod", expect.objectContaining({ accountId: "", edition: "CN", modName: "New" }));
    expect(onApplied).toHaveBeenCalledWith(expect.objectContaining({ accountId: "", installationEdition: "CN", state: expect.objectContaining({ launch_arguments: "-mod New -txt" }) }));
    expect(vi.mocked(invokeCommand).mock.calls.some(([command]) => command === "apply_audio_mod_to_account" || command === "create_account" || command === "assign_mod_capsule_to_account")).toBe(false);
  });
  it("also lets an empty-account Mod library use the selected installation", async () => {
    const { result } = mount([]);
    act(() => result.current.actions.requestProcessing({ origin: "library", edition: "Global" }));
    expect(result.current.draft).toMatchObject({ accountId: "", edition: "Global", installationOnly: true });
    await waitFor(() => expect(invokeCommand).toHaveBeenCalledWith("get_audio_mod_setup_state", { accountId: "", edition: "Global", modName: null }));
  });
});
