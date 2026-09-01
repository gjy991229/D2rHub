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

  const scan = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await invokeCommand<ModCapsulePool>("scan_mod_capsule_pool");
      setPool(next);
      return next;
    } catch (reason) {
      setError(String(reason));
      return null;
    } finally {
      setLoading(false);
    }
  }, []);

  const mutate = useCallback(async (
    command: "add_mod_capsule" | "update_mod_capsule" | "delete_mod_capsule",
    payload: Record<string, unknown>,
  ) => {
    setLoading(true);
    setError(null);
    try {
      const next = await invokeCommand<ModCapsulePool>(command, payload);
      setPool(next);
      await onAssigned?.();
      return next;
    } catch (reason) {
      setError(String(reason));
      throw reason;
    } finally {
      setLoading(false);
    }
  }, [onAssigned]);

  const assign = useCallback(async (accountId: string, capsuleId: string | null) => {
    setAssigningAccountId(accountId);
    setError(null);
    try {
      await invokeCommand("assign_mod_capsule_to_account", { accountId, capsuleId });
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
    scan,
    add: (edition: string, launchArguments: string) => mutate("add_mod_capsule", { edition, launchArguments }),
    update: (capsuleId: string, launchArguments: string) => mutate("update_mod_capsule", { capsuleId, launchArguments }),
    remove: (capsuleId: string) => mutate("delete_mod_capsule", { capsuleId }),
    assign,
  };
}

export type ModCapsuleController = ReturnType<typeof useModCapsulePool>;
