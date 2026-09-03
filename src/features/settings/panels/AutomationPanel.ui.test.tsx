import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type ComponentProps } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AudioModSetupState, GlobalConfig, ModCapsulePool } from "../../../store/types";
import type { ModCapsuleController } from "../../modCapsules/useModCapsulePool";
import { ModProcessingPanel } from "./ModProcessingPanel";

const readyAudioOnlyMod: AudioModSetupState = {
  account_id: "one",
  account_name: "Leader",
  current_mod_name: "D2rHubTools",
  launch_arguments: "-mod D2rHubTools -txt -assettestmode 1",
  has_txt: true,
  ready: true,
  update_required: false,
  recipe_version: 22,
  required_recipe_version: 22,
  build_mode: "minimal",
  source_mod_name: null,
  feature_groups: ["audio_telemetry"],
  auto_exit_on_death_enabled: false,
  reason_code: "ready",
  message: "Ready",
  installed_mods: [],
  running_pid: null,
  session_verified: false,
  active_session_ready: null,
  active_session_update_required: null,
  restart_required: false,
};

const sourceState: AudioModSetupState = {
  ...readyAudioOnlyMod,
  current_mod_name: null,
  ready: false,
  build_mode: null,
  feature_groups: [],
  installed_mods: [{
    name: "MyExistingMod",
    audio_ready: true,
    update_required: false,
    source_eligible: true,
    feature_groups: ["audio_telemetry", "in_game_room_tools"],
    audio_reusable: true,
    auto_exit_on_death_enabled: false,
  }],
};

const baseConfig = { app_language: "zh-CN" } as unknown as GlobalConfig;
const account = { id: "one", display_name: "Leader", initialized: true } as never;
const trackingTarget = { valid: true, account } as never;

function baseProps(
  overrides: Partial<ComponentProps<typeof ModProcessingPanel>> = {},
): ComponentProps<typeof ModProcessingPanel> {
  return {
    config: baseConfig,
    initializedAccounts: [account],
    trackingTarget,
    audioModState: sourceState,
    audioModStateLoading: false,
    audioModScannedAt: Date.now(),
    purpose: "recognition",
    audioSetupMode: "existing",
    setAudioSetupMode: vi.fn(),
    audioSetupSource: "MyExistingMod",
    setAudioSetupSource: vi.fn(),
    audioSetupName: "D2rHubTools",
    setAudioSetupName: vi.fn(),
    includeAudioTelemetry: false,
    setIncludeAudioTelemetry: vi.fn(),
    includeRoomTools: false,
    setIncludeRoomTools: vi.fn(),
    includeAutoExitOnDeath: false,
    setIncludeAutoExitOnDeath: vi.fn(),
    audioPreparing: false,
    audioPrepareProgress: null,
    isAudioModUpgrade: false,
    isAudioModFeatureManagement: false,
    audioSetupNameError: "",
    showAudioSetupNameError: false,
    audioPrepareBlockedReason: "",
    onTargetChange: vi.fn(async () => undefined),
    onPrepare: vi.fn(async () => undefined),
    onRefresh: vi.fn(async () => undefined),
    onBackToRecognition: vi.fn(),
    ...overrides,
  };
}

afterEach(cleanup);

