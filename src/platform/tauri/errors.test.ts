import { normalizeTauriError, TauriOperationError } from "./errors";

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

const backendText = "配置文件不可写";
const normalized = normalizeTauriError("command", "save_global_config", backendText);

assert(normalized instanceof TauriOperationError, "Tauri failures share one typed error shape");
assert(normalized.kind === "command", "normalized errors retain the operation kind");
assert(
  normalized.operationName === "save_global_config",
  "normalized errors retain the command or event name",
);
assert(
  String(normalized) === backendText,
  "normalization preserves legacy user-facing backend error text",
);
assert(
  normalizeTauriError("command", "save_global_config", normalized) === normalized,
  "normalization is idempotent",
);
