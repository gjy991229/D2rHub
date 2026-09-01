import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type ComponentProps, useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AudioModSetupState, GlobalConfig } from "../../../store/types";
import { AutomationPanel } from "./AutomationPanel";

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
  reason_code: "ready",
  message: "Ready",
  installed_mods: [],
  running_pid: null,
  session_verified: false,
  active_session_ready: null,
  active_session_update_required: null,
  restart_required: false,
};

const baseConfig = {
  app_language: "zh-CN",
  rune_audio_enabled: false,
  rune_audio_tracked_categories: [],
  rune_audio_tracked_charm_codes: [],
} as unknown as GlobalConfig;

const account = { id: "one", display_name: "Leader", initialized: true } as never;
const trackingTarget = { valid: true, account } as never;

function baseProps(
  overrides: Partial<ComponentProps<typeof AutomationPanel>> = {},
): ComponentProps<typeof AutomationPanel> {
  return {
    config: baseConfig,
    updateConfig: vi.fn(),
    persistConfig: vi.fn(async () => undefined),
    initializedTrackingAccounts: [account],
    trackingTarget,
    audioStatus: null,
    audioModState: null,
    audioModStateLoading: false,
    audioSetupOpen: true,
    onOpenAudioSetup: vi.fn(),
    onCloseAudioSetup: vi.fn(),
    audioSetupMode: "original",
    setAudioSetupMode: vi.fn(),
    audioSetupSource: "",
    setAudioSetupSource: vi.fn(),
    audioSetupName: "D2rHubTools",
    setAudioSetupName: vi.fn(),
    includeAudioTelemetry: true,
    setIncludeAudioTelemetry: vi.fn(),
    includeRoomTools: true,
    setIncludeRoomTools: vi.fn(),
    audioPreparing: false,
    audioPrepareProgress: null,
    normalizedAudioSetupName: "D2rHubTools",
    isAudioModUpgrade: false,
    isAudioModFeatureManagement: false,
    audioSetupNameError: "",
    showAudioSetupNameError: false,
    hasInitializedAudioAccount: true,
    hasAudioTarget: true,
    hasReadyAudioMod: false,
    isAudioEnableRequested: false,
    isAudioRecognitionActive: false,
    audioPrepareBlockedReason: "",
    onAudioTargetChange: vi.fn(async () => undefined),
    onAudioToggle: vi.fn(async () => undefined),
    onPrepareAudioMod: vi.fn(async () => undefined),
    onToggleDiagnosticRecording: vi.fn(async () => undefined),
    onClose: vi.fn(),
    onInitializeAccount: vi.fn(),
    ...overrides,
  };
}

afterEach(cleanup);

describe("AutomationPanel Mod feature groups", () => {
  it("starts new preparation with both feature groups selected and blocks an empty selection", async () => {
    const user = userEvent.setup();

    function Harness() {
      const [includeAudioTelemetry, setIncludeAudioTelemetry] = useState(true);
      const [includeRoomTools, setIncludeRoomTools] = useState(true);
      const blockedReason = includeAudioTelemetry || includeRoomTools
        ? ""
        : "请至少选择一个 Mod 功能";
      return (
        <AutomationPanel
          {...baseProps({
            includeAudioTelemetry,
            setIncludeAudioTelemetry,
            includeRoomTools,
            setIncludeRoomTools,
            audioPrepareBlockedReason: blockedReason,
          })}
        />
      );
    }

    render(<Harness />);

    const audio = screen.getByRole("checkbox", { name: /声纹识别/ }) as HTMLInputElement;
    const rooms = screen.getByRole("checkbox", { name: /局内房间工具/ }) as HTMLInputElement;
    expect(audio.checked).toBe(true);
    expect(rooms.checked).toBe(true);

    await user.click(audio);
    await user.click(rooms);

    expect(screen.getByText(/请至少选择一个 Mod 功能/)).toBeTruthy();
    expect((screen.getByRole("button", { name: "完成上方配置后即可准备" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("opens a ready Mod as additive feature management and preserves installed groups", async () => {
    const user = userEvent.setup();

    function Harness() {
      const [open, setOpen] = useState(false);
      const [includeAudioTelemetry, setIncludeAudioTelemetry] = useState(true);
      const [includeRoomTools, setIncludeRoomTools] = useState(true);
      return (
        <AutomationPanel
          {...baseProps({
            audioModState: readyAudioOnlyMod,
            audioSetupOpen: open,
            onOpenAudioSetup: () => setOpen(true),
            onCloseAudioSetup: () => setOpen(false),
            includeAudioTelemetry,
            setIncludeAudioTelemetry,
            includeRoomTools,
            setIncludeRoomTools,
            isAudioModUpgrade: true,
            isAudioModFeatureManagement: true,
            hasReadyAudioMod: true,
          })}
        />
      );
    }

    render(<Harness />);
    await user.click(screen.getByRole("button", { name: "管理功能" }));

    expect(screen.getByText("管理 Mod 功能")).toBeTruthy();
    const installedAudio = screen.getByRole("checkbox", { name: /声纹识别.*已安装/ }) as HTMLInputElement;
    expect(installedAudio.checked).toBe(true);
    expect(installedAudio.disabled).toBe(true);
    expect((screen.getByRole("checkbox", { name: /局内房间工具/ }) as HTMLInputElement).disabled).toBe(false);
    expect(screen.getByText("-mod D2rHubTools -txt -assettestmode 1")).toBeTruthy();
    expect(screen.getByRole("button", { name: "增补所选功能" })).toBeTruthy();
  });

  it("uses English copy for feature management without changing feature IDs", () => {
    render(<AutomationPanel {...baseProps({
      config: { ...baseConfig, app_language: "en-US" },
    })} />);

    expect(screen.getAllByText("Mod features").length).toBeGreaterThan(0);
    expect(screen.getByRole("checkbox", { name: /Audio recognition/ })).toBeTruthy();
    expect(screen.getByRole("checkbox", { name: /In-game room tools/ })).toBeTruthy();
  });
});
