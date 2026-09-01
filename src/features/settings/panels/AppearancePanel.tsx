import type { Dispatch, SetStateAction } from "react";
import { Toggle } from "../../../components/ui/Toggle";
import { useGlobalConfig } from "../../../store/globalConfig";
import type { ThemeKey } from "../../../store/theme";
import type { GlobalConfig } from "../../../store/types";

interface AppearancePanelProps {
  config: GlobalConfig;
  theme: ThemeKey;
  setTheme: (theme: ThemeKey) => void;
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
  persistConfig: (draft: GlobalConfig, quiet?: boolean) => Promise<unknown>;
  setFontScaleKey: Dispatch<SetStateAction<number>>;
}

function sliderToPercent(value: number): number {
  return Math.round(10 + 0.009 * value * value);
}

function percentToSlider(percent: number): number {
  if (percent <= 10) return 0;
  return Math.round(100 * Math.sqrt((percent - 10) / 90));
}

export function AppearancePanel({
  config,
  theme,
  setTheme,
  updateConfig,
  persistConfig: persistGlobalDraft,
  setFontScaleKey,
}: AppearancePanelProps) {
  return (
<div className="settings-content-grid">
  <div className="spatial-panel p-3 space-y-2">
    <h3 className="text-xs font-bold text-text-primary">界面语言</h3>
    <div className="flex items-center justify-between py-1">
      <div>
        <span className="text-sm font-semibold text-text-secondary">软件界面显示语言</span>
        <p className="text-2xs text-text-muted">软件界面显示语言，游戏内容和符文名称不受影响</p>
      </div>
      <select
        aria-label="软件界面显示语言"
        value={config.app_language || "zh-CN"}
        onChange={async e => {
          const language = e.target.value;
          updateConfig(c => { c.app_language = language; });
          const cur = useGlobalConfig.getState().config;
          if (cur) await persistGlobalDraft({ ...cur, app_language: language }, true);
        }}
        className="h-8 px-2.5 rounded-lg bg-surface-hover border border-border-default text-text-primary text-xs"
      >
        <option value="zh-CN">中文</option>
        <option value="en-US">English</option>
      </select>
    </div>
  </div>

  <div className="spatial-panel p-3 space-y-2">
    <h3 className="text-xs font-bold text-text-primary">主程序窗口主题</h3>
    <div className="grid grid-cols-2 gap-2.5">
      {([
        { id: "onyx", label: "深色主题 (Onyx)", desc: "纯黑分层与高对比文字" },
        { id: "light", label: "浅色主题 (Light)", desc: "极简素雅明亮界面" }
      ] as const).map(t => {
        const active = theme === t.id;
        return (
          <button
            key={t.id}
            type="button"
            aria-pressed={active}
            onClick={() => setTheme(t.id)}
            className="flex flex-col items-start gap-1 p-3 rounded-xl border transition-all text-left"
            style={{
              borderColor: active ? "var(--accent)" : "var(--border-default)",
              background: active ? "var(--surface-hover)" : "transparent"
            }}
          >
            <span className={`text-md font-bold ${active ? "text-accent" : "text-text-primary"}`}>{t.label}</span>
            <span className="text-2xs text-text-muted">{t.desc}</span>
          </button>
        );
      })}
    </div>
  </div>

  <div className="spatial-panel p-3 space-y-2">
    <h3 className="text-xs font-bold text-text-primary">背景不透明度 (不影响内容/文字)</h3>

    {/* Main window opacity */}
    <div className="space-y-1">
      <div className="flex justify-between items-center text-xs">
        <span className="font-semibold text-text-secondary">主界面背景不透明度</span>
        <div className="flex items-center gap-1.5">
          <input
            type="number"
            aria-label="主界面背景不透明度百分比"
            min={10}
            max={100}
            value={config.main_opacity ?? 95}
            onChange={e => {
              const val = Math.max(10, Math.min(100, parseInt(e.target.value) || 10));
              updateConfig(c => { c.main_opacity = val; });
            }}
            className="h-[24px] w-12 px-1 rounded bg-surface-hover text-center font-mono text-xs text-text-primary border border-border-default"
          />
          <span className="text-text-muted">%</span>
        </div>
      </div>
      <div className="flex items-center gap-2">
        <input
          type="range"
          aria-label="主界面背景不透明度"
          min={0}
          max={100}
          value={percentToSlider(config.main_opacity ?? 95)}
          onChange={e => {
            const val = sliderToPercent(parseInt(e.target.value));
            updateConfig(c => { c.main_opacity = val; });
          }}
          className="flex-1 h-1.5 rounded-full appearance-none bg-surface-hover cursor-pointer"
        />
      </div>
      <p className="text-2xs text-text-muted">采用非线性映射，滑动高透明度区间更灵敏，输入框可输入真实百分比 10-100</p>
    </div>

  </div>

  {/* ── 字体大小 ── */}
  <div className="spatial-panel p-3 space-y-2">
    <h3 className="text-xs font-bold text-text-primary">字体大小</h3>
    <div className="grid grid-cols-3 gap-2">
      {([
        { id: "small",   label: "小",   desc: "放大 30%" },
        { id: "default", label: "默认", desc: "放大 45%" },
        { id: "large",   label: "大",   desc: "放大 60%" },
      ] as const).map(({ id, label, desc }) => {
        const fontScale = (() => {
          try { return document.documentElement.dataset.fontScale || "default"; }
          catch { return "default"; }
        })();
        const active = fontScale === id;
        return (
          <button
            key={id}
            type="button"
            aria-pressed={active}
            onClick={() => {
              document.documentElement.dataset.fontScale = id;
              try { localStorage.setItem("d2rhub-font-scale", id); } catch {}
              updateConfig(c => { c.font_scale = id; });
              const cur = useGlobalConfig.getState().config;
              if (cur) void persistGlobalDraft({ ...cur, font_scale: id }, true);
              setFontScaleKey(k => k + 1);
            }}
            className={`flex flex-col items-center gap-0.5 py-2.5 px-2 rounded-xl border transition-all ${
              active
                ? "border-accent bg-surface-hover"
                : "border-border-default hover:border-border-strong"
            }`}
          >
            <span className={`text-md font-bold ${active ? "text-accent" : "text-text-primary"}`}>{label}</span>
            <span className="text-2xs text-text-muted">{desc}</span>
          </button>
        );
      })}
    </div>
  </div>

  <div className="spatial-panel p-3 space-y-2">
    <h3 className="text-xs font-bold text-text-primary">游戏窗口与任务栏</h3>
    <div className="flex items-center justify-between gap-4 py-1">
      <div>
        <span className="text-sm font-semibold text-text-secondary">游戏实例任务栏独立</span>
        <p className="text-2xs text-text-muted leading-relaxed">
          为每个账号窗口设置独立任务栏标识，可分别拖曳排序。默认关闭，仅对之后启动或重新识别的游戏窗口生效。
        </p>
      </div>
      <Toggle
        checked={!!config.separate_game_taskbar_icons}
        ariaLabel="让每个游戏账号使用独立任务栏图标"
        onChange={value => updateConfig(current => { current.separate_game_taskbar_icons = value; })}
      />
    </div>
  </div>
</div>
  );
}
