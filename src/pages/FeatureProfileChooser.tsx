import { Check, Layers3, Minimize2 } from "lucide-react";
import { useState } from "react";

import type { FeatureProfile } from "../features/profile/featureProfile";
import type { GlobalConfig } from "../store/types";

interface FeatureProfileChooserProps {
  config: GlobalConfig;
  saving: boolean;
  onConfirm: (profile: FeatureProfile) => Promise<void>;
  onExit: () => void;
}

export function FeatureProfileChooser({ config, saving, onConfirm, onExit }: FeatureProfileChooserProps) {
  const isEnglish = config.app_language === "en-US";
  const [selected, setSelected] = useState<FeatureProfile | null>(null);
  const installedCount = config.installed_optional_modules?.length ?? 0;
  const copy = isEnglish
    ? {
        title: "Choose how you use D2RHub",
        description: "This only changes which features are visible and running. Your settings and data are never deleted.",
        minimal: "Minimal mode",
        minimalDetail: "Multi-instance launching and Mod management only. Other modules are paused and completely hidden.",
        normal: "Normal mode",
        normalDetail: "Keep the current complete experience and resume previously enabled modules.",
        affected: `${installedCount} configured module${installedCount === 1 ? "" : "s"} will be kept for normal mode.`,
        changeLater: "You can change this later in Settings → Maintenance & Transfer.",
        confirm: "Confirm and continue",
        exit: "Exit",
      }
    : {
        title: "选择 D2RHub 使用模式",
        description: "模式只改变可见功能和实际运行范围，不会删除任何设置或数据。",
        minimal: "极简模式",
        minimalDetail: "仅保留多开与 Mod 管理；其他模块暂停运行并完全隐藏。",
        normal: "正常模式",
        normalDetail: "保留当前完整功能，并按原配置恢复已启用模块。",
        affected: `现有 ${installedCount} 个模块配置会为正常模式完整保留。`,
        changeLater: "之后可在“设置 → 维护与迁移”中随时切换。",
        confirm: "确认并继续",
        exit: "退出 D2RHub",
      };

  const options: Array<{
    id: FeatureProfile;
    title: string;
    detail: string;
    icon: typeof Minimize2;
  }> = [
    { id: "minimal", title: copy.minimal, detail: copy.minimalDetail, icon: Minimize2 },
    { id: "normal", title: copy.normal, detail: copy.normalDetail, icon: Layers3 },
  ];

  return (
    <div className="flex-1 min-h-0 min-w-0 flex flex-col items-center overflow-y-auto px-6 py-8">
      <div className="my-auto shrink-0 w-full max-w-[680px] account-line px-6 py-6">
        <div className="mb-5" data-tauri-drag-region>
          <h1 className="text-base font-semibold text-text-primary">{copy.title}</h1>
          <p className="mt-1.5 text-sm leading-relaxed text-text-secondary">{copy.description}</p>
        </div>

        <div className="grid grid-cols-2 gap-3 max-[620px]:grid-cols-1" role="radiogroup" aria-label={copy.title}>
          {options.map((option) => {
            const Icon = option.icon;
            const active = selected === option.id;
            return (
              <button
                key={option.id}
                type="button"
                role="radio"
                aria-checked={active}
                disabled={saving}
                onClick={() => setSelected(option.id)}
                className="relative min-h-[144px] rounded-card p-4 text-left transition-all duration-150 focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2 disabled:opacity-60"
                style={{
                  background: active ? "rgb(var(--accent-rgb) / 0.08)" : "var(--surface-tile-soft, var(--surface-card))",
                  border: active ? "1px solid rgb(var(--accent-rgb) / 0.35)" : "1px solid var(--border-default)",
                }}
              >
                <span className="mb-3 flex h-9 w-9 items-center justify-center rounded-[12px] border border-border-default bg-surface-hover text-text-secondary">
                  <Icon size={17} strokeWidth={1.8} aria-hidden="true" />
                </span>
                <strong className="block text-sm font-semibold text-text-primary">{option.title}</strong>
                <span className="mt-1.5 block text-xs leading-relaxed text-text-secondary">{option.detail}</span>
                {active && (
                  <span className="absolute right-3 top-3 flex h-5 w-5 items-center justify-center rounded-full bg-accent text-white" aria-hidden="true">
                    <Check size={12} strokeWidth={2.4} />
                  </span>
                )}
              </button>
            );
          })}
        </div>

        {installedCount > 0 && (
          <p className="mt-3 text-xs leading-relaxed text-text-muted">{copy.affected}</p>
        )}
        <div className="mt-5 flex items-center justify-between gap-4 border-t border-border-default/60 pt-4 max-[520px]:items-start max-[520px]:flex-col">
          <p className="text-xs leading-relaxed text-text-muted">{copy.changeLater}</p>
          <div className="flex shrink-0 gap-2">
            <button type="button" className="control-btn h-9" disabled={saving} onClick={onExit}>
              {copy.exit}
            </button>
            <button
              type="button"
              className="primary-cta h-9"
              disabled={!selected || saving}
              onClick={() => selected && void onConfirm(selected)}
            >
              {saving ? (isEnglish ? "Saving…" : "保存中…") : copy.confirm}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
