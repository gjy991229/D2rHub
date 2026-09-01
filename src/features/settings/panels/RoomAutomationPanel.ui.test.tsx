import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RoomAutomationGateway, RoomAutomationSyncHandlers } from "../../roomAutomation/gateway";
import type {
  RoomAutomationConfig,
  RoomAutomationConfigSnapshot,
  RoomAutomationSaveOutcome,
  RoomAutomationWorkflowStatus,
  RoomChatBindingStatus,
} from "../../roomAutomation/types";
import { RoomAutomationPanel } from "./RoomAutomationPanel";

const config: RoomAutomationConfig = {
  enabled: true,
  chat_f13_auto_patch_enabled: false,
  primary_account_id: "one",
  follower_account_ids: ["two"],
  auto_followers_enabled: false,
  auto_followers_delay_secs: 5,
  shortcut: "Ctrl+Alt+R",
  join_shortcut: "Ctrl+Alt+J",
  name_prefix: "run-",
  password: "pw",
  next_sequence: 7,
  sequence_width: 3,
  background_text_strategy: "post_keys",
  strategy_version: 17,
  flow: { step_delay_ms: 80, character_delay_ms: 10 },
};

const snapshot: RoomAutomationConfigSnapshot = {
  schema_version: 1,
  generation: 4,
  config,
  normalization: {
    source_strategy_version: 17,
    target_strategy_version: 17,
    changed: false,
    requires_chat_binding_consent: false,
  },
  consent_notice: null,
};

const idleStatus: RoomAutomationWorkflowStatus = {
  revision: 3,
  task_id: null,
  running: false,
  phase: "idle",
  recovery_action: null,
  waiting_mode: null,
  room_name: null,
  room_sequence: null,
  attempt: 0,
  primary_account_id: null,
  follower_account_ids: [],
  completed_follower_account_ids: [],
  started_at: null,
  last_error: null,
};

const binding: RoomChatBindingStatus = {
  ready: false,
  totalFiles: 2,
  installedFiles: 0,
  eligibleFiles: 2,
  conflictedFiles: 0,
  backupFiles: 0,
  orphanBackupFiles: 0,
  transactionArtifacts: 0,
  d2rRunning: false,
  consentGranted: false,
  watcherRunning: false,
  autoPatchEnabled: false,
  directories: [],
  lastWatcherError: null,
  message: "Not installed",
};

const readyBinding: RoomChatBindingStatus = {
  ...binding,
  ready: true,
  installedFiles: 2,
  eligibleFiles: 0,
  backupFiles: 2,
  consentGranted: true,
  watcherRunning: true,
  autoPatchEnabled: true,
  message: "Ready",
};

function makeGateway(
  status: RoomAutomationWorkflowStatus = idleStatus,
  chatBinding: RoomChatBindingStatus = readyBinding,
) {
  let handlers: RoomAutomationSyncHandlers | null = null;
  const saveConfig = vi.fn(async (
    _generation: number,
    next: RoomAutomationConfig,
  ): Promise<RoomAutomationSaveOutcome> => ({
    snapshot: {
      ...snapshot,
      generation: 5,
      config: next,
    },
    apply_warning: null,
  }));
  const gateway: RoomAutomationGateway = {
    getConfig: vi.fn(async () => snapshot),
    saveConfig,
    getStatus: vi.fn(async () => status),
    startPrimary: vi.fn(async (): Promise<RoomAutomationWorkflowStatus> => ({ ...status, revision: status.revision + 1, phase: "primary", running: true })),
    startFollowers: vi.fn(async (): Promise<RoomAutomationWorkflowStatus> => ({ ...status, revision: status.revision + 1, phase: "followers", running: true })),
    retry: vi.fn(async (): Promise<RoomAutomationWorkflowStatus> => ({ ...status, revision: status.revision + 1, phase: "primary", running: true })),
    cancel: vi.fn(async (): Promise<RoomAutomationWorkflowStatus> => ({ ...status, revision: status.revision + 1, phase: "cancelled", running: false })),
    getChatBinding: vi.fn(async () => chatBinding),
    installChatBinding: vi.fn(async () => ({ ...chatBinding, ready: true, installedFiles: 2, backupFiles: 2 })),
    restoreChatBinding: vi.fn(async () => chatBinding),
    startSync: vi.fn(async (nextHandlers) => {
      handlers = nextHandlers;
      nextHandlers.onConfig(snapshot);
      nextHandlers.onStatus(status);
      return () => undefined;
    }),
  };
  return { gateway, saveConfig, getHandlers: () => handlers };
}

const accounts = [
  { id: "one", display_name: "Leader", initialized: true },
  { id: "two", display_name: "Follower", initialized: true },
] as never;

afterEach(cleanup);

