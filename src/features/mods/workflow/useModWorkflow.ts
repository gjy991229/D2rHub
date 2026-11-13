import { notifyModCatalogChanged } from "../../modCapsules/catalogChanges";
import { useCallback, useEffect, useRef, useState } from "react";
import { invokeCommand, listenEvent } from "../../../platform/tauri";
import { showToast } from "../../../components/ui/Toast";
import type { AccountMeta, AudioModSetupState } from "../../../store/types";
import { normalizeAccountRegion } from "../../../utils/regionPaths";
import type { ModCapsuleController } from "../../modCapsules/useModCapsulePool";
import { selectedCapsuleForAccount } from "../../modCapsules/model";
import { audioModFeatureInvokeOptions, type AudioModPrepareProgress, type AudioModPrepareResult } from "../featureContract";
import { describeModDraft, initialModFeatures } from "./model";
import { useBatchProcessing } from "./useBatchProcessing";
import { useModInspection } from "./useModInspection";
import { useModPreparationTask } from "./useModPreparationTask";
import { installationTargetId } from "../../../utils/recognitionSource";
import type { ModAppliedResult, ModEdition, ModProcessingDraft, ModProcessingRequest, ModRecipe, ModWorkflowOrigin } from "./types";

interface Options {
  open: boolean;
  active: boolean;
  accounts: AccountMeta[];
  catalog: ModCapsuleController;
  language?: string | null;
  optionalFeaturesAvailable: boolean;
  installationPaths?: Partial<Record<ModEdition, string>>;
  onNavigate: (destination: ModWorkflowOrigin) => void;
  onApplied: (result: ModAppliedResult) => Promise<void>;
}
interface PreparedReceipt { key: string; modName: string; applied?: AudioModSetupState }

