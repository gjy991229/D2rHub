import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ModProcessingPanel } from "./ModProcessingPanel";
import type { AccountMeta, GlobalConfig } from "../../../store/types";
import type { ModCapsuleController } from "../../modCapsules/useModCapsulePool";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("../../../platform/tauri", () => ({ invokeCommand: invoke }));
vi.mock("../../tasks/taskSync", () => ({ subscribeBeforeReadingTasks: vi.fn(async () => () => {}) }));
const account = { id: "one", display_name: "One", initialized: true } as AccountMeta;
function props(): ComponentProps<typeof ModProcessingPanel> {
  return {
    config: { app_language: "zh-CN" } as GlobalConfig, initializedAccounts: [account],
    trackingTarget: { valid: true, account }, audioModState: null, audioModStateLoading: false, audioModScannedAt: null,
    purpose: "recognition", audioSetupMode: "original", setAudioSetupMode: vi.fn(),
    audioSetupSource: "", setAudioSetupSource: vi.fn(), audioSetupName: "", setAudioSetupName: vi.fn(),
    includeAudioTelemetry: true, setIncludeAudioTelemetry: vi.fn(), includeRoomTools: false, setIncludeRoomTools: vi.fn(),
    includeAutoExitOnDeath: false, setIncludeAutoExitOnDeath: vi.fn(),
    audioPreparing: false, audioPrepareProgress: null, isAudioModUpgrade: false, isAudioModFeatureManagement: false,
    audioSetupNameError: null, showAudioSetupNameError: false, audioPrepareBlockedReason: "请输入新 Mod 名称",
    onTargetChange: vi.fn(async () => {}), onPrepare: vi.fn(async () => {}), onRefresh: vi.fn(async () => {}), onBackToRecognition: vi.fn(),
  };
}
beforeEach(() => {
  invoke.mockReset().mockResolvedValue({ catalog: { assets: [] }, processor: { ready: false } });
});
afterEach(cleanup);
describe("missing processor navigation", () => {
  it("offers a download action even if the processing form is not filled in", async () => {
    const input = props();
    render(<ModProcessingPanel {...input} />);
    const button = await screen.findByRole("button", { name: "下载加工器" });
    expect(button.hasAttribute("disabled")).toBe(false);
    await userEvent.click(button);
    expect(screen.getByRole("heading", { name: "Mod 下载与更新" })).toBeTruthy();
    expect(input.onPrepare).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "返回" }));
    await screen.findByRole("button", { name: "下载加工器" });
  });
  it("redirects the catalog Process action to downloads without starting processing", async () => {
    const input = props();
    const catalog = { loading: false, error: null, refresh: vi.fn(), scan: vi.fn(), pool: { generation: 1, accounts: [], capsules: [
      { id: "plain", edition: "Global", name: "Plain", origin: "scanned", source_eligible: true, launch_arguments: "-mod Plain -txt", feature_groups: [], assigned_account_ids: [], ready: true },
    ] } } as unknown as ModCapsuleController;
    render(<ModProcessingPanel {...input} purpose="manage" modCatalog={catalog} />);
    await userEvent.click(screen.getByRole("button", { name: "加工" }));
    await screen.findByRole("heading", { name: "Mod 下载与更新" });
    expect(invoke).toHaveBeenCalledWith("get_mod_resources", { edition: "Global", refresh: false });
    expect(input.onPrepare).not.toHaveBeenCalled();
    expect(input.onTargetChange).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "返回" }));
    await waitFor(() => expect(screen.getByText("Plain")).toBeTruthy());
  });
});
