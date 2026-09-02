import { useCallback, useEffect, useMemo, useState } from "react";

import { showToast } from "../../components/ui/Toast";
import type { SettingsMap } from "../../pages/SettingsEditor";
import { emitEvent, invokeCommand, listenEvent } from "../../platform/tauri";
import type { AccountMeta } from "../../store/types";
import { FRAMERATE_CAP_KEY, writeFramerateCap } from "../../utils/gameSettings";

type GameSettingsTab = "launch" | "game_display" | "game_graphics" | "game_audio" | "game_gameplay" | "game_automap";

interface AccountSettingsControllerOptions {
  accounts: AccountMeta[];
  loadAccounts: () => Promise<void>;
  renameAccount: (id: string, newName: string) => Promise<boolean>;
}

export function useAccountSettingsController({
  accounts,
  loadAccounts,
  renameAccount,
}: AccountSettingsControllerOptions) {
  const [selectedAccountId, setSelectedAccountId] = useState("");
  const [gameSettings, setGameSettings] = useState<SettingsMap>({});
  const [gameSettingsLoading, setGameSettingsLoading] = useState(false);
  const [gameSettingsLoadError, setGameSettingsLoadError] = useState<string | null>(null);
  const [gameSettingsChanged, setGameSettingsChanged] = useState(false);
  const [gameSettingsSaving, setGameSettingsSaving] = useState(false);
  const [gameSettingsTab, setGameSettingsTab] = useState<GameSettingsTab>("launch");
  const [accountNicknameDraft, setAccountNicknameDraft] = useState("");
  const [accountWinXDraft, setAccountWinXDraft] = useState<number | null>(null);
  const [accountWinYDraft, setAccountWinYDraft] = useState<number | null>(null);

  const loadGameSettings = useCallback(async (accountId: string) => {
    setGameSettingsLoading(true);
    setGameSettingsLoadError(null);
    try {
      const data = await invokeCommand<SettingsMap>("get_account_settings", { accountId });
      setGameSettings(data);
      setGameSettingsChanged(false);
    } catch (error) {
      setGameSettings({});
      setGameSettingsChanged(false);
      setGameSettingsLoadError(String(error));
      showToast("error", `加载账号游戏配置失败: ${error}`);
    } finally {
      setGameSettingsLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!selectedAccountId) return;
    const account = accounts.find((candidate) => candidate.id === selectedAccountId);
    if (!account) return;
    setAccountNicknameDraft(account.display_name || account.id);
    setAccountWinXDraft(account.window_x ?? null);
    setAccountWinYDraft(account.window_y ?? null);
    void loadGameSettings(selectedAccountId);
  }, [accounts, loadGameSettings, selectedAccountId]);

  useEffect(() => {
    if (!selectedAccountId) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listenEvent<{ accountId: string }>("account-settings-updated", (event) => {
      if (event.payload.accountId === selectedAccountId && !gameSettingsSaving) {
        void loadGameSettings(selectedAccountId);
      }
    }).then((stopListening) => {
      if (cancelled) stopListening();
      else unlisten = stopListening;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [gameSettingsSaving, loadGameSettings, selectedAccountId]);

  const updateGameSetting = (key: string, value: unknown) => {
    if (gameSettingsLoadError) return;
    setGameSettings((previous) => key === FRAMERATE_CAP_KEY
      ? writeFramerateCap(previous, Number(value))
      : ({ ...previous, [key]: value }));
    setGameSettingsChanged(true);
  };

  const saveAccount = async (quiet = false) => {
    if (!selectedAccountId) return true;
    setGameSettingsSaving(true);
    try {
      const account = accounts.find((candidate) => candidate.id === selectedAccountId);
      if (!account) return false;
      const nextName = accountNicknameDraft.trim();
      if (nextName && nextName !== (account.display_name || account.id)) {
        if (!(await renameAccount(selectedAccountId, nextName))) return false;
      }
      if (accountWinXDraft !== account.window_x || accountWinYDraft !== account.window_y) {
        await invokeCommand("set_account_window_position", {
          accountId: selectedAccountId,
          windowX: accountWinXDraft,
          windowY: accountWinYDraft,
        });
      }
      if (gameSettingsChanged) {
        await invokeCommand("save_account_settings", {
          accountId: selectedAccountId,
          settings: gameSettings,
        });
        setGameSettingsChanged(false);
        await emitEvent("account-settings-updated", { accountId: selectedAccountId });
      }
      await loadAccounts();
      if (!quiet) showToast("success", `账号 "${nextName || account.display_name || account.id}" 的设置已保存`);
      return true;
    } catch (error) {
      showToast("error", `保存账号设置失败: ${error}`);
      return false;
    } finally {
      setGameSettingsSaving(false);
    }
  };

  const snapshotSystemSettings = async () => {
    if (!selectedAccountId) return;
    try {
      const settings = await invokeCommand<SettingsMap>("snapshot_system_settings_to_account", {
        accountId: selectedAccountId,
      });
      setGameSettings(settings);
      setGameSettingsLoadError(null);
      setGameSettingsChanged(false);
      await loadAccounts();
      await emitEvent("account-settings-updated", { accountId: selectedAccountId });
      showToast("success", "已快照系统配置到当前账号");
    } catch (error) {
      showToast("error", `快照系统配置失败: ${error}`);
    }
  };

  const toggleCustomizedSettings = async (accountId: string, customized: boolean) => {
    try {
      if (customized) {
        await invokeCommand("snapshot_system_settings_to_account", { accountId });
      } else {
        await invokeCommand("set_settings_customized", { accountId, customized: false });
      }
      await loadAccounts();
      if (accountId === selectedAccountId) await loadGameSettings(accountId);
      await emitEvent("account-settings-updated", { accountId });
    } catch (error) {
      showToast("error", `切换配置模式失败: ${error}`);
    }
  };

  const selectedAccount = accounts.find((account) => account.id === selectedAccountId);
  const accountHasChanges = useMemo(() => {
    if (!selectedAccount) return false;
    return !!(
      (accountNicknameDraft.trim()
        && accountNicknameDraft.trim() !== (selectedAccount.display_name || selectedAccount.id))
      || accountWinXDraft !== (selectedAccount.window_x ?? null)
      || accountWinYDraft !== (selectedAccount.window_y ?? null)
      || gameSettingsChanged
    );
  }, [accountNicknameDraft, accountWinXDraft, accountWinYDraft, gameSettingsChanged, selectedAccount]);

  return {
    selectedAccountId,
    setSelectedAccountId,
    selectedAccount,
    accountHasChanges,
    accountNicknameDraft,
    setAccountNicknameDraft,
    accountWinXDraft,
    setAccountWinXDraft,
    accountWinYDraft,
    setAccountWinYDraft,
    gameSettings,
    gameSettingsLoading,
    gameSettingsLoadError,
    gameSettingsSaving,
    gameSettingsTab,
    setGameSettingsTab,
    loadGameSettings,
    updateGameSetting,
    saveAccount,
    snapshotSystemSettings,
    toggleCustomizedSettings,
  };
}
