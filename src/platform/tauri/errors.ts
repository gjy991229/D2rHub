export type TauriOperationKind = "command" | "event-listen" | "event-emit";

function formatOriginalError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return String(error);

  try {
    const serialized = JSON.stringify(error);
    if (serialized !== undefined) return serialized;
  } catch {
    // Fall through to the best-effort string conversion.
  }

  return String(error);
}

/**
 * Stable frontend error shape for all Tauri bridge failures.
 *
 * `toString()` intentionally preserves the backend's original display text so
 * existing toasts and error messages do not gain a migration-only prefix.
 */
export class TauriOperationError extends Error {
  readonly kind: TauriOperationKind;
  readonly operationName: string;
  readonly originalError: unknown;
  private readonly displayText: string;

  constructor(kind: TauriOperationKind, operationName: string, originalError: unknown) {
    const displayText = formatOriginalError(originalError);
    const message = originalError instanceof Error ? originalError.message : displayText;
    super(message);
    this.name = "TauriOperationError";
    this.kind = kind;
    this.operationName = operationName;
    this.originalError = originalError;
    this.displayText = displayText;
  }

  override toString(): string {
    return this.displayText;
  }
}

export function normalizeTauriError(
  kind: TauriOperationKind,
  operationName: string,
  error: unknown,
): TauriOperationError {
  if (error instanceof TauriOperationError) return error;
  return new TauriOperationError(kind, operationName, error);
}
