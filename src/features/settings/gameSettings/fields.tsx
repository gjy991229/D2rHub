import React from "react";
import { Monitor, Volume2, Gamepad2, Map, Image } from "lucide-react";
import { RangeSlider } from "../../../components/ui/RangeSlider";
import { Toggle } from "../../../components/ui/Toggle";
import { FRAMERATE_CAP_KEY } from "../../../utils/gameSettings";
import { ResolutionInput } from "../../../components/ui/ResolutionInput";
import "./fields.css";
type Tab = "display" | "graphics" | "audio" | "gameplay" | "automap";
type FieldType = "toggle" | "select" | "range" | "number" | "resolution";
type GraphicsQualityPreset = "low" | "medium" | "high";

export type SettingsMap = Record<string, unknown>;

interface ConfigOption {
  label: string;
  value: number | string;
}

interface ConfigField {
  key: string;
  label: string;
  type: FieldType;
  defaultValue: number | string;
  existingOnly?: boolean;
  min?: number;
  max?: number;
  step?: number;
  options?: ConfigOption[];
}

interface ConfigSection {
  id: Tab;
  label: string;
  title: string;
  icon: React.ReactNode;
  fields: ConfigField[];
}

const offOnOptions: ConfigOption[] = [
  { label: "关闭", value: 0 },
  { label: "开启", value: 1 },
];

const qualityLowUltra: ConfigOption[] = [
  { label: "低", value: 0 },
  { label: "中", value: 1 },
  { label: "高", value: 2 },
  { label: "超高", value: 3 },
  { label: "极高", value: 4 },
];

