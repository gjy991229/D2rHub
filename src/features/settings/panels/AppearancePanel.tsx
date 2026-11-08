import { ArrowRight, Check } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "../../../components/ui/Button";
import { RangeSlider } from "../../../components/ui/RangeSlider";
import { Toggle } from "../../../components/ui/Toggle";
import type { ThemeKey } from "../../../store/theme";
import { THEME_OPTIONS } from "../../../store/themeCatalog";
import type { GlobalConfig } from "../../../store/types";

export interface AppearanceSettingsDraft {
  app_language: GlobalConfig["app_language"];
  theme: ThemeKey;
  main_opacity: number;
  font_scale: GlobalConfig["font_scale"];
  separate_game_taskbar_icons: boolean;
}

interface AppearancePanelProps {
  draft: AppearanceSettingsDraft;
  dirty: boolean;
  applying: boolean;
  onChange: (patch: Partial<AppearanceSettingsDraft>) => void;
  onApply: () => Promise<boolean>;
  onOpenOverlaySettings?: () => void;
  navigationSaving: boolean;
}

function sliderToPercent(value: number): number {
  return Math.round(10 + 0.009 * value * value);
}

function percentToSlider(percent: number): number {
  if (percent <= 10) return 0;
  return Math.round(100 * Math.sqrt((percent - 10) / 90));
}

