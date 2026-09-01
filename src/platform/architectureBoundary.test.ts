const g = globalThis as any;
const fs = g.process.getBuiltinModule("fs");
const path = g.process.getBuiltinModule("path");

type BoundaryRule = {
  id: string;
  label: string;
  pattern: RegExp;
};

type BoundaryViolation = {
  file: string;
  line: number;
  rule: BoundaryRule;
};

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

function collectRustFiles(directory: string): string[] {
  if (!fs.existsSync(directory)) return [];
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry: any) => {
    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) return collectRustFiles(fullPath);
    return entry.name.endsWith(".rs") ? [fullPath] : [];
  });
}

function blankNonNewline(characters: string[], index: number) {
  if (characters[index] !== "\n" && characters[index] !== "\r") {
    characters[index] = " ";
  }
}

/**
 * Masks comments and string literals while preserving offsets and line breaks.
 * Boundary rules inspect Rust paths, so prose and persisted string values must
 * not be mistaken for dependencies. This is intentionally a small lexer rather
 * than a Rust parser; it handles nested block comments and raw strings.
 */
function maskRustCommentsAndStrings(source: string): string {
  const masked = source.split("");
  let index = 0;

  while (index < source.length) {
    if (source.startsWith("//", index)) {
      let cursor = index;
      while (cursor < source.length && source[cursor] !== "\n") {
        blankNonNewline(masked, cursor);
        cursor += 1;
      }
      index = cursor;
      continue;
    }

    if (source.startsWith("/*", index)) {
      let cursor = index;
      let depth = 0;
      while (cursor < source.length) {
        if (source.startsWith("/*", cursor)) {
          blankNonNewline(masked, cursor);
          blankNonNewline(masked, cursor + 1);
          depth += 1;
          cursor += 2;
          continue;
        }
        if (source.startsWith("*/", cursor)) {
          blankNonNewline(masked, cursor);
          blankNonNewline(masked, cursor + 1);
          depth -= 1;
          cursor += 2;
          if (depth === 0) break;
          continue;
        }
        blankNonNewline(masked, cursor);
        cursor += 1;
      }
      index = cursor;
      continue;
    }

    let rawPrefixLength = 0;
    if (source[index] === "r") rawPrefixLength = 1;
    if ((source[index] === "b" || source[index] === "c") && source[index + 1] === "r") {
      rawPrefixLength = 2;
    }
    if (rawPrefixLength > 0) {
      let quote = index + rawPrefixLength;
      let hashCount = 0;
      while (source[quote] === "#") {
        hashCount += 1;
        quote += 1;
      }
      if (source[quote] === '"') {
        const closing = `"${"#".repeat(hashCount)}`;
        const closingIndex = source.indexOf(closing, quote + 1);
        const end = closingIndex < 0 ? source.length : closingIndex + closing.length;
        for (let cursor = index; cursor < end; cursor += 1) {
          blankNonNewline(masked, cursor);
        }
        index = end;
        continue;
      }
    }

    let quote = -1;
    if (source[index] === '"') quote = index;
    if ((source[index] === "b" || source[index] === "c") && source[index + 1] === '"') {
      quote = index + 1;
    }
    if (quote >= 0) {
      let cursor = index;
      while (cursor <= quote) {
        blankNonNewline(masked, cursor);
        cursor += 1;
      }
      while (cursor < source.length) {
        blankNonNewline(masked, cursor);
        if (source[cursor] === "\\") {
          cursor += 1;
          if (cursor < source.length) blankNonNewline(masked, cursor);
          cursor += 1;
          continue;
        }
        if (source[cursor] === '"') {
          cursor += 1;
          break;
        }
        cursor += 1;
      }
      index = cursor;
      continue;
    }

    index += 1;
  }

  return masked.join("");
}

function relativePath(file: string): string {
  return path.relative(g.process.cwd(), file).replace(/\\/g, "/");
}

function lineAt(source: string, index: number): number {
  return source.slice(0, index).split(/\r?\n/).length;
}

function findViolations(files: string[], rules: readonly BoundaryRule[]): BoundaryViolation[] {
  return files.flatMap((file) => {
    const source = maskRustCommentsAndStrings(fs.readFileSync(file, "utf8"));
    return rules.flatMap((rule) => {
      rule.pattern.lastIndex = 0;
      const match = rule.pattern.exec(source);
      return match
        ? [{ file: relativePath(file), line: lineAt(source, match.index), rule }]
        : [];
    });
  });
}

