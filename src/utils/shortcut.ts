/**
 * Normalizes a shortcut key combination string to match the backend's expected format.
 * Expected modifier order: Ctrl -> Alt -> Shift -> Key
 * Note: Win/Meta/Cmd modifier is NOT supported (removed from the shortcut system).
 */
export function normalizeShortcut(combo: string): string {
  if (!combo) return "";
  const parts = combo.split("+");
  if (parts[parts.length - 1]?.trim() === ""
    && parts[parts.length - 2]?.trim().toLowerCase() === "num") {
    parts.splice(-2, 2, "Num+");
  }
  const modifiers = new Set<string>();
  let key = "";

  for (const p of parts) {
    const trimmed = p.trim();
    const lower = trimmed.toLowerCase();
    if (lower === "ctrl" || lower === "control") {
      modifiers.add("Ctrl");
    } else if (lower === "alt") {
      modifiers.add("Alt");
    } else if (lower === "shift") {
      modifiers.add("Shift");
    } else {
      if (trimmed.length === 1) {
        key = trimmed.toUpperCase();
      } else if (lower === "space") {
        key = "Space";
      } else {
        key = trimmed.charAt(0).toUpperCase() + trimmed.slice(1);
      }
    }
  }

  const result: string[] = [];
  if (modifiers.has("Ctrl")) result.push("Ctrl");
  if (modifiers.has("Alt")) result.push("Alt");
  if (modifiers.has("Shift")) result.push("Shift");
  if (key) result.push(key);

  return result.join("+");
}
