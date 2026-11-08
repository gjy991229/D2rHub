import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type ComponentProps } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AccountMeta, GlobalConfig } from "../../../store/types";
import { AutomationPanel } from "./AutomationPanel";
import { invokeCommand } from "../../../platform/tauri";
import { modState } from "../../mods/workflow/testFixtures";

vi.mock("../../../platform/tauri", () => ({
  invokeCommand: vi.fn(async command => command === "get_audio_mod_setup_state" ? { ...modState, account_id: "leader", installed_mods: [] } : ({ processor: { ready: true, update_available: false } })),
  listenEvent: vi.fn(async () => () => {}),
}));
vi.mock("../../../components/ui/Toast", () => ({ showToast: vi.fn() }));

const account = { id: "leader", display_name: "Leader", initialized: true } as AccountMeta;
const config = { app_language: "zh-CN", rune_audio_enabled: false } as GlobalConfig;

function panelProps(overrides: Partial<ComponentProps<typeof AutomationPanel>> = {}): ComponentProps<typeof AutomationPanel> {
  return {
    config, updateConfig: vi.fn(), persistConfig: vi.fn(async () => config),
    initializedTrackingAccounts: [], trackingTarget: { valid: false, reason: "missing" },
    audioStatus: null, audioModState: null, audioModStateLoading: false, audioPreparing: false,
    hasInitializedAudioAccount: false, hasAudioTarget: false, hasReadyAudioMod: false,
    isAudioEnableRequested: false, isAudioRecognitionActive: false,
    onAudioTargetChange: vi.fn(async () => {}), onAudioToggle: vi.fn(async () => false),
    onToggleDiagnosticRecording: vi.fn(async () => {}), onClose: vi.fn(), onInitializeAccount: vi.fn(),
    ...overrides,
  };
}

afterEach(cleanup);

