import { describe, expect, it } from "vitest";
import type { ModCapsulePool } from "../../store/types";
import {
  accountsMissingCapsuleFeature,
  AUTO_EXIT_ON_DEATH_CAPSULE_FEATURE,
  capsuleFeatureLabels,
  compatibleCapsulesForAccount,
  ROOM_TOOLS_CAPSULE_FEATURE,
  selectedCapsuleForAccount,
} from "./model";

const pool: ModCapsulePool = {
  generation: 1,
  scanned_at: "2026-09-02T00:00:00+08:00",
  capsules: [
    {
      id: "cn:shared",
      edition: "CN",
      name: "Shared",
      origin: "scanned",
      launch_arguments: "-mod Shared -txt -assettestmode 1",
      default_launch_arguments: "-mod Shared -txt -assettestmode 1",
      feature_groups: [ROOM_TOOLS_CAPSULE_FEATURE, AUTO_EXIT_ON_DEATH_CAPSULE_FEATURE],
      auto_exit_on_death_enabled: false,
      processed: true,
      source_eligible: true,
      update_required: false,
      ready: true,
      deletable: false,
      assigned_account_ids: ["primary"],
    },
    {
      id: "cn:plain",
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
    },
    {
      id: "global:shared",
      edition: "Global",
      name: "Shared",
      origin: "scanned",
      launch_arguments: "-mod Shared -txt -assettestmode 1",
      default_launch_arguments: "-mod Shared -txt -assettestmode 1",
      feature_groups: [],
      processed: false,
      source_eligible: false,
      update_required: true,
      ready: false,
      deletable: false,
      assigned_account_ids: [],
    },
  ],
  accounts: [
    { account_id: "primary", account_name: "主号", edition: "CN", selected_capsule_id: "cn:shared", legacy_mod_arguments: "-mod Shared -txt", issue: null },
    { account_id: "follower", account_name: "小号", edition: "CN", selected_capsule_id: null, legacy_mod_arguments: "", issue: null },
  ],
};

describe("shared Mod capsule pool", () => {
  it("imports an old account selection by its derived capsule id", () => {
    expect(selectedCapsuleForAccount(pool, "primary")?.name).toBe("Shared");
  });

  it("only offers ready capsules from the account edition", () => {
    expect(compatibleCapsulesForAccount(pool, "primary").map((capsule) => capsule.id)).toEqual(["cn:shared", "cn:plain"]);
  });

  it("requires every room participant to select a room-tools capsule", () => {
    expect(accountsMissingCapsuleFeature(pool, ["primary", "follower"], ROOM_TOOLS_CAPSULE_FEATURE)).toEqual(["follower"]);
  });

  it("distinguishes an installed but disabled death-exit capability", () => {
    expect(capsuleFeatureLabels(pool.capsules[0])).toEqual([
      "局内房间工具",
      "死亡自动退房（已停用）",
    ]);
  });
});
