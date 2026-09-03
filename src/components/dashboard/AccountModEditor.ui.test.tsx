import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { AccountMeta, ModCapsulePool } from "../../store/types";
import { AccountModEditor } from "./AccountModEditor";

const account: AccountMeta = {
  id: "account-1",
  display_name: "一号账号",
  mod_args: "",
  mod_list: [],
  created_at: "2026-08-31T00:00:00Z",
  last_launched_at: null,
  last_reset_at: null,
  initialized: true,
  order: 0,
  is_running: false,
};

const pool: ModCapsulePool = {
  generation: 1,
  scanned_at: "2026-09-02T00:00:00+08:00",
  capsules: [{
    id: "scan:cn:shared",
    edition: "CN",
    name: "Shared",
    origin: "scanned",
    launch_arguments: "-mod Shared -txt -assettestmode 1",
    default_launch_arguments: "-mod Shared -txt -assettestmode 1",
    feature_groups: ["audio_telemetry"],
    processed: true,
    source_eligible: true,
    update_required: false,
    ready: true,
    deletable: false,
    assigned_account_ids: [],
  }],
  accounts: [{
    account_id: account.id,
    account_name: account.display_name,
    edition: "CN",
    selected_capsule_id: null,
    legacy_mod_arguments: "",
    issue: null,
  }],
};

afterEach(cleanup);

describe("AccountModEditor", () => {
  it("selects a shared capsule while keeping original game as a menu option", async () => {
    const user = userEvent.setup();
    const onAssign = vi.fn(async () => pool);
    render(<AccountModEditor account={account} modCapsulePool={pool} onAssign={onAssign} />);

    await user.click(screen.getByTitle(/当前 Mod：原版/));
    expect(screen.getByRole("menuitemradio", { name: /原版游戏/ })).toBeTruthy();
    await user.click(screen.getByRole("menuitemradio", { name: /Shared/ }));

    await waitFor(() => expect(onAssign).toHaveBeenCalledWith(account.id, "scan:cn:shared"));
    expect(screen.queryByTitle("删除 Mod")).toBeNull();
  });

  it("routes additions to central Mod management", async () => {
    const user = userEvent.setup();
    const onOpenModManager = vi.fn();
    render(<AccountModEditor account={account} modCapsulePool={pool} onOpenModManager={onOpenModManager} />);

    await user.click(screen.getByTitle("前往 Mod 管理新增共享参数"));
    expect(onOpenModManager).toHaveBeenCalledWith("add", "CN");
  });
});
