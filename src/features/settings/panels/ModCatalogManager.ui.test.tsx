import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { ModCapsulePool } from "../../../store/types";
import type { ModCapsuleController } from "../../modCapsules/useModCapsulePool";
import { ModCatalogManager } from "./ModCatalogManager";
import { Modal } from "../../../components/ui/Modal";
vi.mock("../../../platform/tauri", () => ({ invokeCommand: vi.fn(async () => ({
  catalog: { assets: [] }, processor: { ready: true },
})) }));

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
    unpackingCapsuleId: null, unpackProgress: null, unpackResult: null, cancelUnpack: vi.fn(async () => {}),
    unpack: vi.fn(async () => ({ pool, backup_path: null, escaped_name_count: 0 })),
    setAutoExitOnDeathEnabled: vi.fn(async () => pool),
    assign: vi.fn(async () => pool),
    ...overrides,
  };
}

afterEach(cleanup);

describe("ModCatalogManager", () => {
  it("opens downloads as a sibling page without nesting another modal", async () => {
    render(<ModCatalogManager catalog={controller()} accounts={[]} onProcess={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "下载与更新" }));
    expect(screen.getByRole("region", { name: "Mod 资源下载" })).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByText("Plain")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "已安装" }));
    expect(screen.getByText("Plain")).toBeTruthy();
  });
  it("allows downloading into an edition that has no installed Mods", async () => {
    render(<ModCatalogManager catalog={controller()} accounts={[]} onProcess={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "下载与更新" }));
    await userEvent.click(screen.getByRole("tab", { name: "国际服" }));
    expect(screen.getByRole("tab", { name: "国际服" }).getAttribute("aria-selected")).toBe("true");
  });
  it("preserves an empty edition through rescans and downloads navigation", async () => {
    const catalog = controller();
    const { rerender } = render(<ModCatalogManager catalog={catalog} accounts={[]} onProcess={vi.fn()} />);
    await userEvent.click(screen.getByRole("tab", { name: "国际服" }));
    expect(screen.getByText("没有扫描到 国际服 Mod")).toBeTruthy();
    rerender(<ModCatalogManager catalog={{ ...catalog, pool: { ...pool, generation: 4 } }} accounts={[]} onProcess={vi.fn()} />);
    expect(screen.getByRole("tab", { name: "国际服" }).getAttribute("aria-selected")).toBe("true");
    await userEvent.click(screen.getByRole("button", { name: "下载与更新" }));
    await userEvent.click(screen.getByRole("button", { name: "已安装" }));
    expect(screen.getByText("没有扫描到 国际服 Mod")).toBeTruthy();
  });
  it("respects an explicitly requested edition with no installed Mods", () => {
    render(<ModCatalogManager initialEdition="Global" catalog={controller()} accounts={[]} onProcess={vi.fn()} />);
    expect(screen.getByRole("tab", { name: "国际服" }).getAttribute("aria-selected")).toBe("true");
  });
  it("edits only the expanded preset and passes its identity to the catalog", async () => {
    const update = vi.fn(async () => pool);
    render(<ModCatalogManager catalog={controller({ update })} accounts={[]} onProcess={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "设置 Plain" }));
    await userEvent.click(screen.getByRole("button", { name: "编辑" }));
    const input = screen.getByRole("textbox", { name: "共享启动参数" });
    await userEvent.clear(input);
    await userEvent.type(input, "-mod Plain -txt -w");
    await userEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(update).toHaveBeenCalledWith("scan:cn:plain", "-mod Plain -txt -w");
  });
  it("shows readable Mod summaries with advanced actions disclosed on demand", () => {
    render(<ModCatalogManager catalog={controller()} accounts={[]} onProcess={vi.fn()} />);

    expect(screen.getByText("源 Mod")).toBeTruthy();
    expect(screen.getAllByText(/加工成品/)).toHaveLength(2);
    const processed = screen.getByText("Ready").closest("article");
    expect(processed?.textContent).toContain("原版");
    expect(processed?.textContent).toContain("声纹识别");
    expect(screen.queryByRole("button", { name: "加工功能" })).toBeNull();
    expect(screen.getAllByRole("button", { name: /^设置 / })).toHaveLength(3);
    expect(screen.queryByTitle("删除自定义参数")).toBeNull();
  });

  it("opens processing for the exact outdated Mod without expanding settings", async () => {
    const onProcess = vi.fn();
    const capsules = pool.capsules.map(capsule => ({ ...capsule, update_required: capsule.name === "Ready" }));
    render(<ModCatalogManager catalog={controller({ pool: { ...pool, capsules } })} accounts={[]} onProcess={onProcess} />);
    expect(screen.getAllByRole("button", { name: "更新" })).toHaveLength(1);
    await userEvent.click(within(screen.getByRole("article", { name: "Ready" })).getByRole("button", { name: "更新" }));
    expect(onProcess).toHaveBeenCalledWith(capsules[1]);
  });

  it("toggles death-exit on the concrete supported Mod row", async () => {
    const user = userEvent.setup();
    const setAutoExitOnDeathEnabled = vi.fn(async () => pool);
    render(<ModCatalogManager
      catalog={controller({ setAutoExitOnDeathEnabled })}
      accounts={[]}
      onProcess={vi.fn()}
    />);

    await user.click(screen.getByRole("button", { name: "设置 DeathExit" }));
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

  it("submits a pending custom preset only once when Enter is repeated", async () => {
    let finish!: (result: ModCapsulePool) => void;
    const add = vi.fn(() => new Promise<ModCapsulePool>(resolve => { finish = resolve; }));
    const user = userEvent.setup();
    render(<ModCatalogManager catalog={controller({ add })} accounts={[]} autoOpenAdd onProcess={vi.fn()} />);

    const input = screen.getByRole("textbox", { name: "自定义共享参数" }) as HTMLInputElement;
    await user.type(input, "-mod Legacy -txt");
    await user.keyboard("{Enter}{Enter}{Enter}");
    expect(add).toHaveBeenCalledTimes(1);
    expect(add).toHaveBeenCalledWith("CN", "-mod Legacy -txt");
    expect(input.disabled).toBe(true);
    expect((screen.getByRole("button", { name: "保存" }) as HTMLButtonElement).disabled).toBe(true);

    await act(async () => finish(pool));
    expect(screen.queryByRole("textbox", { name: "自定义共享参数" })).toBeNull();
  });

  it("cancels a custom argument draft on Escape without closing the settings dialog", async () => {
    const onClose = vi.fn();
    render(<Modal open title="设置中心" onClose={onClose}>
      <ModCatalogManager catalog={controller()} accounts={[]} autoOpenAdd onProcess={vi.fn()} />
    </Modal>);
    const input = screen.getByRole("textbox", { name: "自定义共享参数" });
    fireEvent.keyDown(input, { key: "Escape" });

    expect(screen.queryByRole("textbox", { name: "自定义共享参数" })).toBeNull();
    expect(screen.getByRole("dialog", { name: "设置中心" })).toBeTruthy();
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("gives Unicode Mod names distinct disclosure targets and argument labels", async () => {
    const unicodePool = { ...pool, capsules: ["甲", "乙"].map(name => ({
      ...pool.capsules[0], id: `scan:cn:${name}`, name, launch_arguments: `-mod ${name} -txt`,
    })) };
    render(<ModCatalogManager catalog={controller({ pool: unicodePool })} accounts={[]} onProcess={vi.fn()} />);
    const targets: string[] = [];
    for (const name of ["甲", "乙"]) {
      const row = screen.getByRole("article", { name });
      const disclosure = within(row).getByRole("button", { name: `设置 ${name}` });
      await userEvent.click(disclosure);
      const target = disclosure.getAttribute("aria-controls")!;
      targets.push(target);
      expect(row.contains(document.getElementById(target))).toBe(true);
      await userEvent.click(within(row).getByRole("button", { name: "编辑" }));
      expect((within(row).getByRole("textbox", { name: "共享启动参数" }) as HTMLInputElement).value).toBe(`-mod ${name} -txt`);
    }
    expect(new Set(targets).size).toBe(2);
    expect(new Set(screen.getAllByRole("textbox", { name: "共享启动参数" }).map(input => input.id)).size).toBe(2);
  });
});