const settingsSections: ConfigSection[] = [
  {
    id: "display",
    label: "显示",
    title: "视频部分",
    icon: <Monitor size={15} />,
    fields: [
      {
        key: "Window Mode",
        label: "窗口模式",
        type: "select",
        defaultValue: 0,
        options: [
          { label: "窗口化", value: 0 },
          { label: "全屏", value: 1 },
        ],
      },
      { key: "Screen Resolution (Windowed)", label: "分辨率", type: "resolution", defaultValue: "1280x720" },
      { key: "Sharpening", label: "锐化", type: "range", defaultValue: 25, min: 0, max: 100 },
      { key: "GammaHD", label: "亮度", type: "range", defaultValue: 2400, min: 1600, max: 3200 },
      { key: "Safe Screen Percent", label: "屏幕区域", type: "range", defaultValue: 100, min: 90, max: 100 },
      { key: "VSync", label: "垂直同步", type: "toggle", defaultValue: 0 },
    ],
  },
  {
    id: "graphics",
    label: "图形",
    title: "图形质量",
    icon: <Image size={15} />,
    fields: [
      {
        key: "NVIDIA DLSS",
        label: "NVIDIA DLSS",
        type: "select",
        defaultValue: 0,
        options: [
          { label: "关闭", value: 0 },
          { label: "自动", value: 1 },
          { label: "质量", value: 2 },
          { label: "平衡", value: 3 },
          { label: "性能", value: 4 },
          { label: "超高性能", value: 5 },
        ],
      },
      {
        key: "Graphic Presets",
        label: "画质预设",
        type: "select",
        defaultValue: 7,
        options: [
          { label: "低", value: 0 },
          { label: "中", value: 1 },
          { label: "高", value: 2 },
          { label: "超高", value: 3 },
          { label: "自定义", value: 7 },
        ],
      },
      {
        key: "Texture Quality",
        label: "纹理质量",
        type: "select",
        defaultValue: 3,
        options: [
          { label: "低", value: 2 },
          { label: "中", value: 3 },
          { label: "高", value: 4 },
          { label: "最高", value: 5 },
        ],
      },
      {
        key: "Texture Anisotropy",
        label: "各向异性过滤",
        type: "select",
        defaultValue: 0,
        options: [
          { label: "1x", value: 0 },
          { label: "2x", value: 1 },
          { label: "4x", value: 2 },
          { label: "8x", value: 3 },
          { label: "16x", value: 4 },
        ],
      },
      {
        key: "Ambient Occlusion Quality",
        label: "环境光遮蔽",
        type: "select",
        defaultValue: 0,
        options: [
          { label: "关闭", value: 0 },
          { label: "低", value: 1 },
          { label: "中", value: 2 },
          { label: "高", value: 3 },
          { label: "超高", value: 4 },
        ],
      },
      {
        key: "Character Detail",
        label: "角色细节",
        type: "select",
        defaultValue: 3,
        options: [
          { label: "低", value: 2 },
          { label: "中", value: 3 },
          { label: "高", value: 4 },
          { label: "超高", value: 5 },
        ],
      },
      {
        key: "Environment Detail",
        label: "环境细节",
        type: "select",
        defaultValue: 3,
        options: [
          { label: "低", value: 2 },
          { label: "中", value: 3 },
          { label: "高", value: 4 },
          { label: "超高", value: 5 },
        ],
      },
      {
        key: "Transparency Quality",
        label: "透明度质量",
        type: "select",
        defaultValue: 3,
        options: [
          { label: "高", value: 3 },
          { label: "超高", value: 4 },
          { label: "极高", value: 5 },
        ],
      },
      {
        key: "Shadow Quality",
        label: "阴影质量",
        type: "select",
        defaultValue: 2,
        options: [
          { label: "低", value: 1 },
          { label: "中", value: 2 },
          { label: "高", value: 3 },
          { label: "超高", value: 4 },
        ],
      },
      {
        key: "Anti Aliasing",
        label: "抗锯齿",
        type: "select",
        defaultValue: 0,
        options: [
          { label: "关闭", value: 0 },
          { label: "FXAA", value: 1 },
          { label: "SMAA", value: 2 },
          { label: "TAA", value: 3 },
        ],
      },
      { key: "Resolution Scale", label: "分辨率缩放(%)", type: "range", defaultValue: 100, min: 50, max: 200 },
      { key: "Dynamic Resolution Scaling", label: "分辨率动态调整", type: "toggle", defaultValue: 0 },
      { key: FRAMERATE_CAP_KEY, label: "帧数上限", type: "number", defaultValue: 0, min: 0, max: 500 },
      { key: "Vfx Quality", label: "Vfx质量", type: "select", defaultValue: 2, options: qualityLowUltra },
      {
        key: "Vfx Lighting Quality",
        label: "Vfx光照质量",
        type: "select",
        defaultValue: 1,
        options: [
          { label: "低", value: 0 },
          { label: "中", value: 1 },
          { label: "高", value: 2 },
          { label: "极高", value: 3 },
        ],
      },
      { key: "Perspective", label: "远景模式", type: "toggle", defaultValue: 0 },
    ],
  },
  {
    id: "audio",
    label: "音频",
    title: "音频部分",
    icon: <Volume2 size={15} />,
    fields: [
      { key: "Master Volume", label: "音效音量", type: "range", defaultValue: 80, min: 0, max: 100 },
      { key: "Music Volume", label: "音乐音量", type: "range", defaultValue: 50, min: 0, max: 100 },
      {
        key: "NPC Speech",
        label: "NPC语音",
        type: "select",
        defaultValue: 2,
        options: [
          { label: "仅音频", value: 0 },
          { label: "仅文本", value: 1 },
          { label: "音频与文本", value: 2 },
        ],
      },
      { key: "Subtitles", label: "字幕", type: "toggle", defaultValue: 0, existingOnly: true },
      { key: "GreetingsSubtitles", label: "NPC 问候字幕", type: "toggle", defaultValue: 0, existingOnly: true },
      { key: "Mono Mode", label: "单声道音频", type: "toggle", defaultValue: 0, existingOnly: true },
      { key: "Verb Disable", label: "禁用混响", type: "toggle", defaultValue: 0 },
      { key: "Controller Feedback", label: "控制器反馈", type: "toggle", defaultValue: 0 },
    ],
  },
  {
    id: "gameplay",
    label: "玩法",
    title: "游戏部分",
    icon: <Gamepad2 size={15} />,
    fields: [
      {
        key: "Item Name Display",
        label: "显示物品",
        type: "select",
        defaultValue: 1,
        options: [
          { label: "长按", value: 0 },
          { label: "开关", value: 1 },
          { label: "限时", value: 2 },
        ],
      },
      {
        key: "Unfiltered Item Name Display",
        label: "显示物品(未过滤)",
        type: "select",
        defaultValue: 1,
        options: [
          { label: "长按", value: 0 },
          { label: "开关", value: 1 },
          { label: "限时", value: 2 },
        ],
      },
      { key: "Always Run", label: "始终奔跑", type: "toggle", defaultValue: 0, existingOnly: true },
      { key: "Camera Shake", label: "镜头震动", type: "toggle", defaultValue: 0, existingOnly: true },
      { key: "Display Active Skill Bindings", label: "显示当前技能快捷键", type: "toggle", defaultValue: 0, existingOnly: true },
      { key: "Controller Swap Thumbsticks", label: "交换手柄摇杆", type: "toggle", defaultValue: 0, existingOnly: true },
      { key: "Controller Weapon Swap Separate Bindings", label: "切换武器使用独立手柄绑定", type: "toggle", defaultValue: 0, existingOnly: true },
      { key: "Auto Gold Enabled", label: "自动拾取金币", type: "toggle", defaultValue: 1 },
      { key: "Auto Party Invite", label: "自动队伍邀请", type: "toggle", defaultValue: 0 },
      { key: "Quick Cast Enabled", label: "快速施法", type: "toggle", defaultValue: 0 },
      { key: "Confine Cursor", label: "限制鼠标活动范围", type: "toggle", defaultValue: 0 },
      { key: "Show HP Text", label: "显示生命球文本", type: "toggle", defaultValue: 0 },
      { key: "Show MP Text", label: "显示法力球文本", type: "toggle", defaultValue: 0 },
      { key: "Skill Ammo Feedback", label: "显示技能上的弹药数量", type: "toggle", defaultValue: 0 },
      { key: "Show Clock", label: "显示时钟", type: "toggle", defaultValue: 1 },
      { key: "Allow Chat", label: "允许在线聊天", type: "toggle", defaultValue: 1 },
      { key: "Controller Cursor Speed", label: "鼠标灵敏度", type: "number", defaultValue: 2500, min: 1000, max: 5000 },
      { key: "Controller Rumble Enabled", label: "启用控制器震动", type: "toggle", defaultValue: 1 },
    ],
  },
  {
    id: "automap",
    label: "地图",
    title: "地图部分",
    icon: <Map size={15} />,
    fields: [
      {
        key: "AutoMapMode",
        label: "透视地图尺寸",
        type: "select",
        defaultValue: 0,
        options: [
          { label: "全屏地图", value: 0 },
          { label: "微缩地图放左", value: 1 },
          { label: "微缩地图放右", value: 2 },
        ],
      },
      { key: "AutoMap Party", label: "地图显示队友", type: "toggle", defaultValue: 0, existingOnly: true },
      { key: "AutoMap Party Names", label: "地图显示队友名称", type: "toggle", defaultValue: 0, existingOnly: true },
      { key: "AutoMap Open On Load", label: "进入游戏时打开地图", type: "toggle", defaultValue: 0, existingOnly: true },
      { key: "AutoMap Centers", label: "地图居中", type: "toggle", defaultValue: 0, existingOnly: true },
      { key: "Automap Room Name and Password", label: "地图显示房名和密码", type: "toggle", defaultValue: 0, existingOnly: true },
      { key: "AutoMapFadeAllCustomOpacity", label: "自定义透明度", type: "range", defaultValue: 50, min: 0, max: 100 },
    ],
  },
];

