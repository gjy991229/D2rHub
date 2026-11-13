import { useCallback, useEffect, useRef, useState } from "react";
import { invokeCommand, listenEvent } from "../../platform/tauri";
import type { ModCapsulePool, ModUnpackProgress, ModUnpackResult } from "../../store/types";
import { notifyModCatalogChanged, subscribeModCatalogChanges } from "./catalogChanges";

interface UseModCapsulePoolOptions {
  active: boolean;
  onAssigned?: () => Promise<void> | void;
}

export function useModCapsulePool({ active, onAssigned }: UseModCapsulePoolOptions) {
  const [pool, setPool] = useState<ModCapsulePool | null>(null);
  const [loading, setLoading] = useState(false);
  const [assigningAccountId, setAssigningAccountId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [unpackingCapsuleId, setUnpackingCapsuleId] = useState<string | null>(null);
  const [unpackProgress, setUnpackProgress] = useState<ModUnpackProgress | null>(null);
  const [unpackResult, setUnpackResult] = useState<ModUnpackResult | null>(null);
  const unpackLock = useRef(false);
  const poolRevision = useRef(0);
  const owner = useRef(Symbol());
  const refreshInFlight = useRef<Promise<ModCapsulePool | null> | null>(null);

  const refresh = useCallback((): Promise<ModCapsulePool | null> => {
    if (refreshInFlight.current) return refreshInFlight.current;
    const version = poolRevision.current;
    const request = (async () => {
      setLoading(true);
      setError(null);
      try {
        const next = await invokeCommand<ModCapsulePool>("get_mod_capsule_pool");
        if (version === poolRevision.current) setPool(next);
        return next;
      } catch (reason) {
        setError(String(reason));
        return null;
      } finally {
        setLoading(false);
      }
    })();
    refreshInFlight.current = request;
    void request.finally(() => { refreshInFlight.current = null; });
    return request;
  }, []);

  useEffect(() => {
    if (!active) return;
    void refresh();
  }, [active, refresh]);

  useEffect(() => subscribeModCatalogChanges(() => {
    poolRevision.current += 1;
    if (!active) {
      setPool(null);
      return;
    }
    // A read begun before the mutation must not repopulate the stale list.
    void (async () => {
      await refreshInFlight.current;
      await refresh();
    })();
  }, owner.current), [active, refresh]);

  const scan = useCallback(async () => {
    const version = ++poolRevision.current;
    setLoading(true);
    setError(null);
    try {
      const next = await invokeCommand<ModCapsulePool>("scan_mod_capsule_pool");
      notifyModCatalogChanged(owner.current);
      if (version === poolRevision.current) setPool(next);
      return next;
    } catch (reason) {
      setError(String(reason));
      return null;
    } finally {
      setLoading(false);
    }
  }, []);

  const mutate = useCallback(async (
    command:
      | "add_mod_capsule"
      | "update_mod_capsule"
      | "delete_mod_capsule"
      | "set_mod_auto_exit_on_death_enabled",
    payload: Record<string, unknown>,
  ) => {
    setLoading(true);
    setError(null);
    try {
      poolRevision.current += 1;
      const next = await invokeCommand<ModCapsulePool>(command, payload);
      notifyModCatalogChanged(owner.current);
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

  const unpack = useCallback(async (capsuleId: string) => {
    if (unpackLock.current) throw new Error("MPQ 解压任务正在进行中");
    unpackLock.current = true;
    setUnpackingCapsuleId(capsuleId);
    setUnpackProgress(null);
    setUnpackResult(null);
    setError(null);
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listenEvent<ModUnpackProgress>("mod-mpq-progress", ({ payload }) => {
        if (payload.capsule_id === capsuleId) setUnpackProgress(payload);
      });
      // An earlier read must finish before the mutation, or it could restore stale buttons.
      await refreshInFlight.current;
      const result = await invokeCommand<ModUnpackResult>("unpack_mod_capsule", { capsuleId });
      notifyModCatalogChanged(owner.current);
      poolRevision.current += 1;
      setPool(result.pool);
      setUnpackResult(result);
      return result;
    } catch (reason) {
      // A cancelled process may already have committed or rolled back. Rescan either way.
      poolRevision.current += 1;
      await refreshInFlight.current;
      await refresh();
      setError(String(reason));
      throw reason;
    } finally {
      unlisten?.();
      setUnpackingCapsuleId(null);
      setUnpackProgress(null);
      unpackLock.current = false;
    }
  }, [refresh]);

  const cancelUnpack = useCallback(async () => {
    if (unpackProgress) await invokeCommand("cancel_task", { taskId: unpackProgress.task_id });
  }, [unpackProgress]);

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
    unpack, unpackingCapsuleId, unpackProgress, unpackResult, cancelUnpack,
    loading,
    assigningAccountId,
    error,
    refresh,
    scan,
    openDirectory: (edition: string) => invokeCommand<void>("open_mods_directory", { edition }),
    add: (edition: string, launchArguments: string) => mutate("add_mod_capsule", { edition, launchArguments }),
    update: (capsuleId: string, launchArguments: string) => mutate("update_mod_capsule", { capsuleId, launchArguments }),
    remove: (capsuleId: string) => mutate("delete_mod_capsule", { capsuleId }),
    setAutoExitOnDeathEnabled: (capsuleId: string, enabled: boolean) => mutate(
      "set_mod_auto_exit_on_death_enabled",
      { capsuleId, enabled },
    ),
    assign,
  };
}

export type ModCapsuleController = Omit<ReturnType<typeof useModCapsulePool>, "openDirectory"> & {
  openDirectory?: ReturnType<typeof useModCapsulePool>["openDirectory"];
};
