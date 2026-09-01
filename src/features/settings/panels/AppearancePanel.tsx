import { Check } from "lucide-react";
import { Button } from "../../../components/ui/Button";
import { Toggle } from "../../../components/ui/Toggle";
import type { ThemeKey } from "../../../store/theme";
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
}: AppearancePanelProps) {
  const sliderValue = percentToSlider(draft.main_opacity);

  return (
    <div className="settings-content-grid appearance-settings-grid">
      <div className="appearance-apply-strip" data-dirty={dirty ? "true" : undefined}>
        <div>
          <strong>{dirty ? "外观修改等待应用" : "外观设置已应用"}</strong>
          <p>{dirty ? "点击应用可立即查看效果；离开本页或关闭设置时也会自动应用。" : "主题、透明度和字号使用当前已保存方案。"}</p>
        </div>
        <Button
          variant="primary"
          size="md"
          loading={applying}
          disabled={!dirty}
          onClick={() => void onApply()}
        >
          <Check size={14} aria-hidden="true" />
          应用外观
        </Button>
      </div>

      <section className="spatial-panel appearance-section">
        <div className="appearance-section-heading">
          <h3>界面语言</h3>
          <p>只影响 D2RHub 界面，不改变游戏内容和符文名称。</p>
        </div>
        <select
          aria-label="软件界面显示语言"
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
          <h3>主程序窗口主题</h3>
          <p>选择适合当前使用环境的明暗层级。</p>
        </div>
        <div className="appearance-theme-options">
          {([
            { id: "onyx", label: "深色", desc: "纯黑分层，高对比" },
            { id: "light", label: "浅色", desc: "清晰明亮，低干扰" },
          ] as const).map((option) => {
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
                <span><strong>{option.label}</strong><small>{option.desc}</small></span>
                {active && <Check size={14} aria-hidden="true" />}
              </button>
            );
          })}
        </div>
      </section>

      <section className="spatial-panel appearance-section appearance-opacity-section">
        <div className="appearance-section-heading appearance-section-heading-inline">
          <div>
            <h3>主界面背景不透明度</h3>
            <p>仅调整背景层，文字和控件保持清晰。</p>
          </div>
          <label className="appearance-percent-input">
            <input
              type="number"
              aria-label="主界面背景不透明度百分比"
              min={10}
              max={100}
              value={draft.main_opacity}
              onChange={(event) => onChange({
                main_opacity: Math.max(10, Math.min(100, Number.parseInt(event.target.value, 10) || 10)),
              })}
            />
            <span>%</span>
          </label>
        </div>
        <input
          type="range"
          aria-label="主界面背景不透明度"
          min={0}
          max={100}
          value={sliderValue}
          onChange={(event) => onChange({ main_opacity: sliderToPercent(Number(event.target.value)) })}
          className="appearance-opacity-slider"
          style={{
            background: `linear-gradient(to right, var(--accent) ${sliderValue}%, var(--surface-hover) ${sliderValue}%)`,
          }}
        />
      </section>

      <section className="spatial-panel appearance-section">
        <div className="appearance-section-heading">
          <h3>字体大小</h3>
          <p>导航、表单和状态文字会按同一比例调整。</p>
        </div>
        <div className="appearance-font-options">
          {([
            { id: "small", label: "较小", sample: "Aa" },
            { id: "default", label: "默认", sample: "Aa" },
            { id: "large", label: "较大", sample: "Aa" },
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

      <section className="spatial-panel appearance-section appearance-taskbar-section">
        <div className="appearance-section-heading">
          <h3>游戏实例任务栏独立</h3>
          <p>为每个账号使用独立任务栏标识；对之后启动或重新识别的窗口生效。</p>
        </div>
        <Toggle
          checked={draft.separate_game_taskbar_icons}
          ariaLabel="让每个游戏账号使用独立任务栏图标"
          onChange={(value) => onChange({ separate_game_taskbar_icons: value })}
        />
      </section>
    </div>
  );
}
