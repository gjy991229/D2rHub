const g = globalThis as any;
const fs = g.process.getBuiltinModule("fs");
const path = g.process.getBuiltinModule("path");

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

function collectTypeScriptFiles(directory: string): string[] {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry: any) => {
    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) return collectTypeScriptFiles(fullPath);
    return /\.tsx?$/.test(entry.name) ? [fullPath] : [];
  });
}

const sourceRoot = path.join(g.process.cwd(), "src");

const directTauriImport = /from\s+["']@tauri-apps\/api\/(?:core|event)["']/;
const violations = collectTypeScriptFiles(sourceRoot)
  .map((file) => ({
    file,
    relative: path.relative(g.process.cwd(), file).replace(/\\/g, "/"),
  }))
  .filter(({ relative }) => !relative.startsWith("src/platform/tauri/"))
  .filter(({ file }) => directTauriImport.test(fs.readFileSync(file, "utf8")))
  .map(({ relative }) => relative);

assert(
  violations.length === 0,
  `Tauri core/event imports stay behind src/platform/tauri (violations: ${violations.join(", ") || "none"})`,
);

export {};
