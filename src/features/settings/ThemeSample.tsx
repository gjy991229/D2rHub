import type { ThemeKey } from "../../store/themeCatalog";

/** Render the actual theme tokens, including the relative contrast of its controls. */
export function ThemeSample({ theme, overlay }: { theme: ThemeKey; overlay: boolean }) {
  return <span className="theme-sample" data-theme={theme} data-overlay={overlay} aria-hidden="true">
    <span className="sample-window"><i /><b /><em /></span>
  </span>;
}
