import { MINI_OVERLAY_MIN_HEIGHT } from "./overlaySizing";

const g = globalThis as any;
const fs = g.process.getBuiltinModule("fs");
const path = g.process.getBuiltinModule("path");

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

export function runTests() {
  const capabilityPath = path.join(
    g.process.cwd(),
    "src-tauri",
    "capabilities",
    "default.json",
  );
  const capability = JSON.parse(fs.readFileSync(capabilityPath, "utf8")) as {
    windows?: string[];
    permissions?: string[];
  };
  const tauriConfigPath = path.join(g.process.cwd(), "src-tauri", "tauri.conf.json");
  const tauriConfig = JSON.parse(fs.readFileSync(tauriConfigPath, "utf8")) as {
    app?: { windows?: Array<{ label?: string; minHeight?: number }> };
  };
  const permissions = new Set(capability.permissions ?? []);

  assert(
    capability.windows?.includes("overlay") === true,
    "the overlay window is covered by the default capability",
  );
  assert(
    tauriConfig.app?.windows?.find((window) => window.label === "overlay")?.minHeight
      === MINI_OVERLAY_MIN_HEIGHT,
    "the native overlay minimum height matches the one-row layout minimum",
  );
  for (const permission of [
    "core:window:allow-set-size",
    "core:window:allow-set-min-size",
    "core:window:allow-set-max-size",
    "core:window:allow-set-resizable",
    "core:window:allow-start-dragging",
  ]) {
    assert(
      permissions.has(permission),
      `the overlay resize flow is allowed to call ${permission}`,
    );
  }
}

if (typeof g.process !== "undefined" && typeof g.process.argv !== "undefined") {
  try {
    runTests();
  } catch (error) {
    console.error(error);
    g.process.exit(1);
  }
}
