import { LocateFixed, MonitorUp } from "lucide-react";
import { Button } from "../../../components/ui/Button";
import { Toggle } from "../../../components/ui/Toggle";
import { useGlobalConfig } from "../../../store/globalConfig";
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
  return (
    <div className="settings-content-grid">
      <div className="spatial-panel p-3 space-y-2">
        <h3 className="text-xs font-bold text-text-primary">信息悬浮窗主题</h3>
        <div className="grid grid-cols-2 gap-2.5">
          {([
            { id: "onyx", label: "深色悬浮窗", desc: "纯黑分层，清晰克制" },
            { id: "light", label: "浅色悬浮窗", desc: "明亮清晰界面" },
          ] as const).map((option) => {
            const active = (config.theme_overlay || "light") === option.id;
            return (
              <button
                key={option.id}
                type="button"
                aria-pressed={active}
                onClick={async () => {
                  updateConfig((current) => { current.theme_overlay = option.id; });
                  const current = useGlobalConfig.getState().config;
                  if (current) {
                    await persistGlobalDraft({ ...current, theme_overlay: option.id }, true);
                  }
                }}
                className="flex flex-col items-start gap-1 p-3 rounded-xl border transition-all text-left"
                style={{
                  borderColor: active ? "var(--accent)" : "var(--border-default)",
                  background: active ? "var(--surface-hover)" : "transparent",
                }}
              >
                <span className={`text-md font-bold ${active ? "text-accent" : "text-text-primary"}`}>
                  {option.label}
                </span>
                <span className="text-2xs text-text-muted">{option.desc}</span>
              </button>
            );
          })}
        </div>
      </div>

      <div className="spatial-panel p-3 space-y-2">
        <h3 className="text-xs font-bold text-text-primary">背景不透明度 (不影响内容/文字)</h3>
        <div className="space-y-1">
          <div className="flex justify-between items-center text-xs">
            <span className="font-semibold text-text-secondary">信息悬浮窗背景不透明度</span>
            <div className="flex items-center gap-1.5">
              <input
                type="number"
                aria-label="信息悬浮窗背景不透明度百分比"
                min={10}
                max={100}
                value={config.overlay_opacity ?? 95}
                onChange={(event) => {
                  const value = Math.max(10, Math.min(100, parseInt(event.target.value) || 10));
                  updateConfig((current) => { current.overlay_opacity = value; });
                }}
                className="h-[24px] w-12 px-1 rounded bg-surface-hover text-center font-mono text-xs text-text-primary border border-border-default"
              />
              <span className="text-text-muted">%</span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <input
              type="range"
              aria-label="信息悬浮窗背景不透明度"
              min={0}
              max={100}
              value={percentToSlider(config.overlay_opacity ?? 95)}
              onChange={(event) => {
                const value = sliderToPercent(parseInt(event.target.value));
                updateConfig((current) => { current.overlay_opacity = value; });
              }}
              className="flex-1 h-1.5 rounded-full appearance-none bg-surface-hover cursor-pointer"
            />
          </div>
          <p className="text-2xs text-text-muted">
            采用非线性映射，滑动高透明度区间更灵敏，输入框可输入真实百分比 10-100
          </p>
        </div>
      </div>

      <div className="spatial-panel p-3 space-y-2">
        <h3 className="text-xs font-bold text-text-primary">桌面悬浮窗口</h3>
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
                const current = useGlobalConfig.getState().config;
                if (current) {
                  await persistGlobalDraft({
                    ...current,
                    enable_tz_overlay: visible,
                    enable_overlay: visible || current.enable_stats_overlay,
                  }, true);
                }
                try {
                } catch (error) {
                  console.error("切换 TZ 播报窗口失败", error);
                }
              }}
            />
          </div>
        </div>

        <div className="flex items-center justify-between gap-4 border-t border-border-default/50 py-2">
          <div>
            <span className="text-sm font-semibold text-text-secondary">场景统计窗口</span>
            <p className="text-2xs text-text-muted">
              独立显示运行账号、场景计时与符文掉落；支持贴边自动隐藏，不使用迷你模式
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              loading={windowPlacementBusy === "stats-overlay"}
              disabled={!config.enable_stats_overlay || windowPlacementBusy !== null}
              onClick={() => locateWindow("stats-overlay")}
              title="将窗口移到主界面所在屏幕"
            >
              <LocateFixed size={12} />
              定位
            </Button>
            <Toggle
              checked={!!config.enable_stats_overlay}
              ariaLabel="显示场景统计悬浮窗"
              onChange={async (visible) => {
                updateConfig((current) => {
                  current.enable_stats_overlay = visible;
                  current.enable_overlay = current.enable_tz_overlay || visible;
                });
                const current = useGlobalConfig.getState().config;
                if (current) {
                  await persistGlobalDraft({
                    ...current,
                    enable_stats_overlay: visible,
                    enable_overlay: current.enable_tz_overlay || visible,
                  }, true);
                }
                try {
                } catch (error) {
                  console.error("切换统计窗口失败", error);
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
