import type { OptionalModuleTabId } from "../settings/settingsRegistry";

const APPLICATION_DISCLOSURE_KEY = "d2rhub-disclosure-accepted-version";
const MODULE_DISCLOSURE_KEY = "d2rhub-disclosure-accepted-modules";

function readAcceptedModules(): OptionalModuleTabId[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(MODULE_DISCLOSURE_KEY) || "[]");
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((value): value is OptionalModuleTabId => (
      value === "overlays"
      || value === "pet"
      || value === "automation"
      || value === "room-automation"
    ));
  } catch {
    return [];
  }
}

export function hasAcceptedApplicationDisclosure(version: string): boolean {
  try {
    return localStorage.getItem(APPLICATION_DISCLOSURE_KEY) === version;
  } catch {
    return false;
  }
}

export function acceptApplicationDisclosure(version: string): void {
  try {
    localStorage.setItem(APPLICATION_DISCLOSURE_KEY, version);
  } catch {
    // Keep the in-memory acceptance for this session. If storage is unavailable,
    // the disclosure intentionally appears again on the next launch.
  }
}

export function hasAcceptedModuleDisclosure(module: OptionalModuleTabId): boolean {
  return readAcceptedModules().includes(module);
}

export function acceptModuleDisclosure(module: OptionalModuleTabId): void {
  try {
    localStorage.setItem(
      MODULE_DISCLOSURE_KEY,
      JSON.stringify(Array.from(new Set([...readAcceptedModules(), module]))),
    );
  } catch {
    // The module can still be added for this session. A later attempt may ask again.
  }
}