function describeViolations(violations: readonly BoundaryViolation[]): string {
  return violations.length === 0
    ? "none"
    : violations
      .map(({ file, line, rule }) => `${file}:${line} [${rule.id}] ${rule.label}`)
      .join(", ");
}

function crateModulePattern(moduleName: string): RegExp {
  return new RegExp(
    `\\b(?:crate|self|super(?:\\s*::\\s*super)*)\\s*::\\s*(?:${moduleName}\\b|\\{[^;]*\\b${moduleName}\\b)`,
  );
}

function stdModulePattern(modulePath: string): RegExp {
  const pathSegments = modulePath.split("::");
  const escapedPath = pathSegments
    .map((segment) => segment.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
    .join("\\s*::\\s*");
  const leaf = pathSegments[pathSegments.length - 1];
  const groupedParent = modulePath.includes("::")
    ? `${pathSegments
      .slice(0, -1)
      .map((segment) => segment.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
      .join("\\s*::\\s*")}\\s*::\\s*\\{[^;]*\\b${leaf}\\b`
    : null;
  return new RegExp(
    `\\bstd\\s*::\\s*(?:${escapedPath}\\b|\\{[^;]*\\b${escapedPath}\\b${groupedParent ? `|${groupedParent}` : ""})`,
  );
}

const rustRoot = path.join(g.process.cwd(), "src-tauri", "src");
const domainRoot = path.join(rustRoot, "domain");
const applicationRoot = path.join(rustRoot, "application");
const roomAutomationRoot = path.join(rustRoot, "capabilities", "room_automation");
const roomAutomationModule = path.join(rustRoot, "capabilities", "room_automation.rs");

const domainFiles = collectRustFiles(domainRoot);
const applicationFiles = collectRustFiles(applicationRoot);
const rustFiles = collectRustFiles(rustRoot);
const roomAutomationFiles = [
  ...collectRustFiles(roomAutomationRoot),
  ...(fs.existsSync(roomAutomationModule) ? [roomAutomationModule] : []),
];

assert(
  domainFiles.length > 0,
  "[RUST-DOMAIN-000] src-tauri/src/domain contains Rust sources",
);
assert(
  applicationFiles.length > 0,
  "[RUST-APP-000] src-tauri/src/application contains Rust sources",
);

const domainRules: readonly BoundaryRule[] = [
  { id: "RUST-DOMAIN-001", label: "Tauri dependency", pattern: /\btauri(?:_plugin_[a-zA-Z0-9_]+)?\s*(?:::|\bas\b|;)/ },
  { id: "RUST-DOMAIN-002", label: "command adapter dependency", pattern: crateModulePattern("commands") },
  { id: "RUST-DOMAIN-003", label: "capability implementation dependency", pattern: crateModulePattern("capabilities") },
  { id: "RUST-DOMAIN-004", label: "Windows adapter dependency", pattern: /\b(?:windows|windows_sys|winreg)\s*(?:::|\bas\b|;)/ },
];

const applicationRules: readonly BoundaryRule[] = [
  { id: "RUST-APP-001", label: "command adapter dependency", pattern: crateModulePattern("commands") },
  { id: "RUST-APP-002", label: "capability implementation dependency", pattern: crateModulePattern("capabilities") },
  { id: "RUST-APP-003", label: "infrastructure implementation dependency", pattern: crateModulePattern("infrastructure") },
  { id: "RUST-APP-004", label: "global application state dependency", pattern: crateModulePattern("state") },
  { id: "RUST-APP-005", label: "Tauri dependency", pattern: /\btauri(?:_plugin_[a-zA-Z0-9_]+)?\s*(?:::|\bas\b|;)/ },
  { id: "RUST-APP-006", label: "Windows crate dependency", pattern: /\b(?:windows|windows_sys|winreg)\s*(?:::|\bas\b|;)/ },
  { id: "RUST-APP-007", label: "std::fs dependency", pattern: stdModulePattern("fs") },
  { id: "RUST-APP-008", label: "std::process dependency", pattern: stdModulePattern("process") },
  { id: "RUST-APP-009", label: "std::os::windows dependency", pattern: stdModulePattern("os::windows") },
];

const roomAutomationRules: readonly BoundaryRule[] = [
  { id: "RUST-ROOM-001", label: "global state implementation dependency", pattern: crateModulePattern("state") },
  { id: "RUST-ROOM-002", label: "SharedState dependency", pattern: /\bSharedState\b/ },
  { id: "RUST-ROOM-003", label: "AppState dependency", pattern: /\bAppState\b/ },
  { id: "RUST-ROOM-004", label: "GlobalConfig dependency", pattern: /\bGlobalConfig\b/ },
  { id: "RUST-ROOM-005", label: "global config I/O lock access", pattern: /\bconfig_io\b/ },
  {
    id: "RUST-ROOM-006",
    label: "global config lock access",
    pattern: /\b(?:state|app_state|shared_state)\s*\.\s*config\s*\.\s*(?:read|write)\s*\(/,
  },
  { id: "RUST-ROOM-007", label: "command implementation dependency", pattern: crateModulePattern("commands") },
  { id: "RUST-ROOM-008", label: "audio module implementation dependency", pattern: crateModulePattern("audio_mod") },
  { id: "RUST-ROOM-009", label: "rune-audio implementation dependency", pattern: crateModulePattern("rune_audio") },
  {
    id: "RUST-ROOM-010",
    label: "direct background task spawn",
    pattern: /\b(?:(?:std\s*::\s*)?thread\s*::\s*(?:(?:spawn|scope|Builder)\b|\{[^;]*(?:\bspawn\b|\bscope\b|\bBuilder\b)[^;]*\}|\*)|(?:tokio\s*::\s*(?:task\s*::\s*)?|tauri\s*::\s*async_runtime\s*::\s*)(?:(?:spawn(?:_blocking|_local)?)\b|\{[^;]*\bspawn(?:_blocking|_local)?\b[^;]*\}|\*))(?=\s*(?:::|[;,(]))/,
  },
  {
    id: "RUST-ROOM-013",
    label: "concrete instance registry access",
    pattern: /\bInstanceRegistry\b|\.\s*instances\s*\(/,
  },
  {
    id: "RUST-ROOM-014",
    label: "instance registry mutation",
    pattern: /\.\s*(?:record_launched|record_discovered|record_launch_snapshot|remove_if_pid|reconcile_if_unchanged)\s*\(/,
  },
];

const configurationRules: readonly BoundaryRule[] = [
  {
    id: "RUST-CONFIG-001",
    label: "legacy global configuration I/O lock",
    pattern: /\bconfig_io\b/,
  },
  {
    id: "RUST-CONFIG-002",
    label: "direct global configuration cache lock access",
    pattern: /\.\s*config\s*\.\s*(?:read|write)\s*\(/,
  },
];

const domainViolations = findViolations(domainFiles, domainRules);
const applicationViolations = findViolations(applicationFiles, applicationRules);
const roomAutomationViolations = findViolations(roomAutomationFiles, roomAutomationRules);
const configurationViolations = findViolations(rustFiles, configurationRules);

assert(
  domainViolations.length === 0,
  `Rust domain stays independent of UI, command, capability, and OS adapters (violations: ${describeViolations(domainViolations)})`,
);
assert(
  applicationViolations.length === 0,
  `Rust application stays independent of commands, capabilities, infrastructure, global state, Tauri, and OS adapters (violations: ${describeViolations(applicationViolations)})`,
);
assert(
  roomAutomationViolations.length === 0,
  `Room automation uses application ports instead of global state, config locks, private modules, or direct task spawning (violations: ${describeViolations(roomAutomationViolations)})`,
);
assert(
  configurationViolations.length === 0,
  `Global configuration access stays behind ConfigurationRuntime snapshots and transactions (violations: ${describeViolations(configurationViolations)})`,
);

const domainConfigFile = path.join(domainRoot, "config.rs");
const domainConfigSource = maskRustCommentsAndStrings(fs.readFileSync(domainConfigFile, "utf8"));
const typedRoomRotationField = /\broom_rotation\b/.exec(domainConfigSource);
assert(
  typedRoomRotationField === null,
  typedRoomRotationField === null
    ? "[RUST-ROOM-015] room automation config stays in a versioned module sidecar"
    : `[RUST-ROOM-015] ${relativePath(domainConfigFile)}:${lineAt(domainConfigSource, typedRoomRotationField.index)} must not add room_rotation to the v9 global envelope`,
);

const stateFile = path.join(rustRoot, "state.rs");
const stateSource = maskRustCommentsAndStrings(fs.readFileSync(stateFile, "utf8"));
const publicConfigurationLock = /\bpub(?:\s*\([^)]*\))?\s+(?:config|config_io)\s*:/.exec(stateSource);
assert(
  publicConfigurationLock === null,
  publicConfigurationLock === null
    ? "[RUST-CONFIG-003] AppState does not expose raw configuration locks"
    : `[RUST-CONFIG-003] ${relativePath(stateFile)}:${lineAt(stateSource, publicConfigurationLock.index)} exposes a raw configuration lock`,
);

const configurationRuntimeFile = path.join(applicationRoot, "configuration.rs");
const configurationRuntimeSource = maskRustCommentsAndStrings(
  fs.readFileSync(configurationRuntimeFile, "utf8"),
);
const publicRuntimeLock = /\bpub(?:\s*\([^)]*\))?\s+(?:transaction|cache)\s*:/.exec(
  configurationRuntimeSource,
);
assert(
  publicRuntimeLock === null,
  publicRuntimeLock === null
    ? "[RUST-CONFIG-004] ConfigurationRuntime keeps its transaction and cache private"
    : `[RUST-CONFIG-004] ${relativePath(configurationRuntimeFile)}:${lineAt(configurationRuntimeSource, publicRuntimeLock.index)} exposes configuration synchronization internals`,
);

const systemCommandFile = path.join(rustRoot, "commands", "system.rs");
const systemAdapterFile = path.join(rustRoot, "infrastructure", "system.rs");
const systemCommandSource = maskRustCommentsAndStrings(fs.readFileSync(systemCommandFile, "utf8"));
const systemAdapterSource = maskRustCommentsAndStrings(fs.readFileSync(systemAdapterFile, "utf8"));
const systemCommandImplementation = /\b(?:windows|windows_sys|winreg|sysinfo)\s*::|\bstd\s*::\s*(?:process|thread)\s*::|\bunsafe\b/.exec(
  systemCommandSource,
);
assert(
  systemCommandImplementation === null,
  systemCommandImplementation === null
    ? "[RUST-SYSTEM-001] system commands are thin IPC delegates"
    : `[RUST-SYSTEM-001] ${relativePath(systemCommandFile)}:${lineAt(systemCommandSource, systemCommandImplementation.index)} contains operating-system implementation logic`,
);
const adapterCommandMacro = /#\s*\[\s*tauri\s*::\s*command\s*\]/.exec(systemAdapterSource);
assert(
  adapterCommandMacro === null,
  adapterCommandMacro === null
    ? "[RUST-SYSTEM-002] the system adapter is not an IPC command surface"
    : `[RUST-SYSTEM-002] ${relativePath(systemAdapterFile)}:${lineAt(systemAdapterSource, adapterCommandMacro.index)} exposes a Tauri command macro`,
);

const frontendConfigStoreFile = path.join(g.process.cwd(), "src", "store", "globalConfig.ts");
const frontendConfigStoreSource = fs.readFileSync(frontendConfigStoreFile, "utf8");
assert(
  !/emitEvent\s*\(\s*["']global-config-updated["']/.test(frontendConfigStoreSource),
  "[CONFIG-EVENT-001] only the backend transaction observer publishes global config commits",
);

const settingsCenterFile = path.join(g.process.cwd(), "src", "components", "config", "SettingsCenter.tsx");
const settingsCenterSource = fs.readFileSync(settingsCenterFile, "utf8");
const settingsCenterLines = settingsCenterSource.replace(/\r\n/g, "\n").split("\n").length;
assert(
  settingsCenterLines <= 900
    && settingsCenterSource.includes("useAudioModuleController")
    && settingsCenterSource.includes("useMaintenanceController")
    && settingsCenterSource.includes("useAuxiliaryWindowActions"),
  `[FRONTEND-SETTINGS-001] settings shell stays composition-only and below 900 lines (actual: ${settingsCenterLines})`,
);

for (const entryFile of [
  path.join(g.process.cwd(), "src", "App.tsx"),
  path.join(g.process.cwd(), "src", "pages", "Overlay.tsx"),
  path.join(g.process.cwd(), "src", "pages", "BongoCatWindow.tsx"),
]) {
  const entrySource = fs.readFileSync(entryFile, "utf8");
  assert(
    entrySource.includes("initConfigSync") && !entrySource.includes("initConfigListener"),
    `[CONFIG-EVENT-002] ${relativePath(entryFile)} subscribes before loading its config snapshot`,
  );
}

const legacyRoomFile = path.join(rustRoot, "room_rotation.rs");
assert(
  !fs.existsSync(legacyRoomFile),
  `[RUST-ROOM-011] ${relativePath(legacyRoomFile)} is forbidden; room automation belongs under capabilities/room_automation`,
);

const libFile = path.join(rustRoot, "lib.rs");
const libSource = maskRustCommentsAndStrings(fs.readFileSync(libFile, "utf8"));
const legacyRoomModule = /\b(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+room_rotation\s*;/.exec(libSource);
assert(
  legacyRoomModule === null,
  legacyRoomModule === null
    ? "[RUST-ROOM-012] lib.rs does not register a top-level room_rotation module"
    : `[RUST-ROOM-012] ${relativePath(libFile)}:${lineAt(libSource, legacyRoomModule.index)} top-level mod room_rotation is forbidden`,
);

const maskingFixture = maskRustCommentsAndStrings(`
// crate::commands::launch::launch_accounts();
/* windows::Win32 and /* nested SharedState */ AppState */
const NOTE: &str = "std::process::Command";
const RAW: &str = r#"crate::state::SharedState"#;
use crate::domain::config::GlobalConfig;
`);
assert(
  !/crate\s*::\s*commands|windows\s*::|SharedState|AppState|std\s*::\s*process/.test(maskingFixture)
    && /crate\s*::\s*domain/.test(maskingFixture),
  "[RUST-SCAN-001] boundary scanner ignores comments and strings while preserving Rust paths",
);

const applicationEscapeFixture = `
use super::super::commands::launch;
use std::{fs, process::Command};
use std::os::{windows};
`;
const applicationEscapeRuleIds = applicationRules
  .filter((rule) => {
    rule.pattern.lastIndex = 0;
    return rule.pattern.test(applicationEscapeFixture);
  })
  .map((rule) => rule.id);
assert(
  ["RUST-APP-001", "RUST-APP-007", "RUST-APP-008", "RUST-APP-009"]
    .every((ruleId) => applicationEscapeRuleIds.includes(ruleId)),
  "[RUST-SCAN-002] application rules catch parent paths and grouped std imports",
);

const groupedSpawnFixture = "use tokio::task::{spawn}; spawn(async {});";
const backgroundTaskRule = roomAutomationRules.find((rule) => rule.id === "RUST-ROOM-010")!;
backgroundTaskRule.pattern.lastIndex = 0;
assert(
  backgroundTaskRule.pattern.test(groupedSpawnFixture),
  "[RUST-SCAN-003] room automation rules catch grouped background-task imports",
);

const registryEscapeFixture = `
use crate::application::multi_instance::InstanceRegistry;
state.multi_instance().instances().record_launched("account", 42, "");
`;
const registryEscapeRuleIds = roomAutomationRules
  .filter((rule) => {
    rule.pattern.lastIndex = 0;
    return rule.pattern.test(registryEscapeFixture);
  })
  .map((rule) => rule.id);
assert(
  ["RUST-ROOM-013", "RUST-ROOM-014"]
    .every((ruleId) => registryEscapeRuleIds.includes(ruleId)),
  "[RUST-SCAN-004] room automation cannot access or mutate the concrete instance registry",
);

const configurationEscapeFixture = `
let _io = state.config_io.lock();
let config = shared_state
  .config
  .write();
`;
const configurationEscapeRuleIds = configurationRules
  .filter((rule) => {
    rule.pattern.lastIndex = 0;
    return rule.pattern.test(configurationEscapeFixture);
  })
  .map((rule) => rule.id);
assert(
  ["RUST-CONFIG-001", "RUST-CONFIG-002"]
    .every((ruleId) => configurationEscapeRuleIds.includes(ruleId)),
  "[RUST-SCAN-005] configuration rules catch the legacy I/O and cache locks",
);

export {};