export function AppearancePanel({
  draft,
  dirty,
  applying,
  onChange,
  onApply,
  onOpenOverlaySettings,
  navigationSaving,
}: AppearancePanelProps) {
  const en = draft.app_language === "en-US";
  const copy = en ? {
    pending: "Appearance changes not saved yet", applied: "Appearance saved and applied",
    pendingDescription: "Apply saves your changes and updates the appearance now. Leaving this page or closing Settings does the same.",
    appliedDescription: "The theme, opacity, and text size use your saved settings.", apply: "Apply appearance",
    language: "Interface language", languageDescription: "Changes the D2RHub interface. Game content and rune names stay the same.", languageLabel: "Interface display language",
    theme: "Main window theme", themeDescription: "Choose the colors and contrast that suit your surroundings.",
    opacity: "Main window background opacity", opacityDescription: "Adjusts the background while keeping text and controls clear.", opacityPercent: "Main window background opacity percentage",
    font: "Text size", fontDescription: "Navigation, forms, and status text scale together.",
    overlays: "Overlay appearance and visibility", overlayDescription: "Set overlay themes, background opacity, visibility, and positions independently.", overlaySettings: "Overlay settings",
    taskbar: "Separate game taskbar icons", taskbarDescription: "Give each account its own taskbar identity. Applies to windows launched or detected afterwards.", taskbarLabel: "Use a separate taskbar icon for each game account",
  } : {
    pending: "外观修改尚未保存", applied: "外观已保存并应用",
    pendingDescription: "点击应用会保存并立即更新外观；离开本页或关闭设置时也会保存并应用。", appliedDescription: "主题、透明度和字号使用当前已保存方案。", apply: "应用外观",
    language: "界面语言", languageDescription: "只影响 D2RHub 界面，不改变游戏内容和符文名称。", languageLabel: "软件界面显示语言",
    theme: "主程序窗口主题", themeDescription: "选择适合当前使用环境的明暗层级。",
    opacity: "主界面背景不透明度", opacityDescription: "仅调整背景层，文字和控件保持清晰。", opacityPercent: "主界面背景不透明度百分比",
    font: "字体大小", fontDescription: "导航、表单和状态文字会按同一比例调整。",
    overlays: "悬浮窗外观与显示", overlayDescription: "独立设置悬浮窗主题、背景不透明度、显示开关和窗口位置。", overlaySettings: "悬浮窗设置",
    taskbar: "游戏实例任务栏独立", taskbarDescription: "为每个账号使用独立任务栏标识；对之后启动或重新识别的窗口生效。", taskbarLabel: "让每个游戏账号使用独立任务栏图标",
  };
  const themeEnglish: Record<ThemeKey, { label: string; desc: string }> = {
    onyx: { label: "Dark", desc: "Black surfaces, high contrast" }, light: { label: "Light", desc: "Bright, clear, and quiet" },
    midnight: { label: "Midnight blue", desc: "Blue-gray with soft blue accents" }, forest: { label: "Forest green", desc: "Muted green with sage accents" },
    sand: { label: "Warm sand", desc: "Warm gray with soft brown accents" }, pink: { label: "Misty pink", desc: "Soft pink with muted rose accents" },
  };
  const sliderValue = percentToSlider(draft.main_opacity);
  const [opacityInput, setOpacityInput] = useState(String(draft.main_opacity));
  const opacityFocused = useRef(false);
  useEffect(() => {
    if (!opacityFocused.current) setOpacityInput(String(draft.main_opacity));
  }, [draft.main_opacity]);

  return (
    <div className="settings-content-grid appearance-settings-grid" data-i18n-skip>
      <div className="appearance-apply-strip" data-dirty={dirty ? "true" : undefined}>
        <div>
          <strong role="status">{applying ? (en ? "Saving and applying…" : "正在保存并应用…") : dirty ? copy.pending : copy.applied}</strong>
          <p>{dirty ? copy.pendingDescription : copy.appliedDescription}</p>
        </div>
        <Button
          variant="primary"
          size="md"
          loading={applying}
          disabled={!dirty}
          onClick={() => void onApply()}
        >
          <Check size={14} aria-hidden="true" />
          {copy.apply}
        </Button>
      </div>

      <section className="spatial-panel appearance-section appearance-language-section">
        <div className="appearance-section-heading">
          <h3>{copy.language}</h3>
          <p>{copy.languageDescription}</p>
        </div>
        <select
          aria-label={copy.languageLabel}
          value={draft.app_language || "zh-CN"}
          onChange={(event) => onChange({ app_language: event.target.value })}
          className="settings-input appearance-compact-control"
        >
          <option value="zh-CN">中文</option>
          <option value="en-US">English</option>
        </select>
      </section>

      <section className="spatial-panel appearance-section appearance-theme-section">
        <div className="appearance-section-heading">
          <h3>{copy.theme}</h3>
          <p>{copy.themeDescription}</p>
        </div>
        <div className="appearance-theme-options">
          {THEME_OPTIONS.map((option) => {
            const active = draft.theme === option.id;
            return (
              <button
                key={option.id}
                type="button"
                aria-pressed={active}
                className="appearance-theme-option"
                data-active={active ? "true" : undefined}
                onClick={() => onChange({ theme: option.id })}
              >
                <span className="appearance-theme-swatch" data-theme-preview={option.id} aria-hidden="true" />
                <span><strong>{en ? themeEnglish[option.id].label : option.label}</strong><small>{en ? themeEnglish[option.id].desc : option.desc}</small></span>
                {active && <Check size={14} aria-hidden="true" />}
              </button>
            );
          })}
        </div>
      </section>

      <section className="spatial-panel appearance-section appearance-opacity-section">
        <div className="appearance-section-heading appearance-section-heading-inline">
          <div>
            <h3>{copy.opacity}</h3>
            <p>{copy.opacityDescription}</p>
          </div>
          <label className="appearance-percent-input">
            <input
              type="number"
              aria-label={copy.opacityPercent}
              min={10}
              max={100}
              value={opacityInput}
              onFocus={() => { opacityFocused.current = true; }}
              onChange={event => setOpacityInput(event.target.value)}
              onBlur={() => {
                opacityFocused.current = false;
                const number = opacityInput.trim() ? Number(opacityInput) : NaN;
                const next = Number.isFinite(number) ? Math.max(10, Math.min(100, Math.round(number))) : draft.main_opacity;
                setOpacityInput(String(next));
                if (next !== draft.main_opacity) onChange({ main_opacity: next });
              }}
              onKeyDown={event => {
                if (event.key === "Enter") { event.preventDefault(); event.currentTarget.blur(); }
                if (event.key === "Escape" && opacityInput !== String(draft.main_opacity)) {
                  event.stopPropagation(); setOpacityInput(String(draft.main_opacity));
                }
              }}
            />
            <span>%</span>
          </label>
        </div>
        <RangeSlider
          aria-label={copy.opacity}
          min={0}
          max={100}
          value={sliderValue}
          onChange={(event) => onChange({ main_opacity: sliderToPercent(Number(event.target.value)) })}
          className="w-full"
        />
      </section>

      <section className="spatial-panel appearance-section appearance-font-section">
        <div className="appearance-section-heading">
          <h3>{copy.font}</h3>
          <p>{copy.fontDescription}</p>
        </div>
        <div className="appearance-font-options">
          {([
            { id: "small", label: en ? "Small" : "较小", sample: "Aa" },
            { id: "default", label: en ? "Default" : "默认", sample: "Aa" },
            { id: "large", label: en ? "Large" : "较大", sample: "Aa" },
          ] as const).map((option) => (
            <button
              key={option.id}
              type="button"
              aria-pressed={draft.font_scale === option.id}
              className="appearance-font-option"
              data-active={draft.font_scale === option.id ? "true" : undefined}
              onClick={() => onChange({ font_scale: option.id })}
            >
              <strong data-scale={option.id}>{option.sample}</strong>
              <span>{option.label}</span>
            </button>
          ))}
        </div>
      </section>

      {onOpenOverlaySettings && (
        <section className="spatial-panel appearance-section appearance-overlay-section">
          <div className="appearance-section-heading">
            <h3>{copy.overlays}</h3>
            <p>{copy.overlayDescription}</p>
          </div>
          <Button variant="secondary" size="md" disabled={applying || navigationSaving} onClick={onOpenOverlaySettings}>
            {copy.overlaySettings}
            <ArrowRight size={14} aria-hidden="true" />
          </Button>
        </section>
      )}

      <section className="spatial-panel appearance-section appearance-taskbar-section">
        <div className="appearance-section-heading">
          <h3>{copy.taskbar}</h3>
          <p>{copy.taskbarDescription}</p>
        </div>
        <Toggle
          checked={draft.separate_game_taskbar_icons}
          ariaLabel={copy.taskbarLabel}
          onChange={(value) => onChange({ separate_game_taskbar_icons: value })}
        />
      </section>
    </div>
  );
}
