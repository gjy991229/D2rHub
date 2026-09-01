import { shouldApplyConfigCommandResponse } from "./configCommitOrdering";

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
}

const before = { theme: "light" };
assert(
  shouldApplyConfigCommandResponse(before, before),
  "a command response is a valid fallback when no event crossed the request",
);
assert(
  !shouldApplyConfigCommandResponse(before, { theme: "onyx" }),
  "a delayed command response cannot overwrite a newer committed event",
);

console.log("config commit ordering tests passed");