describe("Recognition onboarding", () => {
  it("shows actionable session errors and hostile degradation without hiding active recognition", () => {
    const props = panelProps({ config: { ...config, rune_audio_enabled: true, hostile_auto_restart_enabled: true },
      isAudioEnableRequested: true, isAudioRecognitionActive: true,
      audioStatus: { running: true, hostile_error: "目标 Mod 未安装敌对自动重开", last_error: null, account_id: "leader", target_pid: 42,
        captured_frames: 0, audio_peak: 0, decoded_packets: 0, rune_events: 0, item_events: 0, scene_ocr_events: 0,
        last_marker: null, last_confidence: null, last_detected_at: null, diagnostic_recording: false, diagnostic_recording_path: null },
    });
    const view = render(<AutomationPanel {...props} />);
    expect(screen.getByText("正在监听")).toBeTruthy();
    expect(screen.getByText("敌对自动重开不可用：目标 Mod 未安装敌对自动重开")).toBeTruthy();
    view.rerender(<AutomationPanel {...props} isAudioRecognitionActive={false}
      audioStatus={{ ...props.audioStatus!, running: false, hostile_error: null, last_error: "请重新启动游戏或指定游戏进程" }} />);
    expect(screen.getByText("已启用，需要处理")).toBeTruthy();
    expect(screen.queryByText("已启用，等待游戏运行")).toBeNull();
    expect(screen.getAllByText("请重新启动游戏或指定游戏进程").some(node => !node.closest('[hidden]'))).toBe(true);
  });

  it("renders the new overview, filter summary and launch-time Mod guidance in English", () => {
    render(<AutomationPanel {...panelProps({ config: { ...config, app_language: "en-US", rune_audio_min_rune_number: 20 },
      initializedTrackingAccounts: [account], trackingTarget: { valid: true, account },
      hasInitializedAudioAccount: true, hasAudioTarget: true,
    })} />);
    expect(screen.getByText("Track drops and run times for the selected account.")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Drops to record/ })).toBeTruthy();
    expect(screen.getAllByText("7 drop categories · Runes #20+").length).toBeGreaterThan(0);
    expect(screen.queryByRole("combobox", { name: "Choose a Mod" })).toBeNull();
    expect(screen.getByText(/actual launch arguments/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Prepare Mod" })).toBeNull();
    expect(screen.queryByText("记录哪些掉落")).toBeNull();
    expect(screen.getByRole("option", { name: "Leader" })).toBeTruthy();
    expect(screen.queryByRole("option", { name: /Follow account/ })).toBeNull();
    expect(screen.getByRole("option", { name: "Specify game process (no account needed)" })).toBeTruthy();
  });
  it("shows enabled-but-waiting state without another enable action and keeps diagnostics collapsed", () => {
    render(<AutomationPanel {...panelProps({ config: { ...config, rune_audio_enabled: true },
      initializedTrackingAccounts: [account], trackingTarget: { valid: true, account },
      hasInitializedAudioAccount: true, hasAudioTarget: true, hasReadyAudioMod: true,
      isAudioEnableRequested: true, isAudioRecognitionActive: false,
    })} />);
    expect(screen.getByText("已启用，等待游戏运行")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "立即开启" })).toBeNull();
    expect(screen.getByRole("button", { name: "诊断与高级" }).getAttribute("aria-current")).toBeNull();
    expect(screen.getByText("音频峰值").closest("section[hidden]")).toBeTruthy();
    expect(screen.getByRole("button", { name: "记录内容" }).getAttribute("aria-current")).toBeNull();
    expect(screen.getAllByText(/记录 7 类掉落/).length).toBeGreaterThan(0);
  });
  it("opens account initialization when no account exists", async () => {
    const props = panelProps();
    render(<AutomationPanel {...props} />);
    await userEvent.click(screen.getByRole("button", { name: "初始化账号" }));
    expect(props.onClose).toHaveBeenCalledTimes(1);
    expect(props.onInitializeAccount).toHaveBeenCalledTimes(1);
    expect(vi.mocked(props.onClose).mock.invocationCallOrder[0]).toBeLessThan(vi.mocked(props.onInitializeAccount).mock.invocationCallOrder[0]);
  });

  it("keeps recognition clickable before prerequisites are ready so it can guide setup", async () => {
    const onAudioToggle = vi.fn(async () => false);
    render(<AutomationPanel {...panelProps({ onAudioToggle })} />);
    const toggle = screen.getByRole("switch", { name: "启用音频声纹自动识别" }) as HTMLButtonElement;
    expect(toggle.disabled).toBe(false);
    expect(screen.getByText(/先初始化一个游戏账号/)).toBeTruthy();
    await userEvent.click(toggle);
    expect(onAudioToggle).toHaveBeenCalledWith(true);
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    expect(screen.getByRole("button", { name: "初始化账号" })).toBeTruthy();
  });

  it("lets the user select the first initialized account as the next setup step", async () => {
    const onAudioTargetChange = vi.fn(async () => {});
    render(<AutomationPanel {...panelProps({ initializedTrackingAccounts: [account], hasInitializedAudioAccount: true, onAudioTargetChange })} />);
    await userEvent.click(screen.getByRole("button", { name: "选择首个账号" }));
    expect(onAudioTargetChange).toHaveBeenCalledWith("leader");
  });

});

it("keeps daily settings concise and lets filters apply without opening diagnostics", async () => {
  const props = panelProps();
  render(<AutomationPanel {...props} />);
  expect(screen.queryByRole("checkbox", { name: "符文" })).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "记录内容" }));
  expect(screen.getByRole("checkbox", { name: "符文" })).toBeTruthy();
  expect(screen.getByText("场景计时与掉落截图")).toBeTruthy();
  expect(screen.queryByRole("combobox", { name: "监听目标" })).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "应用掉落过滤" }));
  await waitFor(() => expect(props.persistConfig).toHaveBeenCalledWith(props.config, true));
  expect(invokeCommand).toHaveBeenCalledWith("restart_rune_audio_monitor");
  await userEvent.click(screen.getByRole("button", { name: "监听设置" }));
  expect(screen.getByRole("combobox", { name: "监听目标" })).toBeTruthy();
});

it("keeps specialist controls in advanced settings without crowding the listening page", async () => {
  render(<AutomationPanel {...panelProps()} />);
  expect(screen.queryByRole("region", { name: "敌对自动重开" })).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "诊断与高级" }));
  expect(screen.getByRole("switch", { name: "启用敌对自动重开" })).toBeTruthy();
  expect(screen.getByRole("combobox", { name: "触发范围" })).toBeTruthy();
  await userEvent.click(screen.getByRole("button", { name: "监听设置" }));
  expect(screen.queryByRole("region", { name: "敌对自动重开" })).toBeNull();
});
