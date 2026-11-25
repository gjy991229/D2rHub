import { useRef, useState, type Dispatch, type SetStateAction } from "react";
import { normalizeTheme } from "../../store/themeCatalog";
import type { GlobalConfig } from "../../store/types";
import type { AppearanceSettingsDraft } from "./panels/AppearancePanel";

export function appearanceFromConfig(config: GlobalConfig): AppearanceSettingsDraft {
  return {
    app_language: config.app_language,
    theme: normalizeTheme(config.theme),
    main_opacity: config.main_opacity ?? 95,
    ui_motion: config.ui_motion ?? "full",
    font_scale: config.font_scale || "default",
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
  committedConfig: GlobalConfig | null;
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
  persistConfig: (draft: GlobalConfig, quiet?: boolean) => Promise<GlobalConfig | null>;
}

/**
 * Appearance shares the settings edit session. Theme and font effects follow
 * committed configuration, so a failed save never needs a global rollback.
 */
export function useAppearanceSettingsController({
  config,
  committedConfig,
  updateConfig,
  persistConfig,
}: AppearanceSettingsControllerParams) {
  const [applying, setApplying] = useState(false);
  const applyingRef = useRef(false);
  const draft = config ? appearanceFromConfig(config) : null;
  const setDraft: Dispatch<SetStateAction<AppearanceSettingsDraft | null>> = (action) => {
    updateConfig(current => {
      const next = typeof action === "function" ? action(appearanceFromConfig(current)) : action;
      if (next) Object.assign(current, next);
    });
  };

  const apply = async (quiet = false): Promise<boolean> => {
    if (!config || !draft) return true;
    if (applyingRef.current) return false;
    applyingRef.current = true;
    setApplying(true);
    try {
      return !!(await persistConfig(config, quiet));
    } finally {
      applyingRef.current = false;
      setApplying(false);
    }
  };

  const hasChanges = !!committedConfig && !!draft && !appearanceSettingsEqual(committedConfig, draft);

  return { draft, setDraft, applying, hasChanges, apply };
}
