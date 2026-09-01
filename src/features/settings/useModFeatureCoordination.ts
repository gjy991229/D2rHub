import { useCallback } from "react";

import { showToast } from "../../components/ui/Toast";
import { useGlobalConfig } from "../../store/globalConfig";
import type { AccountMeta, GlobalConfig } from "../../store/types";
import {
  AUDIO_TELEMETRY_CAPSULE_FEATURE,
  ROOM_TOOLS_CAPSULE_FEATURE,
  selectedCapsuleForAccount,
} from "../modCapsules/model";
import type { ModCapsuleController } from "../modCapsules/useModCapsulePool";
import { roomAutomationGateway } from "../roomAutomation/gateway";
import type { ModProcessingPurpose } from "./panels/ModProcessingPanel";

interface CoordinationOptions {
  accounts: AccountMeta[];
  trackingTargetId: string;
  modCatalog: ModCapsuleController;
  toggleAudio: (enabled: boolean, accountId?: string) => Promise<void>;
  openProcessing: (accountId: string, purpose: Exclude<ModProcessingPurpose, "manage">, autoStart?: boolean) => void;
  onGlobalCommitted: (config: GlobalConfig) => void;
}

export function useModFeatureCoordination({
  accounts,
  trackingTargetId,
  modCatalog,
  toggleAudio,
  openProcessing,
  onGlobalCommitted,
}: CoordinationOptions) {
  const prepareFeature = useCallback(async (
    accountId: string,
    capsuleId: string,
    purpose: "recognition" | "room-tools",
    autoStart = false,
  ) => {
    const capsule = modCatalog.pool?.capsules.find((entry) => entry.id === capsuleId);
    if (!capsule) {
      showToast("error", "选择的 Mod 已不在共享池中，请重新扫描");
      return;
    }
    const requiredFeature = purpose === "recognition"
      ? AUDIO_TELEMETRY_CAPSULE_FEATURE
      : ROOM_TOOLS_CAPSULE_FEATURE;
    const assigned = await modCatalog.assign(accountId, capsule.id);
    if (!assigned) return;
    if (capsule.feature_groups.includes(requiredFeature)) {
      if (purpose === "recognition") await toggleAudio(true, accountId);
      return;
    }
    openProcessing(accountId, purpose, autoStart && capsule.processed);
  }, [modCatalog, openProcessing, toggleAudio]);

  const toggleRecognition = useCallback(async (enabled: boolean): Promise<boolean> => {
    if (!enabled) {
      await toggleAudio(false);
      return true;
    }
    let preferredAccountId = trackingTargetId || accounts.find((account) => account.initialized)?.id || "";
    let roomEnabled = false;
    try {
      const room = await roomAutomationGateway.getConfig();
      roomEnabled = room.config.enabled && !!room.config.primary_account_id;
      if (roomEnabled) preferredAccountId = room.config.primary_account_id;
    } catch {
      // Recognition can still be configured when the optional room module is unavailable.
    }
    if (!preferredAccountId) {
      await toggleAudio(true);
      return true;
    }
    const selected = selectedCapsuleForAccount(modCatalog.pool, preferredAccountId);
    if (selected?.feature_groups.includes(AUDIO_TELEMETRY_CAPSULE_FEATURE)) {
      await toggleAudio(true, preferredAccountId);
      return true;
    }
    if (roomEnabled && selected) {
      await prepareFeature(preferredAccountId, selected.id, "recognition", true);
      return true;
    }
    return false;
  }, [accounts, modCatalog.pool, prepareFeature, toggleAudio, trackingTargetId]);

  const saveRoomLaunchScheme = useCallback(async (accountIds: string[]) => {
    const current = useGlobalConfig.getState().config;
    if (!current || !accountIds.length) return;
    const primary = accounts.find((account) => account.id === accountIds[0]);
    const baseName = `自动跟房 · ${primary?.display_name?.trim() || primary?.id || "主号"}`;
    const usedNames = new Set(current.launch_groups.map((group) => group.name.trim().toLocaleLowerCase()));
    let name = baseName;
    for (let suffix = 2; usedNames.has(name.toLocaleLowerCase()); suffix += 1) name = `${baseName} ${suffix}`;
    const group = {
      id: crypto.randomUUID(),
      name,
      account_ids: [...accountIds],
      members: accountIds.map((accountId) => ({
        account_id: accountId,
        mod_args: accounts.find((account) => account.id === accountId)?.mod_args ?? "",
        position_configured: false,
        graphics_configured: false,
      })),
    };
    try {
      const saved = await useGlobalConfig.getState().patch({ launch_groups: [...current.launch_groups, group] });
      onGlobalCommitted(saved);
      showToast("success", `已保存启动方案“${name}”`);
    } catch (error) {
      showToast("error", `保存启动方案失败：${error}`);
    }
  }, [accounts, onGlobalCommitted]);

  return { prepareFeature, toggleRecognition, saveRoomLaunchScheme };
}
