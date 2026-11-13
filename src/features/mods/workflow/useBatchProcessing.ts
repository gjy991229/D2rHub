import { notifyModCatalogChanged } from "../../modCapsules/catalogChanges";
import { useRef, useState, type RefObject } from "react";
import { invokeCommand, listenEvent } from "../../../platform/tauri";
import type { AccountMeta, AudioModSetupState } from "../../../store/types";
import { normalizeAccountRegion } from "../../../utils/regionPaths";
import type { ModCapsuleController } from "../../modCapsules/useModCapsulePool";
import { audioModFeatureInvokeOptions, type AudioModPrepareProgress, type AudioModFeatureSelection } from "../featureContract";
import { batchPlan, type BatchMode, type BatchRow } from "./batchModel";
import type { ModEdition } from "./types";

export function useBatchProcessing(accounts: AccountMeta[], catalog: ModCapsuleController, lock: RefObject<boolean>, en: boolean) {
  const [edition, setEdition] = useState<ModEdition>("CN");
  const [accountId, setAccountId] = useState("");
  const [mode, setMode] = useState<BatchMode>("create");
  const [features, setFeatures] = useState<AudioModFeatureSelection>({ includeAudioTelemetry: false, includeRoomTools: true, includeEscNextGame: false, includeAutoExitOnDeath: false });
  const [names, setNames] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<string[]>([]);
  const [inspection, setInspection] = useState<AudioModSetupState | null>(null);
  const [rows, setRows] = useState<BatchRow[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [progress, setProgress] = useState<AudioModPrepareProgress | null>(null);
  const [stopping, setStopping] = useState(false);
  const stop = useRef(false);
  const revision = useRef(0);
  const latest = useRef({ accounts, catalog }); latest.current = { accounts, catalog };
  const eligible = (value: AccountMeta, target: ModEdition) => {
    if (!value.initialized) return false;
    const region = normalizeAccountRegion(value.region);
    const savedEdition = latest.current.catalog.pool?.accounts.find(entry => entry.account_id === value.id)?.edition;
    return (region ? (region === "CN" ? "CN" : "Global") : savedEdition) === target;
  };
  const load = async (id: string) => {
    if (lock.current) return;
    const ticket = ++revision.current;
    setAccountId(id); setInspection(null); setSelected([]); setRows(null); setError("");
    if (!id) { setLoading(false); return; }
    setLoading(true);
    try {
      const state = await invokeCommand<AudioModSetupState>("get_audio_mod_setup_state", { accountId: id });
      if (ticket === revision.current) setInspection(state);
    } catch (cause) { if (ticket === revision.current) setError(String(cause)); }
    finally { if (ticket === revision.current) setLoading(false); }
  };
  const open = (target: ModEdition) => {
    if (lock.current) return;
    setEdition(target); setMode("create"); setNames({}); setProgress(null);
    void load(accounts.find(account => eligible(account, target))?.id ?? "");
  };
  const run = async (retry = false) => {
    if (lock.current || !rows || !accountId) return;
    const plan = rows.map(row => retry && (row.state === "failed" || row.state === "skipped") ? { ...row, state: "pending" as const, message: "" } : { ...row });
    if (!plan.some(row => row.state === "pending")) return;
    lock.current = true; setBusy(true); setError(""); setStopping(false); stop.current = false;
    let unlisten: (() => void) | undefined;
    const update = () => setRows(plan.map(row => ({ ...row })));
    try {
      unlisten = await listenEvent<AudioModPrepareProgress>("audio-mod-prepare-progress", event => {
        if (event.payload.account_id === accountId) setProgress(event.payload);
      });
      for (const row of plan) {
        if (row.state !== "pending") continue;
        if (stop.current) { row.state = "skipped"; row.message = en ? "Stopped before starting" : "已停止，尚未开始"; update(); continue; }
        row.state = "running"; row.message = ""; setProgress(null); update();
        try {
          if (!latest.current.accounts.some(account => account.id === accountId && eligible(account, edition))) throw new Error(en ? "Account configuration changed" : "目标账号配置已改变，请重新生成计划");
          const options = audioModFeatureInvokeOptions(row.features);
          if (row.mode === "create" || row.mode === "replace-source") {
            await invokeCommand("prepare_audio_mod", { accountId, modName: row.name, sourceModName: row.sourceModName, replaceSource: row.mode === "replace-source", ...options });
          } else {
            await invokeCommand("upgrade_audio_mod", { accountId, modName: row.name, sourceModName: row.sourceModName, forceRebuild: row.mode === "rebuild", ...options });
          }
          row.state = "success"; row.message = en ? "Ready in the Mod library" : "已完成，可在 Mod 库选用";
        } catch (cause) { row.state = "failed"; row.message = String(cause); }
        update();
      }
      // The next preview must see outputs created by this batch and updated recipes.
      setInspection(null);
      setInspection(await invokeCommand<AudioModSetupState>("get_audio_mod_setup_state", { accountId }));
      notifyModCatalogChanged();
      await latest.current.catalog.refresh();
    } catch (cause) { setError(String(cause)); }
    finally { unlisten?.(); lock.current = false; setBusy(false); setProgress(null); update(); }
  };
  return { edition, accountId, mode, features, names, selected, inspection, rows, busy, loading, error, progress, stopping,
    accounts: accounts.filter(account => eligible(account, edition)), open,
    chooseAccount: load,
    changeMode: (value: BatchMode) => { setMode(value); setSelected([]); setRows(null); },
    changeFeatures: (value: AudioModFeatureSelection) => { setFeatures(value); setRows(null); },
    changeName: (source: string, name: string) => { setNames(previous => ({ ...previous, [source]: name })); setRows(null); },
    select: (value: string[]) => { setSelected(value); setRows(null); },
    preview: () => { if (inspection) setRows(batchPlan(inspection.installed_mods, selected, mode, features, names, en)); },
    edit: () => setRows(null), run,
    stop: () => { stop.current = true; setStopping(true); },
  };
}
