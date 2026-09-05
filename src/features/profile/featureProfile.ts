import type { GlobalConfig } from "../../store/types";

export type FeatureProfile = "normal" | "minimal";

export const CURRENT_FEATURE_PROFILE_PROMPT_REVISION = 1;

export function normalizeFeatureProfile(value: string | null | undefined): FeatureProfile {
  return value === "minimal" ? "minimal" : "normal";
}

export function isFeatureProfileDecisionCurrent(config: GlobalConfig | null | undefined): boolean {
  return (config?.feature_profile === "normal" || config?.feature_profile === "minimal")
    && (config.feature_profile_prompt_revision ?? 0) >= CURRENT_FEATURE_PROFILE_PROMPT_REVISION;
}

export function isMinimalMode(config: GlobalConfig | null | undefined): boolean {
  return isFeatureProfileDecisionCurrent(config)
    && normalizeFeatureProfile(config?.feature_profile) === "minimal";
}

export function optionalFeaturesAreAvailable(config: GlobalConfig | null | undefined): boolean {
  return isFeatureProfileDecisionCurrent(config)
    && normalizeFeatureProfile(config?.feature_profile) === "normal";
}
