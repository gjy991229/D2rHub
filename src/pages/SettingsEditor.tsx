import React, { useEffect, useMemo, useState } from "react";
import { Save, Monitor, Volume2, Gamepad2, Map, Image, X } from "lucide-react";
import { emitEvent, invokeCommand } from "../platform/tauri";
import { Button } from "../components/ui/Button";
import { RangeSlider } from "../components/ui/RangeSlider";
import { Toggle } from "../components/ui/Toggle";
import { showToast } from "../components/ui/Toast";
import type { AccountMeta } from "../store/types";
import { useAccounts } from "../store/accounts";
import { FRAMERATE_CAP_KEY, writeFramerateCap } from "../utils/gameSettings";

interface Props {
  account: AccountMeta;
  onClose: () => void;
}

type Tab = "display" | "graphics" | "audio" | "gameplay" | "automap";
type FieldType = "toggle" | "select" | "range" | "number" | "resolution";

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
      { key: "AutoMapFadeAllCustomOpacity", label: "自定义透明度", type: "range", defaultValue: 50, min: 0, max: 100 },
    ],
  },
];

const tabs = settingsSections.map(({ id, label, icon }) => ({ id, label, icon }));

const baseResolutionOptions = [
  "1280x720",
  "1280x768",
  "1280x800",
  "1360x768",
  "1366x768",
  "1280x960",
  "1444x900",
  "1600x900",
  "1440x1080",
  "1600x1024",
  "1680x1050",
  "1600x1200",
  "1920x1080",
  "1920x1200",
  "1920x1440",
  "2560x1440",
  "2560x1600",
  "3440x1440",
  "3840x2160",
];

