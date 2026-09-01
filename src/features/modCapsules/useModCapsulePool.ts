import { useCallback, useEffect, useState } from "react";
import { invokeCommand } from "../../platform/tauri";
import type { ModCapsulePool } from "../../store/types";

interface UseModCapsulePoolOptions {
  active: boolean;
  onAssigned?: () => Promise<void> | void;
}

export function useModCapsulePool({ active, onAssigned }: UseModCapsulePoolOptions) {
  const [pool, setPool] = useState<ModCapsulePool | null>(null);
  const [loading, setLoading] = useState(false);
  const [assigningAccountId, setAssigningAccountId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await invokeCommand<ModCapsulePool>("get_mod_capsule_pool");
      setPool(next);
      return next;
    } catch (reason) {
      setError(String(reason));
      return null;
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    void refresh();
  }, [active, refresh]);

  const assign = useCallback(async (accountId: string, modName: string) => {
    setAssigningAccountId(accountId);
    setError(null);
    try {
      await invokeCommand("apply_audio_mod_to_account", { accountId, modName });
      await onAssigned?.();
      return await refresh();
    } catch (reason) {
      setError(String(reason));
      return null;
    } finally {
      setAssigningAccountId(null);
    }
  }, [onAssigned, refresh]);

  return {
    pool,
    loading,
    assigningAccountId,
    error,
    refresh,
    assign,
  };
}
