import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { ModCapsulePool } from "../../../store/types";
import type { ModCapsuleController } from "../../modCapsules/useModCapsulePool";
import { ModCatalogManager } from "./ModCatalogManager";

const pool: ModCapsulePool = {
  generation: 3,
  scanned_at: "2026-09-02T00:00:00+08:00",
  capsules: [{
    id: "scan:cn:plain",
    edition: "CN",
    name: "Plain",
    origin: "scanned",
    launch_arguments: "-mod Plain -txt -assettestmode 1",
    default_launch_arguments: "-mod Plain -txt -assettestmode 1",
    feature_groups: [],
    processed: false,
    source_eligible: true,
    update_required: false,
    ready: true,
    deletable: false,
    assigned_account_ids: [],
  }, {
    id: "scan:cn:ready",
    edition: "CN",
    name: "Ready",
    origin: "scanned",
    launch_arguments: "-mod Ready -txt -assettestmode 1",
    default_launch_arguments: "-mod Ready -txt -assettestmode 1",
    feature_groups: ["audio_telemetry"],
    processed: true,
    source_eligible: true,
    update_required: false,
    ready: true,
    deletable: false,
    assigned_account_ids: [],
  }, {
    id: "scan:cn:death-exit",
    edition: "CN",
    name: "DeathExit",
    origin: "scanned",
    launch_arguments: "-mod DeathExit -txt -assettestmode 1",
    default_launch_arguments: "-mod DeathExit -txt -assettestmode 1",
    feature_groups: ["auto_exit_on_death"],
    auto_exit_on_death_enabled: true,
    processed: true,
    source_eligible: true,
    update_required: false,
    ready: true,
    deletable: false,
    assigned_account_ids: [],
  }],
  accounts: [],
};

function controller(overrides: Partial<ModCapsuleController> = {}): ModCapsuleController {
  return {
    pool,
    loading: false,
    assigningAccountId: null,
    error: null,
    refresh: vi.fn(async () => pool),
    scan: vi.fn(async () => pool),
    add: vi.fn(async () => pool),
    update: vi.fn(async () => pool),
    remove: vi.fn(async () => pool),
    setAutoExitOnDeathEnabled: vi.fn(async () => pool),
    assign: vi.fn(async () => pool),
    ...overrides,
  };
}

afterEach(cleanup);

describe("ModCatalogManager", () => {
  it("shows scanned folder presets as immutable shared entries", () => {
    render(<ModCatalogManager catalog={controller()} accounts={[]} onProcess={vi.fn()} />);

    expect(screen.getAllByText("游戏目录预设 · 名称由文件夹决定")).toHaveLength(3);
    const processed = screen.getByText("Ready").closest("article");
    expect(processed?.textContent).toContain("已加工");
    expect(processed?.textContent).toContain("声纹识别");
    expect(screen.getByRole("button", { name: "加工" })).toBeTruthy();
    expect(screen.queryByTitle("删除自定义参数")).toBeNull();
  });

  it("toggles death-exit on the concrete supported Mod row", async () => {
    const user = userEvent.setup();
    const setAutoExitOnDeathEnabled = vi.fn(async () => pool);
    render(<ModCatalogManager
      catalog={controller({ setAutoExitOnDeathEnabled })}
      accounts={[]}
      onProcess={vi.fn()}
    />);

    const toggle = screen.getByRole("switch", { name: "DeathExit 死亡自动退房" });
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    await user.click(toggle);
    expect(setAutoExitOnDeathEnabled).toHaveBeenCalledWith("scan:cn:death-exit", false);
    expect(screen.queryByRole("switch", { name: "Ready 死亡自动退房" })).toBeNull();
  });

  it("adds legacy or special arguments only as a central custom entry", async () => {
    const user = userEvent.setup();
    const add = vi.fn(async () => pool);
    render(<ModCatalogManager catalog={controller({ add })} accounts={[]} autoOpenAdd onProcess={vi.fn()} />);

    const input = screen.getByPlaceholderText(/-mod MyMod/);
    await user.type(input, "-mod Legacy -txt -custom{Enter}");
    expect(add).toHaveBeenCalledWith("CN", "-mod Legacy -txt -custom");
  });
});
