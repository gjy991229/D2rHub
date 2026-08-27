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
    app?: { windows?: Array<{ label?: string; url?: string; minWidth?: number; minHeight?: number }> };
  };
  const permissions = new Set(capability.permissions ?? []);

  assert(
    capability.windows?.includes("overlay") === true,
    "the TZ overlay window is covered by the default capability",
  );
  assert(
    capability.windows?.includes("stats-overlay") === true,
    "the statistics overlay window is covered by the default capability",
  );
  assert(
    tauriConfig.app?.windows?.find((window) => window.label === "overlay")?.minHeight
      === MINI_OVERLAY_MIN_HEIGHT
      && tauriConfig.app?.windows?.find((window) => window.label === "overlay")?.minWidth
        === 220,
    "the native TZ overlay minimums match its one-row content and horizontal floor",
  );
  const statsWindow = tauriConfig.app?.windows?.find((window) => window.label === "stats-overlay");
  assert(
    statsWindow?.url === "overlay.html"
      && statsWindow.minWidth === 220
      && statsWindow.minHeight === 180,
    "the statistics overlay has an independent native window with practical resize limits",
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
