import { notifyModCatalogChanged } from "../modCapsules/catalogChanges";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { taskGateway } from "../tasks/gateway";
import { subscribeBeforeReadingTasks } from "../tasks/taskSync";
import type { TaskSnapshot } from "../tasks/types";
import { modResourcesGateway } from "./gateway";
import { latestResourceTasks, resourceDownloadUrl, resourceTaskMatches } from "./model";
import type { ModResourceAsset, ModResourceState, ResourceFeedback } from "./types";

interface Options {
  edition: string;
  onInstalled?: () => Promise<unknown>;
  onBusy?: (busy: boolean) => void;
}

export function useModResources({ edition, onInstalled, onBusy }: Options) {
  const [data, setData] = useState<ModResourceState | null>(null);
  const [checking, setChecking] = useState(false);
  const [readError, setReadError] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<ResourceFeedback | null>(null);
  const [activeAsset, setActiveAsset] = useState<string | null>(null);
  const [tasks, setTasks] = useState<TaskSnapshot[]>([]);
  const [taskError, setTaskError] = useState<string | null>(null);
  const mounted = useRef(false);
  const context = useRef(edition);
  context.current = edition;
  const readRevision = useRef(0);
  const operation = useRef(false);
  const completed = useRef(new Set<number>());
  const installedCallback = useRef(onInstalled);
  installedCallback.current = onInstalled;
  const runningTask = tasks.find(task => task.state === "running");
  const taskByAsset = useMemo(() => latestResourceTasks(tasks, edition), [tasks, edition]);
  const busy = activeAsset !== null || !!runningTask;

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; readRevision.current += 1; };
  }, []);

  const read = useCallback(async (refresh = false) => {
    const revision = ++readRevision.current;
    setChecking(true);
    try {
      const state = await modResourcesGateway.read(edition, refresh);
      if (!mounted.current || revision !== readRevision.current || context.current !== edition) return;
      setData(state);
      setReadError(null);
    } catch (error) {
      if (mounted.current && revision === readRevision.current && context.current === edition) {
        setReadError(String(error));
      }
    } finally {
      if (mounted.current && revision === readRevision.current) setChecking(false);
    }
  }, [edition]);

  useEffect(() => {
    setData(null);
    setFeedback(null);
    setReadError(null);
    void read();
    return () => { readRevision.current += 1; };
  }, [read]);

  useEffect(() => {
    let live = true;
    let stop: (() => void) | undefined;
    void subscribeBeforeReadingTasks(taskGateway, snapshot => {
      if (live) setTasks([...snapshot.values()]
        .filter(task => task.kind === "mod-resource-install")
        .sort((left, right) => right.task_id - left.task_id));
    }).then(unsubscribe => {
      if (live) stop = unsubscribe;
      else unsubscribe();
    }).catch(error => {
      if (live) {
        setTaskError(String(error));
        // A failed subscription cannot keep an unobserved running task locked forever.
        setTasks([]);
      }
    });
    return () => { live = false; stop?.(); };
  }, []);

  useEffect(() => {
    if (operation.current) return; // The initiating command refreshes its own completed result.
    const succeeded = [...taskByAsset.values()].filter(task =>
      task.state === "succeeded" && !completed.current.has(task.task_id));
    if (!succeeded.length) return;
    succeeded.forEach(task => completed.current.add(task.task_id));
    void read();
    void installedCallback.current?.().catch(error => {
      if (mounted.current) setReadError(String(error));
    });
  }, [taskByAsset, read]);

  useEffect(() => {
    onBusy?.(busy);
    return () => onBusy?.(false);
  }, [busy, onBusy]);

  const install = async (asset: ModResourceAsset, manual: boolean) => {
    if (operation.current || runningTask) return;
    operation.current = true;
    setActiveAsset(asset.id);
    setFeedback(null);
    const afterTaskId = tasks.reduce((latest, task) => Math.max(latest, task.task_id), 0);
    const current = () => mounted.current && context.current === edition;
    try {
      const localFile = manual ? await modResourcesGateway.chooseFile() : null;
      if (manual && localFile === null) return;
      // Navigating while the file picker is open must not install into a stale edition.
      if (!current()) return;
      const result = await modResourcesGateway.install(edition, asset.id, localFile);
      notifyModCatalogChanged();
      if (!current()) return;
      setFeedback({ assetId: asset.id, installedPath: result.path });
      await read();
      await installedCallback.current?.().catch(error => {
        if (current()) setReadError(String(error));
      });
    } catch (error) {
      if (current()) setFeedback({ assetId: asset.id, error: String(error), installAfterTaskId: afterTaskId });
    } finally {
      operation.current = false;
      if (mounted.current) setActiveAsset(null);
    }
  };

  const action = async (assetId: string, perform: () => Promise<unknown>) => {
    try { await perform(); }
    catch (error) {
      if (mounted.current && context.current === edition) setFeedback({ assetId, error: String(error) });
    }
  };

  return {
    data, checking, readError, feedback, taskError, activeAsset, busy, taskByAsset,
    runningElsewhere: !!runningTask && !resourceTaskMatches(runningTask, edition),
    refresh: () => read(true),
    install,
    openExternal: (asset: ModResourceAsset) => action(asset.id, () =>
      modResourcesGateway.openExternal(resourceDownloadUrl(asset, data?.preferred_source))),
    openFolder: (asset: ModResourceAsset) => action(asset.id, () =>
      modResourcesGateway.openFolder(edition)),
    cancel: (asset: ModResourceAsset, task: TaskSnapshot) => action(asset.id, () => taskGateway.cancel(task.task_id)),
  };
}

export type ModResourcesController = ReturnType<typeof useModResources>;
