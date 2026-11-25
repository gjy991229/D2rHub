import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ComponentProps } from "react";
import { useGlobalConfig } from "../../store/globalConfig";
import { useAccounts } from "../../store/accounts";
import type { GlobalConfig } from "../../store/types";
import type { LaunchGroupController } from "../../hooks/useLaunchGroupController";
import { MainActionBar } from "./MainActionBar";

beforeEach(() => {
  useGlobalConfig.setState({ config: null, saving: false });
  useAccounts.setState({ accounts: [] });
});
afterEach(cleanup);
const props: ComponentProps<typeof MainActionBar> = {
  launching: false, launchableAccountIds: ["a"],
  launchGroups: { draft: null, launch: vi.fn(), toggleFavorite: vi.fn() } as unknown as LaunchGroupController,
  onCancelLaunch: vi.fn(), onStartLaunch: vi.fn(), onAddAccount: vi.fn(), onRequestKillAll: vi.fn(),
  launchGroupPanelOpen: false, onToggleLaunchGroupPanel: vi.fn(), onOpenModManager: vi.fn(), onOpenRoomAutomation: vi.fn(),
};

describe("dashboard close action", () => {
  it("names the global action Close all and preserves the distinction from account-scoped closing", () => {
    const { rerender } = render(<MainActionBar {...props} />);
    expect(screen.getByRole("button", { name: "启动全部" })).toBeTruthy();
    const closeAll = screen.getByRole("button", { name: "关闭所有游戏" });
    expect(closeAll.title).toContain("包含未由 Hub 启动的游戏");
    fireEvent.click(closeAll);
    expect(props.onRequestKillAll).toHaveBeenCalledOnce();
    rerender(<MainActionBar {...props} batchSelection={{ mode: "close", ids: ["a", "b"] }} />);
    expect(screen.getByRole("button", { name: "关闭选中 (2)" }).title).toBe("仅关闭选中账号的游戏进程");
    expect(screen.queryByRole("button", { name: "启动全部" })).toBeNull();
    rerender(<MainActionBar {...props} batchSelection={{ mode: "launch", ids: ["a"] }} />);
    expect(screen.queryByRole("button", { name: "关闭所有游戏" })).toBeNull();
  });

  it("renders English close actions directly and prevents dispatch during uncertainty or execution", () => {
    useGlobalConfig.setState({ config: { app_language: "en-US", launch_groups: [] } as unknown as GlobalConfig });
    const { rerender } = render(<MainActionBar {...props} />);
    expect(screen.getByRole("button", { name: "Close all games" })).toBeTruthy();
    rerender(<MainActionBar {...props} batchUncertain batchSelection={{ mode: "close", ids: ["a"] }} />);
    expect(screen.getByRole("button", { name: "Close selected (1)" })).toHaveProperty("disabled", true);
    rerender(<MainActionBar {...props} batchBusy />);
    expect(screen.getByRole("button", { name: "Working…" })).toHaveProperty("disabled", true);
  });
});