/** Owns a processing session; recognition and room features submit explicit requests. */
export function useModWorkflow(options: Options) {
  const latest = useRef(options);
  latest.current = options;
  const [view, setView] = useState<"library" | "processing" | "batch">("library");
  const [libraryEdition, setLibraryEdition] = useState<ModEdition>("CN");
  const [openAdd, setOpenAdd] = useState(false);
  const [draft, setDraft] = useState<ModProcessingDraft | null>(null);
  const initialDraft = useRef<string | null>(null);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const accountDrafts = useRef(new Map<string, ModProcessingDraft>());
  const [busy, setBusy] = useState(false);
  const operation = useRef(false);
  const [progress, setProgress] = useState<AudioModPrepareProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [receipt, setReceipt] = useState<PreparedReceipt | null>(null);
  const receiptRef = useRef(receipt);
  receiptRef.current = receipt;
  const [autoStart, setAutoStart] = useState(false);
  const [processorReady, setProcessorReady] = useState<boolean | null>(null);
  const target = options.accounts.find(account => account.id === draft?.accountId);
  const inspectionIdentity = JSON.stringify([draft?.installationOnly, draft?.edition, options.installationPaths?.[draft?.edition ?? "CN"], target?.initialized, target?.region, target?.mod_args, target?.is_running, target?.running_pid, options.catalog.pool?.scanned_at]);
  const inspectionTargetId = draft?.installationOnly ? installationTargetId(draft.edition) : draft?.accountId ?? "";
  const inspection = useModInspection(options.open && options.active && view === "processing" && (draft?.installationOnly === true || target?.initialized === true),
    inspectionTargetId, inspectionIdentity, draft?.installationOnly ? { edition: draft.edition } : undefined);
  const preparationTask = useModPreparationTask({ accountId: inspectionTargetId, busy });
  const observedTask = useRef(preparationTask.currentTask);
  observedTask.current = preparationTask.currentTask;
  const en = options.language === "en-US";
  const batch = useBatchProcessing(options.accounts, options.catalog, operation, en);
  const analysis = draft ? describeModDraft(draft, inspection.state, en) : null;
  const prepared = !!draft && receipt?.key === JSON.stringify(draft);
  const blockedReason = prepared ? "" : inspection.error
    ? (en ? "Inspection failed. Rescan to retry." : "检查失败，请重新扫描后重试。")
    : inspection.loading ? (en ? "Inspecting installed Mods…" : "正在检查已安装 Mod…")
      : analysis?.blockedReason ?? "";

  const writeDraft = useCallback((next: ModProcessingDraft) => {
    draftRef.current = next;
    setDraft(next);
    setError(null);
    setNotice(null);
    accountDrafts.current.set(`${next.origin}:${next.accountId}`, next);
  }, []);

  const discardDraft = useCallback(() => {
    if (operation.current) return;
    draftRef.current = null; setDraft(null); accountDrafts.current.clear();
    initialDraft.current = null; receiptRef.current = null; setReceipt(null);
    setProgress(null); setError(null); setNotice(null); setAutoStart(false);
    setView("library");
  }, []);

  const openLibrary = useCallback((edition?: ModEdition, add = false) => {
    if (operation.current) return;
    if (draftRef.current && JSON.stringify(draftRef.current) === initialDraft.current && !receiptRef.current && !progress && !error && !notice) discardDraft();
    if (edition) setLibraryEdition(edition);
    setOpenAdd(add);
    setView("library");
    latest.current.onNavigate("library");
  }, [discardDraft, progress, error, notice]);

  const editionForAccount = (account: AccountMeta | undefined): ModEdition | undefined => {
    const region = normalizeAccountRegion(account?.region);
    if (region) return region === "CN" ? "CN" : "Global";
    const edition = latest.current.catalog.pool?.accounts.find(entry => entry.account_id === account?.id)?.edition;
    if (edition === "CN" || edition === "Global") return edition;
    return undefined;
  };

  const requestProcessing = useCallback((request: ModProcessingRequest) => {
    if (operation.current) return;
    const { accounts, catalog } = latest.current;
    const initialized = accounts.filter(account => account.initialized);
    const requestedEdition = request.edition ?? editionForAccount(initialized.find(account => account.id === request.accountId));
    const candidates = initialized.filter(account => !!editionForAccount(account)
      && (!requestedEdition || editionForAccount(account) === requestedEdition));
    const account = request.installationOnly ? undefined : request.accountId ? candidates.find(account => account.id === request.accountId) : candidates[0];
    const installationOnly = request.installationOnly || (!account && request.origin === "library");
    const edition = (requestedEdition ?? editionForAccount(account)) === "Global" ? "Global" : "CN";
    const existing = request.origin !== "library" && account ? selectedCapsuleForAccount(catalog.pool, account.id) : null;
    const source = request.source ?? (existing ? { name: existing.name, processed: existing.processed || existing.update_required } : undefined);
    const next: ModProcessingDraft = {
      origin: request.origin, accountId: installationOnly ? "" : account?.id ?? "", edition,
      ...(installationOnly ? { installationOnly: true } : {}),
      recipe: source?.processed
        ? { kind: "augment", modName: source.name }
        : { kind: "create", source: source?.name ?? null, name: "" },
      features: initialModFeatures(request.origin),
    };
    accountDrafts.current.clear();
    initialDraft.current = JSON.stringify(next);
    writeDraft(next);
    setReceipt(null);
    setProgress(null);
    setProcessorReady(null);
    setAutoStart(request.autoStart === true && next.recipe.kind === "augment");
    setLibraryEdition(edition);
    setView("processing");
    latest.current.onNavigate("library");
  }, [writeDraft]);

  const chooseTarget = useCallback((accountId: string) => {
    if (operation.current || !draftRef.current) return;
    const previous = draftRef.current;
    const account = latest.current.accounts.find(account => account.id === accountId && account.initialized);
    const edition = editionForAccount(account);
    if (!account || !edition) return;
    const saved = accountDrafts.current.get(`${previous.origin}:${accountId}`);
    writeDraft(saved ?? { ...previous, accountId, edition,
      recipe: edition === previous.edition ? previous.recipe : { kind: "create", source: null, name: "" } });
    setAutoStart(false);
    setProgress(null);
  }, [writeDraft]);

  const changeRecipe = useCallback((recipe: ModRecipe) => {
    if (!operation.current && draftRef.current) { writeDraft({ ...draftRef.current, recipe }); setAutoStart(false); }
  }, [writeDraft]);
  const changeFeatures = useCallback((patch: Partial<ModProcessingDraft["features"]>) => {
    if (!operation.current && draftRef.current) { writeDraft({ ...draftRef.current, features: { ...draftRef.current.features, ...patch } }); setAutoStart(false); }
  }, [writeDraft]);

  const back = useCallback(() => {
    if (operation.current) return;
    setAutoStart(false);
    const origin = draftRef.current?.origin ?? "library";
    if (JSON.stringify(draftRef.current) === initialDraft.current && !receiptRef.current && !progress && !error && !notice) discardDraft();
    if (origin === "library") setView("library");
    latest.current.onNavigate(origin);
  }, [view, discardDraft, progress, error, notice]);

  useEffect(() => {
    if ((!options.open || !options.active || view !== "processing") && !busy) return;
    if (!inspectionTargetId) return;
    let disposed = false;
    let stop: (() => void) | undefined;
    void listenEvent<AudioModPrepareProgress>("audio-mod-prepare-progress", event => {
      if (!disposed && operation.current && event.payload.account_id === inspectionTargetId) setProgress(event.payload);
    }).then(unlisten => { if (disposed) unlisten(); else stop = unlisten; }).catch(() => undefined);
    return () => { disposed = true; stop?.(); };
  }, [busy, inspectionTargetId, options.open, options.active, view]);

  const prepare = async () => {
    const current = draftRef.current;
    if (!current || operation.current) return;
    const installationPath = latest.current.installationPaths?.[current.edition];
    const targetStillValid = () => {
      if (current.installationOnly) return installationPath === latest.current.installationPaths?.[current.edition];
      const account = latest.current.accounts.find(account => account.id === current.accountId);
      return account?.initialized && editionForAccount(account) === current.edition;
    };
    const targetUnavailable = current.installationOnly ? (en ? "The game installation changed. Inspect the new directory before continuing." : "游戏安装目录已改变，请重新检查后继续加工。") : en ? "The target account is no longer available for this game edition. Select an account again."
      : "目标账号已不可用或游戏版本已改变，请重新选择账号。";
    if (!targetStillValid()) {
      setError(targetUnavailable);
      return;
    }
    const key = JSON.stringify(current);
    const currentAnalysis = describeModDraft(current, inspection.state, en);
    const recovery = receiptRef.current?.key === key ? receiptRef.current : null;
    if (!recovery && (inspection.loading || inspection.error || currentAnalysis.blockedReason || !processorReady)) return;
    operation.current = true;
    setBusy(true); setAutoStart(false); setError(null); setNotice(null);
    let completed = recovery;
    setProgress({ account_id: current.installationOnly ? installationTargetId(current.edition) : current.accountId, phase: completed ? "applying" : "starting", percent: 1,
      message: en ? "Preparing selected features…" : "正在准备所选功能…" });
    try {
      if (!completed) {
        const features = audioModFeatureInvokeOptions(currentAnalysis.selection);
        if (current.recipe.kind === "augment") {
          const upgraded = await invokeCommand<AudioModSetupState>("upgrade_audio_mod", {
            accountId: current.accountId, ...(current.installationOnly ? { edition: current.edition } : {}), modName: current.recipe.modName, forceRebuild: current.recipe.rebuild ?? false,
            sourceModName: current.recipe.sourceOverride ?? currentAnalysis.selected?.source_mod_name
              ?? (inspection.state?.current_mod_name?.toLocaleLowerCase() === current.recipe.modName.toLocaleLowerCase() ? inspection.state.source_mod_name : null),
            ...features,
          });
          const alreadyAssigned = current.installationOnly || inspection.state?.current_mod_name?.toLocaleLowerCase() === current.recipe.modName.toLocaleLowerCase();
          completed = { key, modName: current.recipe.modName, ...(alreadyAssigned ? { applied: upgraded } : {}) };
        } else {
          const built = await invokeCommand<AudioModPrepareResult>("prepare_audio_mod", {
            accountId: current.accountId, ...(current.installationOnly ? { edition: current.edition } : {}), modName: current.recipe.replaceSource ? current.recipe.source : current.recipe.name.trim(), sourceModName: current.recipe.source, replaceSource: current.recipe.replaceSource ?? false, ...features,
          });
          completed = { key, modName: built.mod_name };
        }
        receiptRef.current = completed;
        setReceipt(completed);
      }
      if (!completed.applied && current.installationOnly) {
        if (!targetStillValid()) throw new Error(targetUnavailable);
        const state = await invokeCommand<AudioModSetupState>("get_audio_mod_setup_state", { accountId: "", edition: current.edition, modName: completed.modName });
        completed = { ...completed, applied: state };
        receiptRef.current = completed; setReceipt(completed);
      }
      if (!completed.applied) {
        if (!targetStillValid()) throw new Error(targetUnavailable);
        const applied = await invokeCommand<AudioModSetupState>("apply_audio_mod_to_account", { accountId: current.accountId, modName: completed.modName });
        completed = { ...completed, applied };
        receiptRef.current = completed;
        setReceipt(completed);
      }
      inspection.accept(completed.applied!);
      if (!targetStillValid()) throw new Error(targetUnavailable);
      await latest.current.onApplied({ origin: current.origin, accountId: current.accountId, state: completed.applied!,
        ...(current.installationOnly ? { installationEdition: current.edition } : {}) });
      notifyModCatalogChanged();
      await latest.current.catalog.refresh();
      receiptRef.current = null; setReceipt(null); setProgress(null);
      draftRef.current = null; setDraft(null); accountDrafts.current.clear();
      setView("library");
      if (latest.current.active) latest.current.onNavigate(current.origin);
      showToast(completed.applied!.restart_required ? "warning" : "success", en
        ? "Mod features are ready. Restart the game to use the updated Mod."
        : "Mod 功能已准备完成，重启游戏后生效。");
    } catch (cause) {
      if (observedTask.current?.state === "cancelled" || /cancel(?:led|ed)|已取消|用户取消/i.test(String(cause))) {
        setNotice(en ? "Processing cancelled. Your draft is kept." : "加工已取消，草稿已保留。");
        setProgress(null);
      } else setError(String(cause));
      // Keep the successful build receipt so Retry applies it instead of rebuilding an existing folder.
    } finally { operation.current = false; setBusy(false); }
  };

  useEffect(() => {
    if (options.open && options.active && view === "processing" && autoStart && processorReady && !blockedReason && !busy) void prepare();
  }, [options.open, options.active, view, autoStart, processorReady, blockedReason, busy]);

  return {
    view, libraryEdition, openAdd, draft, inspection, analysis, blockedReason, busy: busy || batch.busy, progress, error, notice, prepared, processorReady,
    preparationTask, batch,
    en, minimalMode: !options.optionalFeaturesAvailable,
    accounts: options.accounts.filter(account => account.initialized),
    readyCapsules: options.catalog.pool?.capsules.filter(capsule => capsule.ready && capsule.processed && capsule.edition === draft?.edition) ?? [],
    catalog: options.catalog,
    actions: {
      openBatch: (edition: ModEdition) => { if (!operation.current) { batch.open(edition); setView("batch"); } },
      requestProcessing, openLibrary, discardDraft, changeRecipe, changeFeatures, chooseTarget, prepare, back, setProcessorReady,
      setLibraryEdition, refresh: async () => {
        const [state] = await Promise.all([inspection.refresh(), latest.current.catalog.refresh()]);
        return state;
      },
      resume: () => { if (draftRef.current) setView("processing"); },
    },
  };
}
export type ModWorkflowController = ReturnType<typeof useModWorkflow>;
