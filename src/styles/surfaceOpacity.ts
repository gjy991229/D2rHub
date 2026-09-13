import type { CSSProperties } from "react";
import type { ThemeKey } from "../store/theme";
import { isDarkTheme } from "../store/themeCatalog";

type SurfaceKey = "base" | "glass" | "card" | "hover" | "active";

const SURFACE_ALPHA: Record<"onyx" | "light", Record<SurfaceKey, number>> = {
  onyx: {
    base: 1,
    glass: 1,
    card: 1,
    hover: 0.12,
    active: 0.18,
  },
  light: {
    base: 1,
    glass: 0.95,
    card: 0.98,
    hover: 0.04,
    active: 0.07,
  },
};

function clampOpacity(percent: number | undefined): number {
  if (typeof percent !== "number" || !Number.isFinite(percent)) return 0.95;
  return Math.max(10, Math.min(100, percent)) / 100;
}

function fmtAlpha(value: number): string {
  return Math.max(0, Math.min(1, value)).toFixed(3);
}

export function surfaceOpacityVars(
  opacityPercent: number | undefined,
  theme: ThemeKey,
): CSSProperties {
  const opacity = clampOpacity(opacityPercent);
  const alpha = SURFACE_ALPHA[isDarkTheme(theme) ? "onyx" : "light"];
  const modalAlpha = isDarkTheme(theme)
    ? Math.max(0.86, alpha.glass * opacity)
    : Math.max(0.82, alpha.glass * opacity);

  return {
    "--surface-base": `rgb(var(--surface-base-rgb) / ${fmtAlpha(alpha.base * opacity)})`,
    "--surface-glass": `rgb(var(--surface-glass-rgb) / ${fmtAlpha(alpha.glass * opacity)})`,
    "--surface-card": `rgb(var(--surface-card-rgb) / ${fmtAlpha(alpha.card * opacity)})`,
    "--surface-hover": `rgb(var(--surface-hover-rgb) / ${fmtAlpha(alpha.hover * opacity)})`,
    "--surface-active": `rgb(var(--surface-active-rgb) / ${fmtAlpha(alpha.active * opacity)})`,
    "--surface-tile": `rgb(var(--surface-card-rgb) / ${fmtAlpha(Math.min(1, (alpha.card + 0.06) * opacity))})`,
    "--surface-tile-soft": `rgb(var(--surface-card-rgb) / ${fmtAlpha(Math.min(1, (alpha.card - 0.08) * opacity))})`,
    "--surface-control": `rgb(var(--surface-glass-rgb) / ${fmtAlpha(Math.min(1, 0.5 * opacity))})`,
    "--surface-modal": `rgb(var(--surface-glass-rgb) / ${fmtAlpha(modalAlpha)})`,
  } as CSSProperties;
}
