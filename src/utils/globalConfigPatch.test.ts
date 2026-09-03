import type { GlobalConfig } from "../store/types";
import { diffGlobalConfig, hasGlobalConfigPatch } from "./globalConfigPatch";

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

const base = {
  version: 9,
  accounts_dir: "managed",
  legacy_path_migration: null,
  theme: "light",
  font_scale: "default",
  launch_groups: [],
} as unknown as GlobalConfig;

const current = {
  ...base,
  version: 10,
  accounts_dir: "stale-client-value",
  theme: "onyx",
  launch_groups: [{ id: "farm", name: "Farm", account_ids: ["a"] }],
};

const patch = diffGlobalConfig(base, current);
assert(
  JSON.stringify(patch) === JSON.stringify({
    theme: "onyx",
    launch_groups: [{ id: "farm", name: "Farm", account_ids: ["a"] }],
  }),
  "config diffs include only changed user fields and exclude server-managed fields",
);
assert(hasGlobalConfigPatch(patch), "a changed config produces a non-empty patch");
assert(!hasGlobalConfigPatch(diffGlobalConfig(base, { ...base })), "an unchanged config produces no patch");
