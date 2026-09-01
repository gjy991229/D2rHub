const g = globalThis as any;
const fs = g.process.getBuiltinModule("fs");
const path = g.process.getBuiltinModule("path");

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

function collectRustFiles(directory: string): string[] {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry: any) => {
    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) return collectRustFiles(fullPath);
    return entry.name.endsWith(".rs") ? [fullPath] : [];
  });
}

const domainRoot = path.join(g.process.cwd(), "src-tauri", "src", "domain");
const forbiddenDomainDependencies = [
  { label: "Tauri", pattern: /\btauri(?:::|\s*=)/ },
  { label: "command adapters", pattern: /crate::commands\b/ },
  { label: "capability implementations", pattern: /crate::capabilities\b/ },
  { label: "Windows adapters", pattern: /windows(?:_sys)?::|windows-sys\b/ },
];

const violations = collectRustFiles(domainRoot).flatMap((file) => {
  const source = fs.readFileSync(file, "utf8");
  return forbiddenDomainDependencies
    .filter(({ pattern }) => pattern.test(source))
    .map(({ label }) => `${path.relative(g.process.cwd(), file).replace(/\\/g, "/")} -> ${label}`);
});

assert(
  violations.length === 0,
  `Rust domain stays independent of UI, command, capability, and OS adapters (violations: ${violations.join(", ") || "none"})`,
);

export {};