const graphicsQualityPresets: ReadonlyArray<{
  id: GraphicsQualityPreset;
  label: string;
  dlss: number;
  framerateCap: number;
}> = [
  { id: "low", label: "低", dlss: 0, framerateCap: 30 },
  { id: "medium", label: "中", dlss: 1, framerateCap: 180 },
  { id: "high", label: "高", dlss: 2, framerateCap: 0 },
];

function createGraphicsQualityPreset(preset: GraphicsQualityPreset): SettingsMap {
  const patch: SettingsMap = {};
  const graphicsSection = settingsSections.find((section) => section.id === "graphics");

  for (const field of graphicsSection?.fields ?? []) {
    if (field.type !== "select" || !field.options?.length) continue;

    const optionIndex = preset === "low"
      ? 0
      : preset === "high"
        ? field.options.length - 1
        : Math.floor((field.options.length - 1) / 2);
    patch[field.key] = field.options[optionIndex].value;
  }

  const selectedPreset = graphicsQualityPresets.find((candidate) => candidate.id === preset)
    ?? graphicsQualityPresets[0];

  return {
    ...patch,
    "NVIDIA DLSS": selectedPreset.dlss,
    "Resolution Scale": preset === "low" ? 50 : 100,
    ...(preset === "low" ? { "Dynamic Resolution Scaling": 0 } : {}),
    "Graphic Presets": 7,
    "Anti Aliasing": 1,
    [FRAMERATE_CAP_KEY]: selectedPreset.framerateCap,
    Perspective: 0,
    VSync: 0,
  };
}