function numberValue(settings: SettingsMap, key: string, fallback: number): number {
  const value = Number(settings[key] ?? fallback);
  return Number.isFinite(value) ? value : fallback;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function parseResolution(value: string) {
  const match = value.trim().match(/^(\d{3,5})x(\d{3,5})$/i);
  if (!match) return null;
  return { width: Number(match[1]), height: Number(match[2]) };
}

function getResolutionOptions() {
  const screenWidth = typeof window === "undefined" ? 2560 : Math.max(window.screen.width, window.screen.height);
  const screenHeight = typeof window === "undefined" ? 1440 : Math.min(window.screen.width, window.screen.height);
  const options = baseResolutionOptions.filter((item) => {
    const parsed = parseResolution(item);
    return parsed && parsed.width <= screenWidth && parsed.height <= screenHeight;
  });
  return options.length > 0 ? options : [baseResolutionOptions[0]];
}

function normalizeResolution(value: string, options: string[]) {
  const parsed = parseResolution(value);
  if (!parsed) return options[0];
  const first = parseResolution(options[0]) ?? { width: 1280, height: 720 };
  const last = parseResolution(options[options.length - 1]) ?? first;
  const width = clamp(parsed.width, first.width, last.width);
  const height = clamp(parsed.height, first.height, last.height);
  return `${Math.round(width)}x${Math.round(height)}`;
}

export function SettingsEditor({ account, onClose }: Props) {
  const [settings, setSettings] = useState<SettingsMap>({});
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [activeTab, setActiveTab] = useState<Tab>("display");
  const [hasChanges, setHasChanges] = useState(false);
  const { markSettingsCustomized } = useAccounts();

  useEffect(() => {
    loadSettings();
  }, [account.id]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (hasChanges) {
          if (confirm("有未保存的更改，确定关闭吗？")) onClose();
        } else {
          onClose();
        }
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onClose, hasChanges]);

  const loadSettings = async () => {
    setLoading(true);
    try {
      const data = await invokeCommand<SettingsMap>("get_account_settings", {
        accountId: account.id,
      });
      setSettings(data);
    } catch (e) {
      showToast("error", `加载设置失败: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  const update = (key: string, value: unknown) => {
    setSettings((prev) => key === FRAMERATE_CAP_KEY
      ? writeFramerateCap(prev, Number(value))
      : ({ ...prev, [key]: value }));
    setHasChanges(true);
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      await invokeCommand("save_account_settings", {
        accountId: account.id,
        settings,
      });
      await markSettingsCustomized(account.id);
      setHasChanges(false);
      showToast("success", "设置已保存");
      await emitEvent("account-settings-updated", { accountId: account.id });
    } catch (e) {
      showToast("error", `保存失败: ${e}`);
    } finally {
      setSaving(false);
    }
  };

  const snapshotSystemSettings = async () => {
    try {
      const data = await invokeCommand<SettingsMap>("snapshot_system_settings_to_account", {
        accountId: account.id,
      });
      setSettings(data);
      setHasChanges(false);
      showToast("success", "已快照系统配置");
      await emitEvent("account-settings-updated", { accountId: account.id });
    } catch (e) {
      showToast("error", `快照系统配置失败: ${e}`);
    }
  };

  const activeSection = settingsSections.find((section) => section.id === activeTab) ?? settingsSections[0];
  const displayName = account.display_name || account.id;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center modal-backdrop"
      style={{ backgroundColor: "rgba(18,24,34,0.10)" }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="modal-content rounded-2xl w-[90vw] max-w-[860px] max-h-[95vh] flex overflow-hidden"
        style={{
          background: "linear-gradient(180deg, var(--surface-modal, var(--surface-glass)), var(--surface-card))",
          backdropFilter: "blur(16px) saturate(1.04)",
          WebkitBackdropFilter: "blur(16px) saturate(1.04)",
          border: "1px solid var(--border-default)",
          boxShadow: "var(--shadow-elevated)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className="w-[160px] shrink-0 flex flex-col py-3 px-2"
          style={{ borderRight: "1px solid var(--border-default)", background: "var(--surface-tile-soft, var(--surface-glass))" }}
        >
          <div className="px-3 mb-3">
            <p className="text-md font-semibold text-text-primary truncate">{displayName}</p>
            <p className="text-xs text-text-muted">{account.id}</p>
          </div>

          <div className="flex-1 space-y-0.5">
            {tabs.map((t) => (
              <button
                key={t.id}
                onClick={() => setActiveTab(t.id)}
                className="w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-md font-medium transition-all duration-150 text-left"
                style={activeTab === t.id ? {
                  color: "var(--text-primary)",
                  background: "var(--surface-hover)",
                } : {
                  color: "var(--text-muted)",
                }}
              >
                <span className={activeTab === t.id ? "text-accent" : ""}>{t.icon}</span>
                {t.label}
              </button>
            ))}
          </div>

          <div className="px-2 pt-3 mt-auto" style={{ borderTop: "1px solid var(--border-default)" }}>
            <button
              onClick={snapshotSystemSettings}
              className="w-full py-1.5 rounded-md text-xs font-medium text-text-muted hover:text-text-primary hover:bg-surface-hover transition-all text-center"
            >
              快照系统配置
            </button>
          </div>
        </div>

        <div className="flex-1 flex flex-col min-h-0">
          <div
            className="flex items-center justify-between px-5 py-3.5 shrink-0"
            style={{ borderBottom: "1px solid var(--border-default)" }}
          >
            <h2 className="text-lg font-semibold text-text-primary flex items-center gap-2">
              {activeSection.icon}
              {activeSection.label}
            </h2>
            <div className="flex items-center gap-2">
              <Button variant="primary" size="sm" loading={saving} disabled={!hasChanges} onClick={handleSave}>
                <Save size={12} />
                保存
              </Button>
              <button onClick={onClose} className="icon-btn w-7 h-7 rounded-full">
                <X size={14} />
              </button>
            </div>
          </div>

          {loading ? (
            <div className="flex-1 p-5 space-y-3 overflow-auto">
              {[1, 2, 3, 4, 5].map((i) => (
                <div key={i} className="h-10 skeleton rounded-lg" />
              ))}
            </div>
          ) : (
            <div className="flex-1 overflow-auto p-5">
              <div className="max-w-xl space-y-4">
                <SettingsSection section={activeSection} settings={settings} update={update} />
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export function SettingRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 py-1.5">
      <span className="text-sm text-text-secondary">{label}</span>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function SectionCard({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div
      className="rounded-card p-3"
      style={{
        background: "var(--surface-card)",
        border: "1px solid var(--border-default)",
      }}
    >
      <h3 className="text-sm font-medium text-text-primary mb-2">{title}</h3>
      <div>{children}</div>
    </div>
  );
}

export function SliderInput({
  value,
  min,
  max,
  step = 1,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
}) {
  return (
    <div className="flex items-center gap-2 w-48">
      <RangeSlider
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
  value,
  options,
  onChange,
}: {
  value: number | string;
  options: ConfigOption[];
  onChange: (v: number | string) => void;
}) {
  return (
    <select
      value={value}
      onChange={(e) => {
        const v = e.target.value;
        const num = Number(v);
        onChange(Number.isNaN(num) ? v : num);
      }}
      className="h-8 px-2.5 rounded-input bg-surface-card text-text-primary text-xs focus:border-accent focus:outline-none cursor-pointer min-w-[100px]"
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
  value,
  min,
  max,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  onChange: (v: number) => void;
}) {
  return (
    <input
      type="number"
      min={min}
      max={max}
      value={value}
      onChange={(e) => onChange(clamp(Number(e.target.value) || 0, min, max))}
      className="h-8 w-28 rounded-input bg-surface-card px-2.5 text-right text-xs text-text-primary focus:border-accent focus:outline-none"
      style={{ border: "1px solid var(--border-default)" }}
    />
  );
}

function ResolutionInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const options = useMemo(() => getResolutionOptions(), []);
  const listId = "settings-editor-resolution-options";
  return (
    <div className="combo-input w-40">
      <input
        value={value}
        list={listId}
        placeholder="1280x720"
        onChange={(e) => onChange(e.target.value)}
        onBlur={(e) => onChange(normalizeResolution(e.target.value, options))}
      />
      <datalist id={listId}>
        {options.map((option) => (
          <option key={option} value={option} />
        ))}
      </datalist>
    </div>
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
        checked={numberValue(settings, field.key, Number(field.defaultValue)) === 1}
        onChange={(v) => update(field.key, v ? 1 : 0)}
      />
    );
  }

  if (field.type === "select") {
    const fallback = field.defaultValue;
    return (
      <DropdownInput
        value={settings[field.key] == null ? fallback : (settings[field.key] as number | string)}
        options={field.options ?? offOnOptions}
        onChange={(v) => update(field.key, v)}
      />
    );
  }

  if (field.type === "resolution") {
    return (
      <ResolutionInput
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
}: {
  section: ConfigSection;
  settings: SettingsMap;
  update: (k: string, v: unknown) => void;
}) {
  return (
    <SectionCard title={section.title}>
      {section.fields.map((field) => (
        <SettingRow key={field.key} label={field.label}>
          <FieldControl field={field} settings={settings} update={update} />
        </SettingRow>
      ))}
    </SectionCard>
  );
}

export function DisplaySection({ settings, update }: { settings: SettingsMap; update: (k: string, v: unknown) => void }) {
  return <SettingsSection section={settingsSections[0]} settings={settings} update={update} />;
}

export function GraphicsSection({ settings, update }: { settings: SettingsMap; update: (k: string, v: unknown) => void }) {
  return <SettingsSection section={settingsSections[1]} settings={settings} update={update} />;
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
