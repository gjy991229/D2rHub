import { Check, LocateFixed, MonitorUp } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "../../../components/ui/Button";
import { RangeSlider } from "../../../components/ui/RangeSlider";
import { Toggle } from "../../../components/ui/Toggle";
import { THEME_OPTIONS } from "../../../store/themeCatalog";
import type { GlobalConfig } from "../../../store/types";
import { type AuxiliaryWindowLabel } from "../../../utils/windowPlacement";

interface OverlayPanelProps {
  config: GlobalConfig;
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
  persistConfig: (draft: GlobalConfig, quiet?: boolean) => Promise<unknown>;
  windowPlacementBusy: string | null;
  locateWindow: (label: AuxiliaryWindowLabel) => Promise<void>;
  recoverAllWindows: () => Promise<void>;
}

function sliderToPercent(value: number): number {
  return Math.round(10 + 0.009 * value * value);
}

function percentToSlider(percent: number): number {
  if (percent <= 10) return 0;
  return Math.round(100 * Math.sqrt((percent - 10) / 90));
}

export function OverlayPanel({
  config,
  updateConfig,
  persistConfig: persistGlobalDraft,
  windowPlacementBusy,
  locateWindow,
  recoverAllWindows,
}: OverlayPanelProps) {
  const en = config.app_language === "en-US";
  const statisticsModuleInstalled = config.installed_optional_modules?.includes("automation") === true;
  const [opacityInput, setOpacityInput] = useState(String(config.overlay_opacity ?? 95));
  const opacityFocused = useRef(false);
  useEffect(() => {
    if (!opacityFocused.current) setOpacityInput(String(config.overlay_opacity ?? 95));
  }, [config.overlay_opacity]);
  return (
    <div className="settings-content-grid overlay-settings">
      <div className="spatial-panel p-3 space-y-2">
        <h3 className="text-sm font-semibold text-text-primary">{en ? "Overlay theme" : "悬浮窗主题"}</h3>
        <div className="appearance-theme-options">
          {THEME_OPTIONS.map((option) => {
            const active = (config.theme_overlay || "light") === option.id;
            return (
              <button
                key={option.id}
                type="button"
                aria-pressed={active}
                onClick={async () => {
                  updateConfig((current) => { current.theme_overlay = option.id; });
                  const current = config;
                  if (current) {
                    await persistGlobalDraft({ ...current, theme_overlay: option.id }, true);
                  }
                }}
                className="appearance-theme-option"
                data-active={active ? "true" : undefined}
              >
                <span className="appearance-theme-swatch" data-theme-preview={option.id} aria-hidden="true" />
                <span><strong>{option.label}</strong><small>{option.desc}</small></span>
                {active && <Check size={14} aria-hidden="true" />}
              </button>
            );
          })}
        </div>
      </div>

      <div className="spatial-panel p-3 space-y-2">
        <h3 className="text-sm font-semibold text-text-primary">{en ? "Background opacity" : "背景不透明度"}</h3>
        <div className="space-y-1">
          <div className="flex justify-between items-center text-xs">
            <span className="font-semibold text-text-secondary">信息悬浮窗背景不透明度</span>
            <div className="flex items-center gap-1.5">
              <input
                type="number"
                aria-label="信息悬浮窗背景不透明度百分比"
                min={10}
                max={100}
                value={opacityInput}
                onFocus={() => { opacityFocused.current = true; }}
                onChange={event => setOpacityInput(event.target.value)}
                onBlur={() => {
                  opacityFocused.current = false;
                  const value = opacityInput.trim() === "" ? config.overlay_opacity ?? 95 : Math.max(10, Math.min(100, Number(opacityInput)));
                  setOpacityInput(String(value));
                  updateConfig((current) => { current.overlay_opacity = value; });
                }}
                onKeyDown={event => {
                  if (event.key === "Enter") { event.preventDefault(); event.currentTarget.blur(); }
                  if (event.key === "Escape") { event.stopPropagation(); setOpacityInput(String(config.overlay_opacity ?? 95)); }
                }}
                className="w-16 px-2 rounded-lg bg-surface-control text-center font-mono text-xs text-text-primary border border-border-default"
              />
              <span className="text-text-muted">%</span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <RangeSlider
              aria-label="信息悬浮窗背景不透明度"
              min={0}
              max={100}
              value={percentToSlider(config.overlay_opacity ?? 95)}
              onChange={(event) => {
                const value = sliderToPercent(parseInt(event.target.value));
                updateConfig((current) => { current.overlay_opacity = value; });
              }}
              className="min-w-0 flex-1"
            />
          </div>
          <p className="text-2xs text-text-muted">
            {en ? "Drag the slider or enter 10%–100%. Text and controls stay clear." : "拖动滑块或输入 10%–100%；仅背景变化，文字和控件保持清晰。"}
          </p>
        </div>
      </div>

      <div className="spatial-panel p-3 space-y-2">
        <h3 className="text-sm font-semibold text-text-primary">{en ? "Visibility & placement" : "显示与位置"}</h3>
        <div className="flex items-center justify-between gap-4 py-1">
          <div>
            <span className="text-sm font-semibold text-text-secondary">邪恶区域播报窗口</span>
            <p className="text-2xs text-text-muted">独立显示当前与下一轮 TZ；支持迷你模式和贴边隐藏</p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              loading={windowPlacementBusy === "overlay"}
              disabled={!config.enable_tz_overlay || windowPlacementBusy !== null}
              onClick={() => locateWindow("overlay")}
              title="将窗口移到主界面所在屏幕"
            >
              <LocateFixed size={12} />
              定位
            </Button>
            <Toggle
              checked={!!config.enable_tz_overlay}
              ariaLabel="显示邪恶区域播报悬浮窗"
              onChange={async (visible) => {
                updateConfig((current) => {
                  current.enable_tz_overlay = visible;
                  current.enable_overlay = visible || current.enable_stats_overlay;
                });
                const current = config;
                if (current) {
                  await persistGlobalDraft({
                    ...current,
                    enable_tz_overlay: visible,
                    enable_overlay: visible || current.enable_stats_overlay,
                  }, true);
                }
              }}
            />
          </div>
        </div>

        <div className="flex items-center justify-between gap-4 border-t border-border-default/50 py-2">
          <div>
            <span className="text-sm font-semibold text-text-secondary">场景统计窗口</span>
            <p className="text-2xs text-text-muted">
              {statisticsModuleInstalled
                ? (en ? "Show scene timing and drops. Double-click blank space for mini mode; drag to move or resize." : "显示场景计时和掉落；双击空白处切换迷你模式，可拖动位置和边缘缩放。")
                : (en ? "Add Recognition & Stats to show scene timing and drops." : "添加“识别与统计”后，即可显示场景计时和掉落。")}
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              loading={windowPlacementBusy === "stats-overlay"}
              disabled={!statisticsModuleInstalled || !config.enable_stats_overlay || windowPlacementBusy !== null}
              onClick={() => locateWindow("stats-overlay")}
              title="将窗口移到主界面所在屏幕"
            >
              <LocateFixed size={12} />
              定位
            </Button>
            <Toggle
              checked={!!config.enable_stats_overlay}
              disabled={!statisticsModuleInstalled}
              ariaLabel="显示场景统计悬浮窗"
              onChange={async (visible) => {
                updateConfig((current) => {
                  current.enable_stats_overlay = visible;
                  current.enable_overlay = current.enable_tz_overlay || visible;
                });
                const current = config;
                if (current) {
                  await persistGlobalDraft({
                    ...current,
                    enable_stats_overlay: visible,
                    enable_overlay: current.enable_tz_overlay || visible,
                  }, true);
                }
              }}
            />
          </div>
        </div>

        <div className="flex items-center justify-between gap-4 border-t border-border-default/50 pt-2">
          <p className="min-w-0 text-2xs leading-relaxed text-text-muted">
            显示器布局变化时会自动保证窗口可见；也可以将已启用的悬浮窗统一移回当前屏幕。
          </p>
          <Button
            variant="secondary"
            size="sm"
            className="shrink-0"
            loading={windowPlacementBusy === "all"}
            disabled={windowPlacementBusy !== null
              || (!config.enable_tz_overlay && !config.enable_stats_overlay && !config.enable_bongo_cat)}
            onClick={recoverAllWindows}
          >
            <MonitorUp size={12} />
            全部移回
          </Button>
        </div>
      </div>
    </div>
  );
}