function matchesGraphicsQualityPreset(settings: SettingsMap, preset: GraphicsQualityPreset): boolean {
  return Object.entries(createGraphicsQualityPreset(preset)).every(
    ([key, value]) => Number(settings[key]) === Number(value),
  );
}

function numberValue(settings: SettingsMap, key: string, fallback: number): number {
  const value = Number(settings[key] ?? fallback);
  return Number.isFinite(value) ? value : fallback;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function SettingRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="game-setting-row">
      <span className="text-sm text-text-secondary">{label}</span>
      <div className="game-setting-control">{children}</div>
    </div>
  );
}

export function SectionCard({
  title,
  actions,
  children,
}: {
  title: string;
  actions?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div
      className="game-setting-section rounded-card p-3"
      style={{
        background: "var(--surface-card)",
        border: "1px solid var(--border-default)",
      }}
    >
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-sm font-medium text-text-primary">{title}</h3>
        {actions}
      </div>
      <div>{children}</div>
    </div>
  );
}

export function SliderInput({
  label,
  value,
  min,
  max,
  step = 1,
  onChange,
}: {
  label?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
}) {
  return (
    <div className="game-setting-slider">
      <RangeSlider
        aria-label={label}
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="min-w-0 flex-1"
      />
      <span className="text-xs text-text-primary tabular-nums w-10 text-right">{value}</span>
    </div>
  );
}

export function DropdownInput({
  label,
  value,
  options,
  onChange,
}: {
  label?: string;
  value: number | string;
  options: ConfigOption[];
  onChange: (v: number | string) => void;
}) {
  return (
    <select
      aria-label={label}
      value={value}
      onChange={(e) => {
        const v = e.target.value;
        const num = Number(v);
        onChange(Number.isNaN(num) ? v : num);
      }}
      className="game-setting-select h-8 px-2.5 rounded-input bg-surface-card text-text-primary text-xs focus:border-accent focus:outline-none cursor-pointer min-w-[100px]"
      style={{ border: "1px solid var(--border-default)" }}
    >
      {options.map((opt) => (
        <option key={String(opt.value)} value={String(opt.value)}>
          {opt.label}
        </option>
      ))}
    </select>
  );
}

function NumberInput({
  label,
  value,
  min,
  max,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  onChange: (v: number) => void;
}) {
  const [draft, setDraft] = React.useState(String(value));
  const focused = React.useRef(false);
  React.useEffect(() => { if (!focused.current) setDraft(String(value)); }, [value]);
  return (
    <input
      aria-label={label}
      type="number"
      min={min}
      max={max}
      value={draft}
      onFocus={() => { focused.current = true; }}
      onChange={event => setDraft(event.target.value)}
      onBlur={() => {
        focused.current = false;
        const number = draft.trim() ? Number(draft) : NaN;
        const next = Number.isFinite(number) ? clamp(Math.round(number), min, max) : value;
        setDraft(String(next));
        if (next !== value) onChange(next);
      }}
      onKeyDown={event => {
        if (event.key === "Enter") { event.preventDefault(); event.currentTarget.blur(); }
        if (event.key === "Escape" && draft !== String(value)) { event.stopPropagation(); setDraft(String(value)); }
      }}
      className="h-8 w-28 rounded-input bg-surface-card px-2.5 text-right text-xs text-text-primary focus:border-accent focus:outline-none"
      style={{ border: "1px solid var(--border-default)" }}
    />
  );
}

