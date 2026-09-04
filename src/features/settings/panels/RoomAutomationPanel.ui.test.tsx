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
import type { ModCapsulePool } from "../../../store/types";

const config: RoomAutomationConfig = {
  enabled: true,
  chat_f13_auto_patch_enabled: false,
  primary_account_id: "one",
  follower_account_ids: ["two"],
  auto_followers_enabled: false,
  auto_followers_delay_secs: 5,
  follower_join_mode: "simultaneous",
  follower_join_interval_secs: 3,
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

const capsulePool: ModCapsulePool = {
  generation: 1,
  scanned_at: "2026-09-02T00:00:00+08:00",
  capsules: [{
    id: "cn:rooms",
    edition: "CN",
    name: "Rooms",
    origin: "scanned",
    launch_arguments: "-mod Rooms -txt -assettestmode 1",
    default_launch_arguments: "-mod Rooms -txt -assettestmode 1",
    feature_groups: ["in_game_room_tools"],
    processed: true,
    source_eligible: true,
    update_required: false,
    ready: true,
    deletable: false,
    assigned_account_ids: ["one"],
  }],
  accounts: [
    { account_id: "one", account_name: "Leader", edition: "CN", selected_capsule_id: "cn:rooms", legacy_mod_arguments: "-mod Rooms -txt", issue: null },
    { account_id: "two", account_name: "Follower", edition: "CN", selected_capsule_id: null, legacy_mod_arguments: "", issue: null },
  ],
};

afterEach(cleanup);

describe("RoomAutomationPanel", () => {
  it("redirects the first participant missing a room-tools capsule without a confirmation modal", async () => {
    const { gateway } = makeGateway();
    const disabledSnapshot = { ...snapshot, config: { ...config, enabled: false } };
    gateway.startSync = vi.fn(async (handlers) => {
      handlers.onConfig(disabledSnapshot);
      handlers.onStatus(idleStatus);
      return () => undefined;
    });
    const onRequireRoomTools = vi.fn();
    const user = userEvent.setup();
    render(
      <RoomAutomationPanel
        accounts={accounts}
        gateway={gateway}
        modCapsulePool={capsulePool}
        onRequireRoomTools={onRequireRoomTools}
      />,
    );

    expect(await screen.findByText("配置会保留，但快捷键和跟房任务均不会运行。")).toBeTruthy();
    expect(screen.queryByText("局内房间工具是必要条件")).toBeNull();
    await user.click(screen.getByRole("switch", { name: "启用自动跟房模块" }));
    expect(screen.queryByRole("heading", { name: "启用自动跟房" })).toBeNull();
    expect(onRequireRoomTools).toHaveBeenCalledWith("two", undefined, false);
  });

  it("keeps the room primary independent from the recognition target", async () => {
    const { gateway, saveConfig } = makeGateway();
    const disabledSnapshot = { ...snapshot, config: { ...config, enabled: false } };
    gateway.startSync = vi.fn(async (handlers) => {
      handlers.onConfig(disabledSnapshot);
      handlers.onStatus(idleStatus);
      return () => undefined;
    });
    const readyPool: ModCapsulePool = {
      ...capsulePool,
      capsules: [{ ...capsulePool.capsules[0], assigned_account_ids: ["one", "two"] }],
      accounts: capsulePool.accounts.map((entry) => ({ ...entry, selected_capsule_id: "cn:rooms" })),
    };
    const user = userEvent.setup();
    render(<RoomAutomationPanel accounts={accounts} gateway={gateway} modCapsulePool={readyPool}
      recognitionEnabled recognitionAccountId="two" />);

    await user.click(await screen.findByRole("switch", { name: "启用自动跟房模块" }));
    await waitFor(() => expect(saveConfig).toHaveBeenCalled());
    const saved = saveConfig.mock.calls[saveConfig.mock.calls.length - 1]?.[1];
    expect(saved.primary_account_id).toBe("one");
    expect(saved.follower_account_ids).toEqual(["two"]);
  });

  it("keeps module copy local, shows a room preview, and saves with generation CAS", async () => {
    const { gateway, saveConfig } = makeGateway();
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} />);

    expect(await screen.findByRole("heading", { name: "Room Automation" })).toBeTruthy();
    expect(screen.getByText("run-007")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Room prefix"), { target: { value: "chaos-" } });
    await waitFor(() => expect(saveConfig.mock.calls[saveConfig.mock.calls.length - 1]?.[1].name_prefix).toBe("chaos-"));
    expect(saveConfig.mock.calls[0][0]).toBe(4);
    expect(await screen.findByText("chaos-007")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Apply settings/ })).toBeNull();
  });

  it("removes the redundant manual room action bar", async () => {
    const { gateway } = makeGateway();
    render(<RoomAutomationPanel accounts={accounts} gateway={gateway} />);
    expect(await screen.findByRole("heading", { name: "自动跟房" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /主账号创建房间/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /让跟随账号加入/ })).toBeNull();
  });

  it("requires an explicit F13 scan when an enabled module needs it", async () => {
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
    expect(gateway.installChatBinding).not.toHaveBeenCalled();
    await userEvent.click(screen.getAllByRole("button", { name: "扫描并补装 F13" })[0]);
    await waitFor(() => expect(gateway.installChatBinding).toHaveBeenCalledTimes(1));
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

  it("auto-saves valid edits without an apply or discard boundary", async () => {
    const { gateway, saveConfig } = makeGateway();
    const user = userEvent.setup();
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} />);

    const prefix = await screen.findByLabelText("Room prefix") as HTMLInputElement;
    await user.clear(prefix);
    await user.type(prefix, "instant-");

    await waitFor(() => expect(saveConfig.mock.calls[saveConfig.mock.calls.length - 1]?.[1].name_prefix).toBe("instant-"));
    expect(screen.queryByRole("button", { name: /Apply settings|Discard/ })).toBeNull();
    await waitFor(() => expect(screen.getByText("Settings applied")).toBeTruthy());
  });

  it("keeps a stale save conflict sticky until an explicit reload", async () => {
    const { gateway, saveConfig } = makeGateway();
    saveConfig.mockRejectedValueOnce(new Error("stale generation"));
    const user = userEvent.setup();
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} />);

    const prefix = await screen.findByLabelText("Room prefix") as HTMLInputElement;
    await user.clear(prefix);
    await user.type(prefix, "local-");

    expect(await screen.findByText(/Settings changed elsewhere/)).toBeTruthy();
    await user.type(prefix, "again");
    expect(screen.getByText(/Settings changed elsewhere/)).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Reload" }));
    await waitFor(() => expect(screen.queryByText(/Settings changed elsewhere/)).toBeNull());
  });

  it("captures canonical shortcuts instead of accepting free-form text", async () => {
    const { gateway, saveConfig } = makeGateway();
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
    await waitFor(() => expect(saveConfig.mock.calls[saveConfig.mock.calls.length - 1]?.[1].shortcut).toBe("Ctrl+Num+"));
  });

  it("keeps settings editable without rendering the removed current-task controls", async () => {
    const { gateway, getHandlers, saveConfig } = makeGateway();
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

    expect(screen.queryByRole("button", { name: "Cancel task" })).toBeNull();
    expect(screen.queryByText("Current task")).toBeNull();
    expect((screen.getByLabelText("Room prefix") as HTMLInputElement).disabled).toBe(false);
    await waitFor(() => expect(saveConfig.mock.calls[saveConfig.mock.calls.length - 1]?.[1].name_prefix).toBe("local-"));
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
    render(<RoomAutomationPanel accounts={accounts} language="en-US" gateway={gateway} />);

    const prefix = await screen.findByLabelText("Room prefix") as HTMLInputElement;
    fireEvent.change(prefix, { target: { value: "saved-" } });

    expect(await screen.findByText(/Settings were saved, but the module could not apply them immediately/)).toBeTruthy();
    expect((screen.getByLabelText("Room prefix") as HTMLInputElement).value).toBe("saved-");
    expect(screen.getByText("Settings applied")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Apply settings/ })).toBeNull();
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

    await user.click(await screen.findByText("In-game chat binding"));
    const restore = await screen.findByRole("button", { name: "Restore original binding" });
    expect((restore as HTMLButtonElement).disabled).toBe(false);
    await user.click(restore);
    expect(gateway.restoreChatBinding).toHaveBeenCalledTimes(1);
  });
});
