import type { AccountMeta, LaunchGroup } from "../store/types";
import { inspectLaunchGroup, launchGroupNameExists, nextLaunchGroupName } from "./launchGroups";

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

function account(overrides: Partial<AccountMeta>): AccountMeta {
  return {
    id: "account-a",
    display_name: "账号 A",
    mod_args: "",
    created_at: "2026-08-30T00:00:00Z",
    last_launched_at: null,
    last_reset_at: null,
    initialized: true,
    order: 0,
    is_running: false,
    auth_mode: "token",
    region: "KR",
    ...overrides,
  };
}

const group: LaunchGroup = {
  id: "primary",
  name: "主力队",
  account_ids: ["account-a", "account-b"],
};

const accounts = [
  account({ id: "account-a", display_name: "账号 A", order: 2 }),
  account({ id: "account-b", display_name: "账号 B", order: 1 }),
];

const available = inspectLaunchGroup(group, accounts);
assert(available.can_launch, "a group with ready accounts is launchable");
assert(
  JSON.stringify(available.ordered_account_ids) === JSON.stringify(["account-b", "account-a"]),
  "group launch order follows the current account card order",
);

const unavailable = inspectLaunchGroup(group, [
  account({ id: "account-a", initialized: false }),
  account({ id: "account-b", auth_mode: "bnet", region: "KR" }),
]);
assert(!unavailable.can_launch, "a group is blocked when any member is unavailable");
assert(
  unavailable.issues.some(issue => issue.reason === "not_initialized")
    && unavailable.issues.some(issue => issue.reason === "token_migration"),
  "unavailable group members keep precise reasons",
);

const missing = inspectLaunchGroup(group, [account({ id: "account-a" })]);
assert(
  missing.issues.length === 1 && missing.issues[0].reason === "missing",
  "deleted group members are reported as missing instead of being launched partially",
);

const empty = inspectLaunchGroup({ id: "empty", name: "空组", account_ids: [] }, accounts);
assert(!empty.can_launch, "an empty group cannot launch");

const names: LaunchGroup[] = [
  { id: "one", name: "启动组 1", account_ids: ["account-a"] },
  { id: "farm", name: "Farm", account_ids: ["account-b"] },
];
assert(nextLaunchGroupName(names) === "启动组 2", "new groups receive the next unused default name");
assert(launchGroupNameExists(names, " farm "), "group name comparison ignores case and outer whitespace");
assert(!launchGroupNameExists(names, "Farm", "farm"), "editing a group does not conflict with its own name");
