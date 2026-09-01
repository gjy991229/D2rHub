export const FRAMERATE_CAP_KEY = "Framerate Cap";
export const LEGACY_FRAMERATE_TARGET_KEY = "Framerate Target";

export type GameSettingsMap = Record<string, unknown>;

export function readFramerateCap(settings: GameSettingsMap, fallback: number): number {
  const value = Number(settings[FRAMERATE_CAP_KEY] ?? fallback);
  return Number.isFinite(value) ? value : fallback;
}

/**
 * Framerate Cap is the D2R setting used by every FPS control in D2RHub.
 * A legacy Target key is never created, but is kept in sync when an existing
 * Settings.json already contains it so external tooling cannot see a conflict.
 */
export function writeFramerateCap<T extends GameSettingsMap>(settings: T, value: number): T {
  const next: GameSettingsMap = { ...settings, [FRAMERATE_CAP_KEY]: value };
  if (Object.prototype.hasOwnProperty.call(settings, LEGACY_FRAMERATE_TARGET_KEY)) {
    next[LEGACY_FRAMERATE_TARGET_KEY] = value;
  }
  return next as T;
}