describe("ModProcessingPanel feature inheritance", () => {
  it("opens the exact Mod selected from management instead of the target account's previous Mod", async () => {
    const user = userEvent.setup();
    const catalogPool: ModCapsulePool = {
      generation: 1,
      scanned_at: "2026-09-02T00:00:00+08:00",
      capsules: [{
        id: "scan:cn:mdk", edition: "CN", name: "MDK", origin: "scanned",
        launch_arguments: "-mod MDK -txt -assettestmode 1",
        default_launch_arguments: "-mod MDK -txt -assettestmode 1",
        feature_groups: [], processed: false, source_eligible: true, update_required: false,
        ready: true, deletable: false, assigned_account_ids: [],
      }],
      accounts: [{ account_id: "one", account_name: "Leader", edition: "CN", selected_capsule_id: null, legacy_mod_arguments: "", issue: null }],
    };
    const assign = vi.fn(async () => catalogPool);
    const catalog = {
      pool: catalogPool, loading: false, assigningAccountId: null, error: null,
      refresh: vi.fn(async () => catalogPool), scan: vi.fn(async () => catalogPool),
      add: vi.fn(async () => catalogPool), update: vi.fn(async () => catalogPool),
      remove: vi.fn(async () => catalogPool),
      setAutoExitOnDeathEnabled: vi.fn(async () => catalogPool), assign,
    } as ModCapsuleController;
    const onTargetChange = vi.fn(async () => undefined);
    const setAudioSetupSource = vi.fn();
    render(<ModProcessingPanel {...baseProps({ purpose: "manage", modCatalog: catalog,
      modCapsulePool: catalogPool, onTargetChange, setAudioSetupSource })} />);

    await user.click(screen.getByRole("button", { name: "加工" }));
    await waitFor(() => expect(assign).toHaveBeenCalledWith("one", "scan:cn:mdk"));
    expect(onTargetChange).toHaveBeenCalledWith("one");
    expect(setAudioSetupSource).toHaveBeenCalledWith("MDK");
  });

  it("locks recognition when processing was opened to enable recognition", () => {
    render(<ModProcessingPanel {...baseProps({
      audioSetupMode: "original",
      audioSetupSource: "",
      audioModState: { ...sourceState, installed_mods: [] },
    })} />);

    const audio = screen.getByRole("checkbox", { name: /声纹识别/ }) as HTMLInputElement;
    const rooms = screen.getByRole("checkbox", { name: /局内房间工具/ }) as HTMLInputElement;
    expect(audio.checked).toBe(true);
    expect(audio.disabled).toBe(true);
    expect(screen.getByText("本次目标 · 必选")).toBeTruthy();
    expect(rooms.disabled).toBe(false);
    const deathExit = screen.getByRole("checkbox", { name: /死亡后自动退房/ }) as HTMLInputElement;
    expect(deathExit.checked).toBe(false);
    expect(deathExit.disabled).toBe(false);
  });

  it("locks every module already present in the selected source Mod", () => {
    render(<ModProcessingPanel {...baseProps()} />);

    const audio = screen.getByRole("checkbox", { name: /声纹识别/ }) as HTMLInputElement;
    const rooms = screen.getByRole("checkbox", { name: /局内房间工具/ }) as HTMLInputElement;
    expect(audio.checked).toBe(true);
    expect(audio.disabled).toBe(true);
    expect(rooms.checked).toBe(true);
    expect(rooms.disabled).toBe(true);
    expect(screen.getAllByText("源 Mod 已有").length).toBe(2);
  });

  it("locks room tools when opened from automatic-room prerequisites", () => {
    render(<ModProcessingPanel {...baseProps({
      purpose: "room-tools",
      audioSetupMode: "original",
      audioSetupSource: "",
      audioModState: { ...sourceState, installed_mods: [] },
    })} />);

    const rooms = screen.getByRole("checkbox", { name: /局内房间工具/ }) as HTMLInputElement;
    expect(rooms.checked).toBe(true);
    expect(rooms.disabled).toBe(true);
    expect(screen.getByText("自动跟房必选")).toBeTruthy();
  });

  it("preserves installed modules while allowing additive management", () => {
    render(<ModProcessingPanel {...baseProps({
      purpose: "manage",
      audioModState: readyAudioOnlyMod,
      audioSetupMode: "original",
      audioSetupSource: "",
      isAudioModUpgrade: true,
      isAudioModFeatureManagement: true,
      includeRoomTools: true,
    })} />);

    const installedAudio = screen.getByRole("checkbox", { name: /声纹识别/ }) as HTMLInputElement;
    expect(installedAudio.checked).toBe(true);
    expect(installedAudio.disabled).toBe(true);
    expect((screen.getByRole("checkbox", { name: /局内房间工具/ }) as HTMLInputElement).disabled).toBe(false);
    expect(screen.queryByText("-mod D2rHubTools -txt -assettestmode 1")).toBeNull();
    expect(screen.getByRole("button", { name: /增补所选模块/ })).toBeTruthy();
  });

  it("keeps activation controls out of the processing flow", () => {
    render(<ModProcessingPanel {...baseProps({
      purpose: "manage",
      audioModState: {
        ...readyAudioOnlyMod,
        feature_groups: ["audio_telemetry", "auto_exit_on_death"],
        auto_exit_on_death_enabled: true,
      },
      audioSetupMode: "original",
      audioSetupSource: "",
      isAudioModUpgrade: true,
      isAudioModFeatureManagement: true,
    })} />);

    const installed = screen.getByRole("checkbox", { name: /死亡后自动退房/ }) as HTMLInputElement;
    expect(installed.checked).toBe(true);
    expect(installed.disabled).toBe(true);
    expect(screen.queryByRole("switch", { name: /死亡后自动退房/ })).toBeNull();
  });

  it("uses English copy without changing feature semantics", () => {
    render(<ModProcessingPanel {...baseProps({
      config: { ...baseConfig, app_language: "en-US" },
      audioSetupMode: "original",
      audioSetupSource: "",
      audioModState: { ...sourceState, installed_mods: [] },
    })} />);

    expect(screen.getByText("Feature modules")).toBeTruthy();
    expect(screen.getByRole("checkbox", { name: /Audio recognition/ })).toBeTruthy();
    expect(screen.getByRole("checkbox", { name: /In-game room tools/ })).toBeTruthy();
    expect(screen.getByRole("checkbox", { name: /Auto-exit after death/ })).toBeTruthy();
  });
});