function FieldControl({
  field,
  settings,
  update,
}: {
  field: ConfigField;
  settings: SettingsMap;
  update: (k: string, v: unknown) => void;
}) {
  if (field.type === "toggle") {
    return (
      <Toggle
        ariaLabel={field.label}
        checked={numberValue(settings, field.key, Number(field.defaultValue)) === 1}
        onChange={(v) => update(field.key, typeof settings[field.key] === "boolean" ? v : v ? 1 : 0)}
      />
    );
  }

  if (field.type === "select") {
    const fallback = field.defaultValue;
    return (
      <DropdownInput
        label={field.label}
        value={settings[field.key] == null ? fallback : (settings[field.key] as number | string)}
        options={field.options ?? offOnOptions}
        onChange={(v) => update(field.key, v)}
      />
    );
  }

  if (field.type === "resolution") {
    return (
      <ResolutionInput
        label={field.label}
        value={String(settings[field.key] ?? field.defaultValue)}
        onChange={(v) => update(field.key, v)}
      />
    );
  }

  const min = field.min ?? 0;
  const max = field.max ?? 100;
  const value = numberValue(settings, field.key, Number(field.defaultValue));

  if (field.type === "range") {
    return (
      <SliderInput
        label={field.label}
        value={clamp(value, min, max)}
        min={min}
        max={max}
        step={field.step}
        onChange={(v) => update(field.key, v)}
      />
    );
  }

  return (
    <NumberInput
      label={field.label}
      value={clamp(value, min, max)}
      min={min}
      max={max}
      onChange={(v) => update(field.key, v)}
    />
  );
}

function SettingsSection({
  section,
  settings,
  update,
  actions,
}: {
  section: ConfigSection;
  settings: SettingsMap;
  update: (k: string, v: unknown) => void;
  actions?: React.ReactNode;
}) {
  return (
    <SectionCard title={section.title} actions={actions}>
      {section.fields.filter(field => !field.existingOnly || [0, 1, true, false].includes(settings[field.key] as number | boolean)).map((field) => (
        <SettingRow key={field.key} label={field.label}>
          <FieldControl field={field} settings={settings} update={update} />
        </SettingRow>
      ))}
    </SectionCard>
  );
}

function GraphicsQualityPresetControl({
  settings,
  update,
}: {
  settings: SettingsMap;
  update: (k: string, v: unknown) => void;
}) {
  const applyPreset = (preset: GraphicsQualityPreset) => {
    for (const [key, value] of Object.entries(createGraphicsQualityPreset(preset))) {
      update(key, value);
    }
  };

  return (
    <div className="flex items-center gap-2">
      <span className="micro-meta">快速预设</span>
      <div className="preset-pill-group" role="group" aria-label="图形质量快速预设">
        {graphicsQualityPresets.map((preset) => {
          const active = matchesGraphicsQualityPreset(settings, preset.id);
          return (
            <button
              key={preset.id}
              type="button"
              className="preset-pill"
              data-active={active ? "true" : "false"}
              aria-pressed={active}
              title={`应用${preset.label}画质预设`}
              onClick={() => applyPreset(preset.id)}
            >
              {preset.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}

export function DisplaySection({ settings, update }: { settings: SettingsMap; update: (k: string, v: unknown) => void }) {
  return <SettingsSection section={settingsSections[0]} settings={settings} update={update} />;
}

export function GraphicsSection({ settings, update }: { settings: SettingsMap; update: (k: string, v: unknown) => void }) {
  return (
    <SettingsSection
      section={settingsSections[1]}
      settings={settings}
      update={update}
      actions={<GraphicsQualityPresetControl settings={settings} update={update} />}
    />
  );
}

export function AudioSection({ settings, update }: { settings: SettingsMap; update: (k: string, v: unknown) => void }) {
  return <SettingsSection section={settingsSections[2]} settings={settings} update={update} />;
}

export function GameplaySection({ settings, update }: { settings: SettingsMap; update: (k: string, v: unknown) => void }) {
  return <SettingsSection section={settingsSections[3]} settings={settings} update={update} />;
}

export function AutomapSection({ settings, update }: { settings: SettingsMap; update: (k: string, v: unknown) => void }) {
  return <SettingsSection section={settingsSections[4]} settings={settings} update={update} />;
}
