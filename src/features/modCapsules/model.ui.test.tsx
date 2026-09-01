import { describe, expect, it } from "vitest";
import type { ModCapsulePool } from "../../store/types";
import {
  accountsMissingCapsuleFeature,
  compatibleCapsulesForAccount,
  ROOM_TOOLS_CAPSULE_FEATURE,
  selectedCapsuleForAccount,
} from "./model";

const pool: ModCapsulePool = {
  capsules: [
    {
      id: "cn:shared",
      edition: "CN",
      name: "Shared",
      feature_groups: [ROOM_TOOLS_CAPSULE_FEATURE],
      update_required: false,
      ready: true,
      assigned_account_ids: ["primary"],
    },
    {
      id: "global:shared",
      edition: "Global",
      name: "Shared",
      feature_groups: [],
      update_required: true,
      ready: false,
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
    expect(compatibleCapsulesForAccount(pool, "primary").map((capsule) => capsule.id)).toEqual(["cn:shared"]);
  });

  it("requires every room participant to select a room-tools capsule", () => {
    expect(accountsMissingCapsuleFeature(pool, ["primary", "follower"], ROOM_TOOLS_CAPSULE_FEATURE)).toEqual(["follower"]);
  });
});
