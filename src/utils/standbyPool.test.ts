import type { AccountMeta } from "../store/types";
import {
  completeWorkspaceOrder,
  insertAccountId,
  moveIdWithinList,
  partitionAccountWorkspace,
  prioritizeWorkspaceCollisionIds,
  workspaceContainerForId,
} from "./standbyPool";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function account(id: string, order: number): AccountMeta {
  return {
    id,
    display_name: id,
    mod_args: "",
    created_at: "2026-01-01T00:00:00Z",
    last_launched_at: null,
    last_reset_at: null,
    initialized: true,
    order,
    is_running: false,
  };
}

const accounts = [account("a", 0), account("b", 1), account("c", 2)];
const partition = partitionAccountWorkspace(accounts, ["c", "missing", "c", "a"]);
assert(partition.active.map(item => item.id).join(",") === "b", "active cards exclude standby accounts");
assert(partition.standby.map(item => item.id).join(",") === "c,a", "standby order follows the global pool");
assert(partition.standbyIds.join(",") === "c,a", "missing and duplicate pool ids are ignored");
assert(
  partition.active.filter(item => item.initialized).map(item => item.id).join(",") === "b",
  "default launch candidates never include standby accounts",
);

assert(insertAccountId(["a", "c"], "b", "c").join(",") === "a,b,c", "cross-pool insertion respects the target card");
assert(insertAccountId(["a", "b"], "a").join(",") === "b,a", "insertion removes the previous occurrence");
assert(moveIdWithinList(["a", "b", "c"], "a", "c").join(",") === "b,c,a", "same-pool dragging reorders cards");
assert(completeWorkspaceOrder(["b", "a"], ["d", "c"]).join(",") === "b,a,d,c", "persisted order keeps both workspace sections complete");

const launchpadDropId = "launchpad";
const standbyDropId = "standby";
const activeIds = ["a", "b"];
const standbyIds = ["c", "d"];
assert(
  workspaceContainerForId("c", activeIds, standbyIds, launchpadDropId, standbyDropId) === "standby",
  "workspace items resolve to their owning container",
);
assert(
  prioritizeWorkspaceCollisionIds(
    "c",
    ["c", launchpadDropId],
    activeIds,
    standbyIds,
    launchpadDropId,
    standbyDropId,
  ).join(",") === launchpadDropId,
  "dragging out of the dock ignores the translated source item and reaches the launchpad",
);
assert(
  prioritizeWorkspaceCollisionIds(
    "c",
    [launchpadDropId, "b"],
    activeIds,
    standbyIds,
    launchpadDropId,
    standbyDropId,
  ).join(",") === `b,${launchpadDropId}`,
  "an active card is preferred over its launchpad parent for precise insertion",
);
assert(
  prioritizeWorkspaceCollisionIds(
    "a",
    [standbyDropId, "d"],
    activeIds,
    standbyIds,
    launchpadDropId,
    standbyDropId,
  ).join(",") === `d,${standbyDropId}`,
  "a dock card is preferred over the dock parent for precise insertion",
);
