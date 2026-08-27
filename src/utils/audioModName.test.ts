import { validateAudioModName } from "./audioModName";

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

export function runTests() {
  assert(validateAudioModName("MyAudioMod") === null, "accepts an ASCII Mod name");
  assert(validateAudioModName("my-audio_mod-2") === null, "accepts separators supported by the generator");
  assert(validateAudioModName("  MyAudioMod  ") === null, "validates the trimmed Mod name");
  assert(validateAudioModName("") !== null, "requires the user to name the generated Mod");
  assert(validateAudioModName("我的Mod") !== null, "rejects characters unsupported by the generator");
  assert(validateAudioModName("My Audio Mod") !== null, "rejects spaces unsupported by the generator");
  assert(validateAudioModName("CON") !== null, "rejects Windows reserved directory names");
  assert(validateAudioModName("a".repeat(129)) !== null, "enforces the generator length limit");
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
