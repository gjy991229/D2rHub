import { useEffect, useState } from "react";
import { useGlobalConfig } from "../../store/globalConfig";
import type { ThemeKey } from "../../store/theme";
import { normalizeTheme } from "../../store/themeCatalog";
import type { GlobalConfig } from "../../store/types";
import type { AppearanceSettingsDraft } from "./panels/AppearancePanel";

export function appearanceFromConfig(config: GlobalConfig): AppearanceSettingsDraft {
  return {
    app_language: config.app_language,
    theme: normalizeTheme(config.theme),
    main_opacity: config.main_opacity ?? 95,
    font_scale: config.font_scale || "default",
    separate_game_taskbar_icons: !!config.separate_game_taskbar_icons,
  };
}

export function appearanceSettingsEqual(
  config: GlobalConfig | null,
  draft: AppearanceSettingsDraft | null,
): boolean {
  return !!config && !!draft
    && JSON.stringify(appearanceFromConfig(config)) === JSON.stringify(draft);
}

interface AppearanceSettingsControllerParams {
  open: boolean;
  config: GlobalConfig | null;
  persistConfig: (draft: GlobalConfig, quiet?: boolean) => Promise<GlobalConfig | null>;
  previewTheme: (theme: ThemeKey) => void;
}

/**
 * 外观草稿、即时预览与应用/回滚的编排。
 *
 * 预览会先落地到 store 与 DOM 再持久化，失败时必须整体回滚，
 * 因此草稿、主题预览与持久化只能由同一处持有。
 */
export function useAppearanceSettingsController({
  open,
  config,
  persistConfig,
  previewTheme,
}: AppearanceSettingsControllerParams) {
  const [draft, setDraft] = useState<AppearanceSettingsDraft | null>(null);
  const [applying, setApplying] = useState(false);

  // 只在设置面板打开时用磁盘配置重置草稿，避免编辑过程中被外部更新打断。
  useEffect(() => {
    if (open && config) setDraft(appearanceFromConfig(config));
  }, [open]);

  const apply = async (quiet = false): Promise<boolean> => {
    const current = useGlobalConfig.getState().config;
    if (!current || !draft) return true;
    const next: GlobalConfig = {
      ...current,
      app_language: draft.app_language,
      theme: draft.theme,
      main_opacity: draft.main_opacity,
      font_scale: draft.font_scale,
      separate_game_taskbar_icons: draft.separate_game_taskbar_icons,
    };

    setApplying(true);
    useGlobalConfig.setState({ config: next });
    previewTheme(draft.theme);
    document.documentElement.dataset.fontScale = draft.font_scale;
    try { localStorage.setItem("d2rhub-font-scale", draft.font_scale); } catch {}
    const saved = await persistConfig(next, quiet);
    setApplying(false);
    if (!saved) {
      useGlobalConfig.setState({ config: current });
      previewTheme(normalizeTheme(current.theme));
      document.documentElement.dataset.fontScale = current.font_scale || "default";
      try { localStorage.setItem("d2rhub-font-scale", current.font_scale || "default"); } catch {}
      return false;
    }
    setDraft(appearanceFromConfig(saved));
    return true;
  };

  const hasChanges = !!config && !!draft && !appearanceSettingsEqual(config, draft);

  return { draft, setDraft, applying, hasChanges, apply };
}
