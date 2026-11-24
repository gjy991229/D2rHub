import type { ReactNode } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useGlobalConfig } from "../../store/globalConfig";
import { useAccounts } from "../../store/accounts";
import { invokeCommand } from "../../platform/tauri";
import type { GlobalConfig } from "../../store/types";
import type { GlobalConfigPatch } from "../../utils/globalConfigPatch";
import type { SettingsTabId } from "../../features/settings/settingsRegistry";
import { SettingsCenter } from "./SettingsCenter";

const modRuntime = vi.hoisted(() => ({ busy: false }));
vi.mock("../../features/mods/workflow/useModWorkflow", () => ({ useModWorkflow: () => ({ busy: modRuntime.busy, actions: { openLibrary: vi.fn(), requestProcessing: vi.fn() } }) }));

const navigation = vi.hoisted(() => ({ result: null as Promise<boolean> | null }));

vi.mock("../../platform/tauri", () => ({
  invokeCommand: vi.fn(async () => null),
  listenEvent: vi.fn(async () => () => {}),
  emitEvent: vi.fn(async () => {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../../components/ui/Toast", () => ({ showToast: vi.fn() }));
vi.mock("../../features/settings/SettingsShell", () => ({
  SettingsShell: ({ children, activeTab, onTabChange, onClose }: {
    children: ReactNode; activeTab: SettingsTabId;
    onTabChange: (tab: SettingsTabId) => Promise<boolean>;
    onClose: () => void;
  }) => <div>
    <output aria-label="Current section">{activeTab}</output>
    <button onClick={() => { navigation.result = onTabChange("agent"); }}>Change section</button>
    <button onClick={onClose}>Close settings</button>
    {children}
  </div>,
}));
vi.mock("../../features/settings/panels/PathsPanel", () => ({
  PathsPanel: ({ config, updateConfig }: {
    config: GlobalConfig; updateConfig: (update: (config: GlobalConfig) => void) => void;
  }) => <input aria-label="Game path" value={config.cn_game_path}
    onChange={event => updateConfig(next => { next.cn_game_path = event.target.value; })} />,
}));
vi.mock("../../features/settings/panels/LaunchStrategyPanel", () => ({ LaunchStrategyPanel: () => <p>Launch settings</p> }));

const initial = {
  cn_game_path: "C:\\Games\\D2R", global_game_path: "", cn_saved_games_path: "",
  global_saved_games_path: "", installed_optional_modules: [], feature_profile: "normal",
  app_language: "zh-CN", rune_audio_target_account: "", launch_groups: [], theme: "light",
  main_opacity: 95, font_scale: "default",
} as unknown as GlobalConfig;

beforeEach(() => {
  navigation.result = null;
  modRuntime.busy = false;
  useGlobalConfig.setState({ config: initial, patch: vi.fn(async changes => {
    const saved = { ...useGlobalConfig.getState().config!, ...changes };
    useGlobalConfig.setState({ config: saved });
    return saved;
  }) });
  useAccounts.setState({ accounts: [] });
});
afterEach(cleanup);

function mount(onClose = vi.fn(), initialTab: SettingsTabId = "paths") {
  return render(<SettingsCenter open initialTab={initialTab} onClose={onClose}
    onReconfigure={vi.fn()} onInitializeAccount={vi.fn()} />);
}

it("can repair supported configs from maintenance in Pure mode without an installed module", async () => {
  useGlobalConfig.setState({ config: { ...initial, feature_profile: "minimal" } });
  const reloadAccounts = vi.fn(async () => undefined);
  const originalLoadAccounts = useAccounts.getState().loadAccounts;
  useAccounts.setState({ loadAccounts: reloadAccounts });
  const report = {
    repaired: ["acount1/account.json.corrupt-test"],
    issues: ["acount2/account.json: 没有有效备份"],
  };
  try {
    await vi.mocked(invokeCommand).withImplementation(
      async command => command === "check_and_repair_configs" ? report : null,
      async () => {
        mount(vi.fn(), "advanced");
        expect(screen.getByLabelText("Current section").textContent).toBe("advanced");
        fireEvent.click(await screen.findByRole("button", { name: "检查并修复配置" }));
        expect(await screen.findByText(report.repaired[0])).toBeTruthy();
        expect(screen.getByText(report.issues[0])).toBeTruthy();
        expect(invokeCommand).toHaveBeenCalledWith("check_and_repair_configs");
        expect(reloadAccounts).toHaveBeenCalledOnce();
      },
    );
  } finally {
    useAccounts.setState({ loadAccounts: originalLoadAccounts });
  }
});

it("keeps navigation pending until the isolated draft is committed", async () => {
  let complete!: (config: GlobalConfig) => void;
  const patch = vi.fn((changes: GlobalConfigPatch) => new Promise<GlobalConfig>(resolve => {
    complete = saved => { useGlobalConfig.setState({ config: saved }); resolve(saved); };
    expect(changes).toEqual({ cn_game_path: "D:\\D2R" });
  }));
  useGlobalConfig.setState({ patch });
  mount();
  fireEvent.change(await screen.findByLabelText("Game path"), { target: { value: "D:\\D2R" } });
  expect(useGlobalConfig.getState().config?.cn_game_path).toBe(initial.cn_game_path);
  fireEvent.click(screen.getByText("Change section"));
  expect(navigation.result).toBeInstanceOf(Promise);
  expect(screen.getByLabelText("Current section").textContent).toBe("paths");
  await act(async () => {
    complete({ ...initial, cn_game_path: "D:\\D2R" });
    expect(await navigation.result).toBe(true);
  });
  expect(screen.getByLabelText("Current section").textContent).toBe("agent");
});

it("allows navigation while preserving a failed draft and offering recovery", async () => {
  const patch = vi.fn(async () => { throw new Error("disk full"); });
  useGlobalConfig.setState({ patch });
  mount();
  fireEvent.change(await screen.findByLabelText("Game path"), { target: { value: "D:\\D2R" } });
  fireEvent.click(screen.getByText("Change section"));
  await act(async () => { expect(await navigation.result).toBe(true); });
  expect(screen.getByLabelText("Current section").textContent).toBe("agent");
  expect(screen.getByRole("alert")).toBeTruthy();
  fireEvent.click(screen.getByText("返回修改处"));
  expect(screen.getByLabelText("Current section").textContent).toBe("paths");
  expect((screen.getByLabelText("Game path") as HTMLInputElement).value).toBe("D:\\D2R");
  expect(useGlobalConfig.getState().config?.cn_game_path).toBe(initial.cn_game_path);
  await waitFor(() => expect(patch).toHaveBeenCalledOnce());
});

it("keeps settings open when the user types another change during close-save", async () => {
  let complete!: (config: GlobalConfig) => void;
  const pending = new Promise<GlobalConfig>(resolve => {
    complete = saved => { useGlobalConfig.setState({ config: saved }); resolve(saved); };
  });
  useGlobalConfig.setState({ patch: vi.fn(() => pending) });
  const onClose = vi.fn();
  mount(onClose);
  fireEvent.change(await screen.findByLabelText("Game path"), { target: { value: "D:\\D2R" } });
  fireEvent.click(screen.getByText("Close settings"));
  fireEvent.change(screen.getByLabelText("Game path"), { target: { value: "E:\\D2R" } });
  await act(async () => {
    complete({ ...initial, cn_game_path: "D:\\D2R" });
    await pending;
  });
  expect(onClose).not.toHaveBeenCalled();
  expect((screen.getByLabelText("Game path") as HTMLInputElement).value).toBe("E:\\D2R");
  expect(useGlobalConfig.getState().config?.cn_game_path).toBe("D:\\D2R");
});

it("offers an explicit discard exit instead of trapping an invalid path draft", async () => {
  useGlobalConfig.setState({ patch: vi.fn(async () => { throw new Error("invalid path"); }) });
  const onClose = vi.fn(); mount(onClose);
  fireEvent.change(await screen.findByLabelText("Game path"), { target: { value: "" } });
  fireEvent.click(screen.getByText("Close settings"));
  expect(onClose).not.toHaveBeenCalled();
  fireEvent.click(await screen.findByText("放弃未保存修改并关闭"));
  expect(onClose).toHaveBeenCalledOnce();
  expect(useGlobalConfig.getState().config?.cn_game_path).toBe(initial.cn_game_path);
});

it("keeps unrelated navigation and closing available during background Mod work", async () => {
  modRuntime.busy = true;
  const onClose = vi.fn(); mount(onClose);
  await screen.findByLabelText("Game path");
  expect(screen.getByText("Mod 正在后台加工，可以继续使用其他设置。")).toBeTruthy();
  fireEvent.click(screen.getByText("Change section"));
  await act(async () => { expect(await navigation.result).toBe(true); });
  fireEvent.click(screen.getByText("Close settings"));
  await waitFor(() => expect(onClose).toHaveBeenCalledOnce());
});
