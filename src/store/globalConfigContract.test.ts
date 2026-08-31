import {
  GLOBAL_CONFIG_FIELDS,
  GLOBAL_CONFIG_FIELDS_COVER_TYPESCRIPT,
} from "./globalConfigContract";

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
}

assert(GLOBAL_CONFIG_FIELDS_COVER_TYPESCRIPT, "the field list covers the TypeScript config");
assert(
  new Set(GLOBAL_CONFIG_FIELDS).size === GLOBAL_CONFIG_FIELDS.length,
  "the field list does not contain duplicates",
);
assert(GLOBAL_CONFIG_FIELDS.includes("launch_groups"), "launch groups are part of the contract");
assert(
  GLOBAL_CONFIG_FIELDS.includes("favorite_launch_group_ids"),
  "favorite launch groups are part of the contract",
);
console.log("global config TypeScript contract tests passed");
