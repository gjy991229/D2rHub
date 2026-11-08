import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { MiniDashboard } from "./MiniDashboard";
import { useAccounts } from "../../store/accounts";
import { useGlobalConfig } from "../../store/globalConfig";
import { useLaunch } from "../../store/launch";
import type { GlobalConfig } from "../../store/types";

vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ isVisible: async () => false }) }));
vi.mock("../../platform/tauri", () => ({ invokeCommand: vi.fn().mockResolvedValue([]), emitEvent: vi.fn(), listenEvent: vi.fn() }));

beforeEach(() => {
  useAccounts.setState({ accounts: [] });
  useGlobalConfig.setState({ config: { app_language: "en-US", launch_groups: [] } as unknown as GlobalConfig, saving: false });
  useLaunch.setState({ launching: false, progress: {}, results: [], error: null, logs: [] });
});
afterEach(cleanup);

it("navigates the mini action menu with arrows and restores its trigger on Escape", () => {
  render(<MiniDashboard busy={false} pinned={false} onPin={vi.fn()} onExpand={vi.fn()}
    onConfigure={vi.fn()} onManageGroups={vi.fn()} onTasks={vi.fn()} onKillAll={vi.fn()}
    onAddAccount={vi.fn()} onRoomAutomation={vi.fn()} />);
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
