import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invokeCommand } from "../../platform/tauri";
import { MiniDashboard } from "./MiniDashboard";
import { useAccounts } from "../../store/accounts";
import { useGlobalConfig } from "../../store/globalConfig";
import { useLaunch } from "../../store/launch";
import type { AccountMeta, GlobalConfig } from "../../store/types";

vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ isVisible: async () => false }) }));
vi.mock("../../platform/tauri", () => ({ invokeCommand: vi.fn().mockResolvedValue([]), emitEvent: vi.fn(), listenEvent: vi.fn() }));

beforeEach(() => {
  vi.mocked(invokeCommand).mockResolvedValue([]);
  useAccounts.setState({ accounts: [] });
  useGlobalConfig.setState({ config: { app_language: "en-US", launch_groups: [] } as unknown as GlobalConfig, saving: false });
  useLaunch.setState({ launching: false, progress: {}, results: [], error: null, logs: [] });
});
afterEach(cleanup);

function renderMini() {
  render(<MiniDashboard busy={false} pinned={false} onPin={vi.fn()} onExpand={vi.fn()}
    onConfigure={vi.fn()} onManageGroups={vi.fn()} onTasks={vi.fn()} onKillAll={vi.fn()}
    onAddAccount={vi.fn()} onRoomAutomation={vi.fn()} />);
}

it("navigates the mini action menu with arrows and restores its trigger on Escape", () => {
  renderMini();
  const trigger = screen.getByRole("button", { name: "More actions" });
  fireEvent.keyDown(trigger, { key: "ArrowDown" });
  expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Settings" }));
  fireEvent.keyDown(document.activeElement!, { key: "End" });
  expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Close all…" }));
  fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
  expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Settings" }));
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  expect(screen.queryByRole("menu")).toBeNull();
  expect(document.activeElement).toBe(trigger);
});

it("starts a mini row with its saved scheme and only the requested account", async () => {
  const accounts = ["one", "two"].map(id => ({ id, initialized: true, auth_mode: "token" } as AccountMeta));
  useAccounts.setState({ accounts });
  useGlobalConfig.setState({ config: {
    ...useGlobalConfig.getState().config!,
    launch_groups: [{ id: "daily", name: "Daily", account_ids: ["one", "two"], revision: 4 }],
  } });
  try {
    renderMini();
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "daily" } });
    fireEvent.click(screen.getByRole("button", { name: "Start two" }));
    await waitFor(() => expect(invokeCommand).toHaveBeenCalledWith("launch_saved_scheme", { id: "daily", revision: 4, accountIds: ["two"] }));
  } finally {
    localStorage.removeItem("d2rhub-mini-launch-group");
  }
});
