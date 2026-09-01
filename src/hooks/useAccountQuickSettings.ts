import { useCallback, useEffect, useRef, useState } from "react";
import { emitEvent, invokeCommand, listenEvent } from "../platform/tauri";

import { useAccounts } from "../store/accounts";

export interface AccountQuickSettings {
  resolution: string;
  fps: number;
}

const DEFAULT_SETTINGS: AccountQuickSettings = {
  resolution: "1280x720",
  fps: 30,
};

type SettingsPatch = Partial<AccountQuickSettings>;
type SettingsSubscriber = () => void;

const settingsSubscribers = new Map<string, Set<SettingsSubscriber>>();
let settingsListenerPromise: Promise<void> | null = null;

function ensureSettingsListener(): void {
  if (settingsListenerPromise) return;

  settingsListenerPromise = listenEvent<{ accountId: string }>(
    "account-settings-updated",
    (event) => {
      settingsSubscribers.get(event.payload.accountId)?.forEach((subscriber) => subscriber());
    },
  ).then(() => undefined).catch((error) => {
    settingsListenerPromise = null;
    console.warn("Failed to subscribe to account settings updates", error);
  });
}

function subscribeToSettings(accountId: string, subscriber: SettingsSubscriber): () => void {
  const subscribers = settingsSubscribers.get(accountId) ?? new Set<SettingsSubscriber>();
  subscribers.add(subscriber);
  settingsSubscribers.set(accountId, subscribers);
  ensureSettingsListener();

  return () => {
    subscribers.delete(subscriber);
    if (subscribers.size === 0) settingsSubscribers.delete(accountId);
  };
}

function parseSettings(raw: Record<string, unknown>): AccountQuickSettings {
  return {
    resolution: String(raw["Screen Resolution (Windowed)"] ?? DEFAULT_SETTINGS.resolution),
    fps: Number(raw["Framerate Target"] ?? raw["Framerate Cap"] ?? DEFAULT_SETTINGS.fps),
  };
}

function applyPendingPatch(
  settings: AccountQuickSettings,
  patch: SettingsPatch,
): AccountQuickSettings {
  return { ...settings, ...patch };
}

function hasPatch(patch: SettingsPatch): boolean {
  return Object.keys(patch).length > 0;
}

export function useAccountQuickSettings(accountId: string, enabled: boolean) {
  const [settings, setSettings] = useState<AccountQuickSettings>(DEFAULT_SETTINGS);
  const [loaded, setLoaded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadedRef = useRef(false);
  const mountedRef = useRef(true);
  const loadPromiseRef = useRef<Promise<void> | null>(null);
  const savePromiseRef = useRef<Promise<void> | null>(null);
  const pendingPatchRef = useRef<SettingsPatch>({});
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const load = useCallback((force = false): Promise<void> => {
    if (loadedRef.current && !force) return Promise.resolve();
    if (loadPromiseRef.current) return loadPromiseRef.current;

    if (mountedRef.current) setLoading(true);
    const request = invokeCommand<Record<string, unknown>>("get_account_settings", { accountId })
      .then((raw) => {
        if (!mountedRef.current) return;
        setSettings(applyPendingPatch(parseSettings(raw), pendingPatchRef.current));
        loadedRef.current = true;
        setLoaded(true);
        setError(null);
      })
      .catch((loadError) => {
        if (mountedRef.current) {
          loadedRef.current = false;
          setLoaded(false);
          setError(String(loadError));
        }
        throw loadError;
      })
      .finally(() => {
        loadPromiseRef.current = null;
        if (mountedRef.current) setLoading(false);
      });

    loadPromiseRef.current = request;
    return request;
  }, [accountId]);

  const flush = useCallback((): Promise<void> => {
    if (saveTimerRef.current) {
      clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
    }

    if (savePromiseRef.current) return savePromiseRef.current;
    const patch = pendingPatchRef.current;
    if (!hasPatch(patch)) return Promise.resolve();
    pendingPatchRef.current = {};

    const request = (async () => {
      try {
        const raw = await invokeCommand<Record<string, unknown>>("get_account_settings", { accountId });
        const merged = { ...raw };
        if (patch.resolution !== undefined) {
          merged["Screen Resolution (Windowed)"] = patch.resolution;
        }
        if (patch.fps !== undefined) {
          merged["Framerate Target"] = patch.fps;
        }

        await invokeCommand("save_account_settings", { accountId, settings: merged });
        await invokeCommand("mark_settings_customized", { accountId });

        const account = useAccounts.getState().accounts.find((candidate) => candidate.id === accountId);
        if (account && !account.has_customized_settings) {
          useAccounts.getState().updateAccount({ ...account, has_customized_settings: true });
        }

        await emitEvent("account-settings-updated", { accountId });
        if (mountedRef.current) setError(null);
      } catch (saveError) {
        // Preserve edits made while this request was running; newer values win.
        pendingPatchRef.current = { ...patch, ...pendingPatchRef.current };
        if (mountedRef.current) setError(String(saveError));
        throw saveError;
      } finally {
        savePromiseRef.current = null;
      }

      // An edit may have arrived while the first request was in flight.
      if (hasPatch(pendingPatchRef.current)) await flush();
    })();

    savePromiseRef.current = request;
    return request;
  }, [accountId]);

  const update = useCallback((patch: SettingsPatch) => {
    setSettings((current) => applyPendingPatch(current, patch));
    pendingPatchRef.current = { ...pendingPatchRef.current, ...patch };
    setError(null);

    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    saveTimerRef.current = setTimeout(() => {
      void flush().catch(() => undefined);
    }, 600);
  }, [flush]);

  useEffect(() => {
    if (!enabled) return;
    void load().catch(() => undefined);
  }, [enabled, load]);

  useEffect(() => subscribeToSettings(accountId, () => {
    if (loadedRef.current) void load(true).catch(() => undefined);
  }), [accountId, load]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
      if (hasPatch(pendingPatchRef.current)) void flush().catch(() => undefined);
    };
  }, [flush]);

  return {
    settings,
    loaded,
    loading,
    error,
    load,
    update,
    flush,
  };
}