describe("RoomAutomationPanel", () => {
  it("shows the Mod prerequisite only when enabling and can jump to Mod settings", async () => {
    const { gateway } = makeGateway();
    const disabledSnapshot = { ...snapshot, config: { ...config, enabled: false } };
    gateway.startSync = vi.fn(async (handlers) => {
      handlers.onConfig(disabledSnapshot);
      handlers.onStatus(idleStatus);
      return () => undefined;
    });
    const onOpenAudioModSettings = vi.fn();
    const user = userEvent.setup();
    render(
      <RoomAutomationPanel
        accounts={accounts}
        gateway={gateway}
        onOpenAudioModSettings={onOpenAudioModSettings}
      />,
    );

    expect(await screen.findByText("配置会保留，但快捷键、文件监视和跟房任务均不会运行。")).toBeTruthy();
    expect(screen.queryByText("局内房间工具是必要条件")).toBeNull();
    await user.click(screen.getByRole("switch", { name: "启用自动跟房模块" }));
    expect(screen.getByRole("heading", { name: "启用自动跟房" })).toBeTruthy();
    expect(screen.getByText(/参与账号需要使用包含/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "先检查 Mod" }));
    expect(onOpenAudioModSettings).toHaveBeenCalledTimes(1);
  });

  it("keeps module copy local, shows a room preview, and saves with generation CAS", async () => {
    const { gateway, saveConfig } = makeGateway();
    const user = userEvent.setup();
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} />);

    expect(await screen.findByRole("heading", { name: "Room Automation" })).toBeTruthy();
    expect(screen.getByText("run-007")).toBeTruthy();

    await user.clear(screen.getByLabelText("Room prefix"));
    await user.type(screen.getByLabelText("Room prefix"), "chaos-");

    expect(screen.getByText("Unapplied changes")).toBeTruthy();
    expect((screen.getByRole("button", { name: /Create with primary/ }) as HTMLButtonElement).disabled).toBe(true);
    await user.click(screen.getByRole("button", { name: /Apply settings/ }));

    await waitFor(() => expect(saveConfig).toHaveBeenCalledTimes(1));
    expect(saveConfig.mock.calls[0][0]).toBe(4);
    expect(saveConfig.mock.calls[0][1].name_prefix).toBe("chaos-");
    expect(await screen.findByText("chaos-007")).toBeTruthy();
  });

  it("keeps manual waiting active while allowing the next-sequence primary action", async () => {
    const waitingStatus: RoomAutomationWorkflowStatus = {
      ...idleStatus,
      revision: 8,
      task_id: 12,
      running: false,
      phase: "waiting",
      recovery_action: null,
      waiting_mode: { mode: "manual" },
      room_name: "run-007",
      room_sequence: 7,
      attempt: 1,
      primary_account_id: "one",
      follower_account_ids: ["two"],
    };
    const { gateway } = makeGateway(waitingStatus);
    const user = userEvent.setup();
    render(<RoomAutomationPanel accounts={accounts} gateway={gateway} />);

    expect(await screen.findByText(/若房名重复/)).toBeTruthy();
    expect((screen.getByLabelText("房名开头") as HTMLInputElement).disabled).toBe(true);
    expect(screen.queryByRole("button", { name: /应用配置/ })).toBeNull();
    const nextPrimary = screen.getByRole("button", { name: /下一序号重新建房/ });
    expect((nextPrimary as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByRole("button", { name: /让跟随账号加入/ }) as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByRole("button", { name: /取消任务/ }) as HTMLButtonElement).disabled).toBe(false);
    await user.click(nextPrimary);
    await waitFor(() => expect(gateway.startPrimary).toHaveBeenCalledTimes(1));
  });

  it("does not allow manual actions during automatic waiting", async () => {
    const automaticStatus: RoomAutomationWorkflowStatus = {
      ...idleStatus,
      revision: 9,
      task_id: 13,
      running: true,
      phase: "waiting",
      waiting_mode: { mode: "automatic", delay_secs: 5 },
      room_name: "run-007",
      room_sequence: 7,
      primary_account_id: "one",
      follower_account_ids: ["two"],
    };
    const { gateway } = makeGateway(automaticStatus);
    render(<RoomAutomationPanel accounts={accounts} gateway={gateway} />);

    expect(await screen.findByText("5 秒后自动继续")).toBeTruthy();
    expect((screen.getByRole("button", { name: /主账号创建房间/ }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: /让跟随账号加入/ }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("renders explicit migration consent and non-color binding state", async () => {
    const { gateway } = makeGateway(idleStatus, binding);
    const consentSnapshot = {
      ...snapshot,
      consent_notice: {
        source: "global-v9.room_rotation",
        original_strategy_version: 12,
        requires_user_reauthorization: true,
      },
    };
    gateway.startSync = vi.fn(async (handlers) => {
      handlers.onConfig(consentSnapshot);
      handlers.onStatus(idleStatus);
      return () => undefined;
    });
    render(<RoomAutomationPanel accounts={accounts} gateway={gateway} />);

    expect(await screen.findByText(/旧版配置中的自动授权已被撤销/)).toBeTruthy();
    expect(screen.getByText("F13 绑定尚未就绪")).toBeTruthy();
    expect((screen.getByRole("button", { name: "授权并安装 F13 绑定" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("keeps configuration available when the binding status fails and releases sync on unmount", async () => {
    const { gateway } = makeGateway();
    const stop = vi.fn();
    gateway.startSync = vi.fn(async (handlers) => {
      handlers.onConfig(snapshot);
      handlers.onStatus(idleStatus);
      return stop;
    });
    gateway.getChatBinding = vi.fn(async () => {
      throw new Error("binding service unavailable");
    });

    const { unmount } = render(<RoomAutomationPanel accounts={accounts} gateway={gateway} />);

    expect(await screen.findByText(/binding service unavailable/)).toBeTruthy();
    expect(screen.getByLabelText("房名开头")).toBeTruthy();
    expect(stop).not.toHaveBeenCalled();
    unmount();
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("reports its explicit draft boundary and can discard without saving", async () => {
    const { gateway, saveConfig } = makeGateway();
    const onDirtyChange = vi.fn();
    const user = userEvent.setup();
    render(
      <RoomAutomationPanel
        accounts={accounts}
        language="en-US"
        gateway={gateway}
        onDirtyChange={onDirtyChange}
      />,
    );

    const prefix = await screen.findByLabelText("Room prefix") as HTMLInputElement;
    await user.clear(prefix);
    await user.type(prefix, "discard-");
    await waitFor(() => expect(onDirtyChange).toHaveBeenLastCalledWith(true));

    await user.click(screen.getByRole("button", { name: "Discard" }));

    await waitFor(() => expect((screen.getByLabelText("Room prefix") as HTMLInputElement).value).toBe("run-"));
    expect(onDirtyChange).toHaveBeenLastCalledWith(false);
    expect(saveConfig).not.toHaveBeenCalled();
  });

  it("derives dirty state from the persisted snapshot instead of edit history", async () => {
    const { gateway } = makeGateway();
    const onDirtyChange = vi.fn();
    const user = userEvent.setup();
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} onDirtyChange={onDirtyChange} />);

    const prefix = await screen.findByLabelText("Room prefix") as HTMLInputElement;
    await user.clear(prefix);
    await user.type(prefix, "temporary-");
    await waitFor(() => expect(onDirtyChange).toHaveBeenLastCalledWith(true));
    await user.clear(prefix);
    await user.type(prefix, "run-");

    await waitFor(() => expect(onDirtyChange).toHaveBeenLastCalledWith(false));
    expect(screen.getByText("Settings applied")).toBeTruthy();
  });

  it("keeps an external-generation conflict sticky until an explicit reload", async () => {
    const { gateway, getHandlers } = makeGateway();
    const user = userEvent.setup();
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} />);

    const prefix = await screen.findByLabelText("Room prefix") as HTMLInputElement;
    await user.clear(prefix);
    await user.type(prefix, "local-");
    getHandlers()?.onConfig({
      ...snapshot,
      generation: 5,
      config: { ...config, name_prefix: "external-" },
    });

    expect(await screen.findByText(/Settings changed elsewhere/)).toBeTruthy();
    await user.type(prefix, "again");
    expect(screen.getByText(/Settings changed elsewhere/)).toBeTruthy();
    expect((screen.getByRole("button", { name: /Apply settings/ }) as HTMLButtonElement).disabled).toBe(true);

    await user.click(screen.getByRole("button", { name: "Reload" }));
    await waitFor(() => expect(screen.queryByText(/Settings changed elsewhere/)).toBeNull());
  });

  it("captures canonical shortcuts instead of accepting free-form text", async () => {
    const { gateway } = makeGateway();
    const user = userEvent.setup();
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} />);

    const shortcut = await screen.findByRole("button", { name: "Create-room shortcut" });
    await user.click(shortcut);
    expect(shortcut.textContent).toBe("Press a key combination…");
    fireEvent.keyDown(shortcut, { key: "F12", ctrlKey: true, shiftKey: true });
    expect(shortcut.textContent).toBe("Ctrl+Shift+F12");

    await user.click(shortcut);
    fireEvent.keyDown(shortcut, { key: "+", code: "NumpadAdd", ctrlKey: true });
    expect(shortcut.textContent).toBe("Ctrl+Num+");
    expect(screen.getByText("Unapplied changes")).toBeTruthy();
  });

  it("can cancel an externally-started workflow and discard a dirty local draft", async () => {
    const { gateway, getHandlers } = makeGateway();
    const user = userEvent.setup();
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} />);

    const prefix = await screen.findByLabelText("Room prefix") as HTMLInputElement;
    await user.clear(prefix);
    await user.type(prefix, "local-");
    getHandlers()?.onStatus({
      ...idleStatus,
      revision: 9,
      task_id: 31,
      running: false,
      phase: "waiting",
      recovery_action: null,
      waiting_mode: { mode: "manual" },
      room_name: "run-007",
      room_sequence: 7,
      primary_account_id: "one",
      follower_account_ids: ["two"],
    });

    const cancel = await screen.findByRole("button", { name: "Cancel task" });
    expect((cancel as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByRole("button", { name: "Discard" }) as HTMLButtonElement).disabled).toBe(false);

    await user.click(cancel);
    await waitFor(() => expect(gateway.cancel).toHaveBeenCalledTimes(1));
    await user.click(screen.getByRole("button", { name: "Discard" }));
    await waitFor(() => expect((screen.getByLabelText("Room prefix") as HTMLInputElement).value).toBe("run-"));
  });

  it("accepts a durable save outcome while surfacing a later lifecycle warning", async () => {
    const { gateway, saveConfig } = makeGateway();
    const committed = {
      ...snapshot,
      generation: 5,
      config: { ...config, name_prefix: "saved-" },
    };
    saveConfig.mockResolvedValueOnce({
      snapshot: committed,
      apply_warning: "shortcut reload failed",
    });
    const user = userEvent.setup();
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} />);

    const prefix = await screen.findByLabelText("Room prefix") as HTMLInputElement;
    await user.clear(prefix);
    await user.type(prefix, "saved-");
    await user.click(screen.getByRole("button", { name: /Apply settings/ }));

    expect(await screen.findByText(/Settings were saved, but the module could not apply them immediately/)).toBeTruthy();
    expect((screen.getByLabelText("Room prefix") as HTMLInputElement).value).toBe("saved-");
    expect(screen.getByText("Settings applied")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Apply settings/ })).toBeNull();
  });

  it("labels follower recovery as a same-room retry", async () => {
    const recoveryStatus: RoomAutomationWorkflowStatus = {
      ...idleStatus,
      revision: 11,
      task_id: 41,
      phase: "error",
      recovery_action: "resume_followers",
      room_name: "run-007",
      room_sequence: 7,
      primary_account_id: "one",
      follower_account_ids: ["two"],
      last_error: "follower failed",
    };
    const { gateway } = makeGateway(recoveryStatus);
    const user = userEvent.setup();
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} />);

    const retry = await screen.findByRole("button", { name: "Retry followers (same room)" });
    expect((retry as HTMLButtonElement).disabled).toBe(false);
    await user.click(retry);
    expect(gateway.retry).toHaveBeenCalledTimes(1);
  });

  it("labels primary recovery as using the next durably reserved sequence", async () => {
    const recoveryStatus: RoomAutomationWorkflowStatus = {
      ...idleStatus,
      revision: 12,
      task_id: 42,
      phase: "error",
      recovery_action: "retry_primary",
      room_name: "run-007",
      room_sequence: 7,
      primary_account_id: "one",
      follower_account_ids: ["two"],
      last_error: "primary failed",
    };
    const { gateway } = makeGateway(recoveryStatus);
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} />);

    expect(await screen.findByRole("button", { name: "Retry creation (next sequence)" })).toBeTruthy();
  });

  it("can refresh a healthy F13 binding status", async () => {
    const { gateway } = makeGateway();
    const user = userEvent.setup();
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} />);

    await user.click(await screen.findByText("In-game chat binding"));
    const refresh = await screen.findByRole("button", { name: "Refresh status" });
    await waitFor(() => expect(gateway.getChatBinding).toHaveBeenCalledTimes(1));
    await user.click(refresh);
    await waitFor(() => expect(gateway.getChatBinding).toHaveBeenCalledTimes(2));
  });

  it("allows restoring backed-up bindings after the optional module is disabled", async () => {
    const disabledSnapshot = {
      ...snapshot,
      config: { ...config, enabled: false },
    };
    const { gateway } = makeGateway(idleStatus, readyBinding);
    gateway.getConfig = vi.fn(async () => disabledSnapshot);
    gateway.startSync = vi.fn(async (handlers) => {
      handlers.onConfig(disabledSnapshot);
      handlers.onStatus(idleStatus);
      return () => undefined;
    });
    const user = userEvent.setup();
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} />);

    const restore = await screen.findByRole("button", { name: "Restore original binding" });
    expect((restore as HTMLButtonElement).disabled).toBe(false);
    await user.click(restore);
    expect(gateway.restoreChatBinding).toHaveBeenCalledTimes(1);
  });
});
