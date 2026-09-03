import { extractBattleNetToken } from "./battleNetToken";

const token = "CN-0123456789abcdef0123456789ABCDEF-a1B2c3D4e";
const usToken = "US-ABCDEF0123456789ABCDEF0123456789-Z9y8X7w6V";

function assertEqual(actual: string | null, expected: string | null, message: string) {
  if (actual !== expected) {
    throw new Error(`FAIL: ${message}\nExpected: ${expected}\nActual: ${actual}`);
  }
  console.log(`PASS: ${message}`);
}

export function runTests() {
  assertEqual(
    extractBattleNetToken(`http://localhost:0/?ST=${token}&flowTrackingId=example`),
    token,
    "extracts the ST value before the next ampersand",
  );
  assertEqual(
    extractBattleNetToken(`请复制整个链接：http://localhost:0/?foo=1&st=${token}&flowTrackingId=example，然后粘贴到软件里。`),
    token,
    "ignores Chinese instructions surrounding a complete URL",
  );
  assertEqual(
    extractBattleNetToken(`中文也可能被复制 ST=请忽略这些字${usToken}＆flowTrackingId=123 中文结尾`),
    usToken,
    "finds a complete token inside a noisy ST value and accepts a full-width ampersand",
  );
  assertEqual(
    extractBattleNetToken(`旧教程直接复制：${token}。`),
    token,
    "keeps bare-token paste compatible with the old guide",
  );
  assertEqual(
    extractBattleNetToken(`http://localhost:0/?LAST=${token}&flowTrackingId=example`),
    token,
    "still recovers a standalone complete token when ST is not present",
  );
  assertEqual(
    extractBattleNetToken("http://localhost:0/?ST=CN-incomplete&flowTrackingId=example"),
    null,
    "rejects an incomplete token",
  );
  assertEqual(
    extractBattleNetToken("http://localhost:0/?ST=CN-a-b&flowTrackingId=example"),
    null,
    "rejects a token-shaped value whose credential segments are too short",
  );
  assertEqual(
    extractBattleNetToken("http://localhost:0/?ST=XX-0123456789abcdef0123456789abcdef-a1b2c3d4e&flowTrackingId=example"),
    null,
    "rejects an unknown region prefix",
  );
  assertEqual(
    extractBattleNetToken(`http://localhost:0/?ST=${token}_truncated&flowTrackingId=example`),
    null,
    "does not accept a token embedded in a longer ASCII credential fragment",
  );
}

const g = globalThis as any;
if (typeof g.process !== "undefined" && typeof g.process.argv !== "undefined") {
  try {
    runTests();
  } catch (error) {
    console.error(error);
    g.process.exit(1);
  }
}
