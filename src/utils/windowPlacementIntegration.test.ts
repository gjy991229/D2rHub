const g = globalThis as any;
const fs = g.process.getBuiltinModule("fs");
const path = g.process.getBuiltinModule("path");

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

function source(...segments: string[]) {
  return (fs.readFileSync(path.join(g.process.cwd(), ...segments), "utf8") as string)
    .replace(/\r\n/g, "\n");
}

export function runTests() {
  const effects = source("src", "hooks", "useAppEffects.ts");
  const settings = [
    source("src", "components", "config", "SettingsCenter.tsx"),
    source("src", "features", "settings", "panels", "OverlayPanel.tsx"),
    source("src", "features", "settings", "panels", "PetPanel.tsx"),
  ].join("\n");
  const overlay = source("src", "pages", "Overlay.tsx");
  const cat = source("src", "pages", "BongoCatWindow.tsx");
  const native = source("src-tauri", "src", "lib.rs");
  const tray = source("src-tauri", "src", "tray.rs");
  const i18n = source("src", "i18n.tsx");

  assert(
    effects.includes("setAuxiliaryWindowVisible(entry.label, entry.enabled)")
      && !effects.includes("await overlayWin.show()")
      && !effects.includes("await catWin.show()"),
    "automatic overlay visibility is routed through the native safe-show service",
  );
  assert(
    overlay.includes('await restoreWindowPlacement("overlay", saved);')
      && overlay.includes('invokeCommand("save_window_placement"')
      && overlay.includes("userWindowMovePendingRef"),
    "overlay restore and explicit user moves use the versioned physical placement service",
  );
  assert(
    cat.includes('restoreWindowPlacement("bongo-cat", legacyGeometry)')
      && cat.includes("useWindowPlacementSave")
      && cat.includes("markPlacementInteraction"),
    "cat placement migrates legacy storage without treating automatic recovery as a user move",
  );
  assert(
    settings.includes('locateWindow("overlay"')
      && settings.includes('locateWindow("stats-overlay"')
      && settings.includes('locateWindow("bongo-cat"')
      && settings.includes('recoverAuxiliaryWindows("main")'),
    "settings expose per-window locate and one-click recovery for all overlays",
  );
  assert(
    i18n.includes('"桌面悬浮窗口": "Desktop Overlay Windows"')
      && i18n.includes('"邪恶区域播报窗口": "Terror Zone Broadcast"')
      && i18n.includes('"场景统计窗口": "Run Statistics"'),
    "window recovery controls follow the existing bilingual settings surface",
  );
  assert(
    tray.includes('"recover-overlays"')
      && tray.includes('recover_auxiliary_windows_for_app(app, "cursor")'),
    "the tray provides an emergency recovery path on the cursor display",
  );
  assert(
    native.includes("window_placement::restore_window_placement")
      && native.includes("window_placement::save_window_placement")
      && native.includes("window_placement::set_auxiliary_window_visible")
      && !native.includes("// 加载悬浮窗几何并应用"),
    "native startup no longer races the frontend with a second legacy overlay restore",
  );
}

runTests();
