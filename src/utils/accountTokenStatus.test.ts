import { getAccountTokenStatus } from "./accountTokenStatus";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const hour = 60 * 60 * 1000;
const now = Date.parse("2026-08-31T12:00:00Z");

assert(!getAccountTokenStatus(null, now).expired, "missing reset time is not treated as an expired token");
assert(!getAccountTokenStatus("not-a-date", now).expired, "invalid legacy dates fail open like the previous card logic");
assert(getAccountTokenStatus(new Date(now - 720 * hour).toISOString(), now).expired, "a 30-day Battle.net token is expired");

const warning = getAccountTokenStatus(new Date(now - 700 * hour).toISOString(), now);
assert(warning.warning && !warning.expired && warning.label === "20h", "the final 48 hours show an expiry warning");

const healthy = getAccountTokenStatus(new Date(now - 24 * hour).toISOString(), now);
assert(!healthy.warning && healthy.label === "29d", "healthy Battle.net tokens show remaining days");
