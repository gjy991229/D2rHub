import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  ChevronDown,
  KeyRound,
  RefreshCw,
  UsersRound,
} from "lucide-react";
import { useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { Button } from "../../../components/ui/Button";
import { Toggle } from "../../../components/ui/Toggle";
import { parseShortcutFromKeyEvent } from "../../../hooks/useShortcutRecorder";
import type { AccountMeta, ModCapsulePool } from "../../../store/types";
import {
  accountsMissingCapsuleFeature,
  capsuleSelectionForAccount,
  compatibleCapsulesForAccount,
  ROOM_TOOLS_CAPSULE_FEATURE,
  selectedCapsuleForAccount,
} from "../../modCapsules/model";
import { ROOM_AUTOMATION_COPY } from "../../roomAutomation/copy";
import { FOREGROUND_TIMING_FIELDS, foregroundTimingWithDefaults } from "../../roomAutomation/foregroundTiming";
import {
  roomAutomationGateway,
  type RoomAutomationGateway,
} from "../../roomAutomation/gateway";
import {
  canonicalizeRoomAutomationShortcut,
  generatedRoomName,
  roomAutomationConfigsEqual,
  validateRoomAutomationConfig,
} from "../../roomAutomation/model";
import type {
  RoomAutomationConfig,
  RoomAutomationConfigSnapshot,
  RoomAutomationWorkflowStatus,
  RoomChatBindingStatus,
} from "../../roomAutomation/types";
import { normalizeSettingsLanguage } from "../settingsRegistry";
import "../roomAutomationParticipants.css";

interface RoomAutomationPanelProps {
  accounts: AccountMeta[];
  language?: string | null;
  gateway?: RoomAutomationGateway;
  modCapsulePool?: ModCapsulePool | null;
  modCapsulePoolLoading?: boolean;
  modCapsulePoolError?: string | null;
  assigningAccountId?: string | null;
  onAssignModCapsule?: (accountId: string, capsuleId: string) => Promise<unknown>;
  /** @deprecated Room automation no longer follows the recognition module state. */
  recognitionEnabled?: boolean;
  /** @deprecated Room automation no longer derives its primary account from recognition. */
  recognitionAccountId?: string;
  onRequireRoomTools?: (accountId: string, capsuleId?: string, autoStart?: boolean) => void;
  onSaveLaunchScheme?: (accountIds: string[]) => Promise<void> | void;
}

type Operation = "save" | "scan" | "restore";

function cloneConfig(config: RoomAutomationConfig): RoomAutomationConfig {
  return {
    ...config,
    input_method: config.input_method ?? "background_keys",
    foreground_timing: foregroundTimingWithDefaults(config.foreground_timing),
    chat_key: config.chat_key ?? "pause",
    follower_join_mode: config.follower_join_mode ?? "simultaneous",
    follower_join_interval_secs: config.follower_join_interval_secs ?? 3,
    follower_account_ids: [...config.follower_account_ids],
    flow: { ...config.flow, key_hold_ms: config.flow.key_hold_ms ?? 50 },
  };
}

function isWorkflowActive(status: RoomAutomationWorkflowStatus | null): boolean {
  return status?.phase === "primary" || status?.phase === "waiting" || status?.phase === "followers";
}

function accountLabel(account: AccountMeta): string {
  return account.display_name?.trim() || account.id;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isStaleConfigError(error: unknown): boolean {
  return /stale|generation|版本|已被.*更新/i.test(errorMessage(error));
}

export function RoomAutomationPanel({
  accounts,
  language,
  gateway = roomAutomationGateway,
  modCapsulePool = null,
  modCapsulePoolLoading = false,
  modCapsulePoolError = null,
  assigningAccountId = null,
  onAssignModCapsule,
  onRequireRoomTools,
  onSaveLaunchScheme,
}: RoomAutomationPanelProps) {
  const locale = normalizeSettingsLanguage(language);
  const copy = ROOM_AUTOMATION_COPY[locale];
  const eligibleAccounts = useMemo(() => accounts.filter((account) => account.initialized), [accounts]);
  const [snapshot, setSnapshot] = useState<RoomAutomationConfigSnapshot | null>(null);
  const [draft, setDraft] = useState<RoomAutomationConfig | null>(null);
  const [status, setStatus] = useState<RoomAutomationWorkflowStatus | null>(null);
  const [binding, setBinding] = useState<RoomChatBindingStatus | null>(null);
  const [bindingLoading, setBindingLoading] = useState(true);
  const [bindingError, setBindingError] = useState<string | null>(null);
  const [bindingFeedback, setBindingFeedback] = useState<string | null>(null);
  const [bindingExpanded, setBindingExpanded] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(true);
  const [unavailable, setUnavailable] = useState<string | null>(null);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [stale, setStale] = useState(false);
  const [operation, setOperation] = useState<Operation | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [bindingReloadKey, setBindingReloadKey] = useState(0);
  const snapshotRef = useRef<RoomAutomationConfigSnapshot | null>(null);
  const draftRef = useRef<RoomAutomationConfig | null>(null);
  const dirtyRef = useRef(false);
  const operationRef = useRef<Operation | null>(null);
  const mountedRef = useRef(true);
  const persistDraftRef = useRef<((candidate: RoomAutomationConfig) => Promise<void>) | null>(null);
  const dirty = useMemo(
    () => !roomAutomationConfigsEqual(snapshot?.config ?? null, draft),
    [draft, snapshot],
  );

  useEffect(() => {
    dirtyRef.current = dirty;
    draftRef.current = draft;
  }, [dirty, draft]);

  useEffect(() => {
    operationRef.current = operation;
  }, [operation]);

  const commitConfig = useCallback((next: RoomAutomationConfigSnapshot) => {
    if (snapshotRef.current && next.generation < snapshotRef.current.generation) return false;
    snapshotRef.current = next;
    setSnapshot(next);
    const committed = cloneConfig(next.config);
    draftRef.current = committed;
    setDraft(committed);
    dirtyRef.current = false;
    setStale(false);
    return true;
  }, []);

  const acceptConfig = useCallback((next: RoomAutomationConfigSnapshot) => {
    if (operationRef.current === "save") return;
    if (dirtyRef.current) {
      if (!snapshotRef.current || next.generation > snapshotRef.current.generation) setStale(true);
      return;
    }
    commitConfig(next);
  }, [commitConfig]);

  const acceptStatus = useCallback((next: RoomAutomationWorkflowStatus) => {
    setStatus((current) => !current || next.revision >= current.revision ? next : current);
  }, []);

  useEffect(() => {
    let disposed = false;
    let stopSync: (() => void) | undefined;
    setLoading(true);
    setUnavailable(null);
    setOperationError(null);

    void (async () => {
      try {
        const stop = await gateway.startSync({
          onConfig: (next) => {
            if (!disposed) acceptConfig(next);
          },
          onStatus: (next) => {
            if (!disposed) acceptStatus(next);
          },
        });
        if (disposed) {
          stop();
          return;
        }
        stopSync = stop;
        setLoading(false);
      } catch (error) {
        stopSync?.();
        stopSync = undefined;
        if (!disposed) {
          setUnavailable(errorMessage(error));
          setLoading(false);
        }
      }
    })();

    return () => {
      disposed = true;
      stopSync?.();
      stopSync = undefined;
    };
  }, [acceptConfig, acceptStatus, gateway, reloadKey]);

  useEffect(() => {
    let disposed = false;
    setBindingLoading(true);
    setBindingError(null);
    void gateway.getChatBinding()
      .then((next) => {
        if (!disposed) setBinding(next);
      })
      .catch((error) => {
        if (!disposed) {
          setBinding(null);
          setBindingError(errorMessage(error));
        }
      })
      .finally(() => {
        if (!disposed) setBindingLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [bindingReloadKey, gateway, reloadKey]);

  const updateDraft = useCallback((updater: (current: RoomAutomationConfig) => RoomAutomationConfig) => {
    setDraft((current) => {
      if (!current) return current;
      const next = updater(cloneConfig(current));
      draftRef.current = next;
      dirtyRef.current = !roomAutomationConfigsEqual(snapshotRef.current?.config ?? null, next);
      return next;
    });
    setOperationError(null);
    setBindingFeedback(null);
  }, []);

  const reload = () => {
    dirtyRef.current = false;
    if (snapshotRef.current) setDraft(cloneConfig(snapshotRef.current.config));
    setStale(false);
    setOperationError(null);
    setBindingFeedback(null);
    setReloadKey((current) => current + 1);
  };

  const validation = useMemo(() => draft
    ? validateRoomAutomationConfig(draft, copy, eligibleAccounts.map((account) => account.id))
    : null, [copy, draft, eligibleAccounts]);
  const workflowActive = isWorkflowActive(status);
  const saving = operation === "save";
  const editorDisabled = loading || !!unavailable || (!!operation && operation !== "save");
  const followerJoinMode = draft?.follower_join_mode ?? "simultaneous";
  const participantAccountIds = useMemo(() => draft
    ? [draft.primary_account_id, ...draft.follower_account_ids].filter(Boolean)
    : [], [draft]);
  const participantsMissingRoomTools = useMemo(() => accountsMissingCapsuleFeature(
    modCapsulePool,
    participantAccountIds,
    ROOM_TOOLS_CAPSULE_FEATURE,
  ), [modCapsulePool, participantAccountIds]);

  const persistDraft = useCallback(async (candidate: RoomAutomationConfig) => {
    const currentSnapshot = snapshotRef.current;
    if (!currentSnapshot || operationRef.current || stale) return;
    const candidateValidation = validateRoomAutomationConfig(
      candidate,
      copy,
      eligibleAccounts.map((account) => account.id),
    );
    if (!candidateValidation.valid) return;
    operationRef.current = "save";
    setOperation("save");
    setOperationError(null);
    let saved = false;
    try {
      const outcome = await gateway.saveConfig(currentSnapshot.generation, candidate);
      saved = true;
      snapshotRef.current = outcome.snapshot;
      setSnapshot(outcome.snapshot);
      if (roomAutomationConfigsEqual(draftRef.current, candidate)) {
        const committed = cloneConfig(outcome.snapshot.config);
        draftRef.current = committed;
        setDraft(committed);
        dirtyRef.current = false;
      }
      setStale(false);
      if (outcome.snapshot.config.input_method !== "foreground_mouse"
        && outcome.snapshot.config.chat_f13_auto_patch_enabled
        && (candidate.enabled || candidate.chat_key !== currentSnapshot.config.chat_key)) {
        setBindingFeedback(outcome.apply_warning ? null : copy.configScanComplete);
        setBindingReloadKey((current) => current + 1);
      }
      if (outcome.apply_warning) {
        setOperationError(`${copy.savedButRuntimeFailed}: ${outcome.apply_warning}`);
      }
    } catch (error) {
      const staleError = isStaleConfigError(error);
      setStale(staleError);
      setOperationError(`${copy.saveFailed}: ${errorMessage(error)}`);
    } finally {
      operationRef.current = null;
      setOperation(null);
      // A final edit can arrive during a save, just before the panel closes.
      if (!mountedRef.current && saved && dirtyRef.current && draftRef.current) {
        void persistDraftRef.current?.(cloneConfig(draftRef.current));
      }
    }
  }, [copy, eligibleAccounts, gateway, stale]);

  useEffect(() => {
    persistDraftRef.current = persistDraft;
  }, [persistDraft]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      // Leaving the page flushes a valid pending edit instead of dropping
      // the debounce timer's last value. Validation stays in persistDraft.
      if (dirtyRef.current && draftRef.current) {
        void persistDraftRef.current?.(cloneConfig(draftRef.current));
      }
    };
  }, []);

  useEffect(() => {
    if (!draft || !dirty || !validation?.valid || stale || operationError || operationRef.current) return;
    const timer = window.setTimeout(() => void persistDraft(cloneConfig(draft)), 450);
    return () => window.clearTimeout(timer);
  }, [dirty, draft, operationError, persistDraft, saving, stale, validation?.valid]);

  const updateBinding = async (
    kind: "scan" | "restore",
    action: () => Promise<RoomChatBindingStatus>,
  ): Promise<RoomChatBindingStatus | null> => {
    if (editorDisabled || dirty || !binding || bindingLoading
      || bindingError || operationRef.current) return null;
    operationRef.current = kind;
    setOperation(kind);
    setOperationError(null);
    setBindingError(null);
    setBindingFeedback(null);
    try {
      const next = await action();
      setBinding(next);
      if (kind === "scan") {
        setBindingFeedback(copy.manualScanComplete(next.installedFiles, next.totalFiles, next.d2rRunning));
      }
      const committed = await gateway.getConfig();
      commitConfig(committed);
      return next;
    } catch (error) {
      setOperationError(`${copy.bindingFailed}: ${errorMessage(error)}`);
      const [latestBinding, latestConfig] = await Promise.allSettled([
        gateway.getChatBinding(),
        gateway.getConfig(),
      ]);
      if (latestBinding.status === "fulfilled") {
        setBinding(latestBinding.value);
        setBindingError(null);
      } else {
        setBindingError(errorMessage(latestBinding.reason));
      }
      if (latestConfig.status === "fulfilled") {
        commitConfig(latestConfig.value);
      }
      return null;
    } finally {
      operationRef.current = null;
      setOperation(null);
    }
  };

  const scanAndInstallBinding = async () => {
    const previous = binding;
    const next = await updateBinding("scan", gateway.installChatBinding);
    const discoveredFiles = !!previous && !!next && (
      next.totalFiles > previous.totalFiles
      || next.installedFiles > previous.installedFiles
    );
    setBindingExpanded(
      !next
      || !next.ready
      || !previous?.ready
      || discoveredFiles,
    );
  };

  const enableRoomAutomation = () => {
    if (!draft) return;
    const candidate = {
      ...draft,
      enabled: true,
      chat_f13_auto_patch_enabled: draft.input_method === "foreground_mouse"
        ? draft.chat_f13_auto_patch_enabled : true,
    };
    const candidateValidation = validateRoomAutomationConfig(
      candidate,
      copy,
      eligibleAccounts.map((account) => account.id),
    );
    if (!candidateValidation.valid) {
      setOperationError(Object.values(candidateValidation.fieldErrors)[0] ?? copy.completeRequired);
      return;
    }
    if (modCapsulePoolLoading) {
      setOperationError(copy.capsulePoolLoading);
      return;
    }
    if (modCapsulePoolError) {
      setOperationError(`${copy.capsulePoolUnavailable}: ${modCapsulePoolError}`);
      return;
    }
    const candidateParticipants = [candidate.primary_account_id, ...candidate.follower_account_ids].filter(Boolean);
    const missingAccountId = accountsMissingCapsuleFeature(
      modCapsulePool,
      candidateParticipants,
      ROOM_TOOLS_CAPSULE_FEATURE,
    )[0];
    if (missingAccountId) {
      const account = eligibleAccounts.find((candidate) => candidate.id === missingAccountId);
      const selected = selectedCapsuleForAccount(modCapsulePool, missingAccountId);
      setOperationError(copy.capsuleRequired(account ? accountLabel(account) : missingAccountId));
      onRequireRoomTools?.(missingAccountId, selected?.id, !!selected?.processed);
      return;
    }
    updateDraft(() => candidate);
  };

  if (loading) {
    return (
      <div className="room-automation-state" role="status" aria-live="polite">
        <span className="room-automation-state-dot" data-tone="neutral" aria-hidden="true" />
        <span>{copy.loading}</span>
      </div>
    );
  }

  if (unavailable || !draft || !snapshot) {
    return (
      <div className="room-automation-state room-automation-state-block" data-tone="danger" role="alert">
        <AlertCircle size={18} aria-hidden="true" />
        <div>
          <strong>{copy.unavailable}</strong>
          <p>{unavailable ? `${copy.loadFailed}: ${unavailable}` : copy.loadFailed}</p>
        </div>
        <Button size="sm" onClick={reload}>{copy.retryLoad}</Button>
      </div>
    );
  }

  const foregroundMouse = draft.input_method === "foreground_mouse";
  const foregroundTiming = foregroundTimingWithDefaults(draft.foreground_timing);
  const bindingNeedsAttention = !foregroundMouse && draft.enabled && (!binding?.ready || !!bindingError || bindingLoading);
  const bindingCardOpen = bindingExpanded
    ?? (draft.enabled && !bindingLoading && (!binding?.ready || !!bindingError));
  const statusTone = status?.phase === "error"
    ? "danger"
    : !draft.enabled
      ? "neutral"
      : workflowActive || bindingNeedsAttention
        ? "warning"
        : "success";
  const statusTitle = status?.phase === "error"
    ? copy.error
    : !draft.enabled
      ? copy.disabled
      : bindingNeedsAttention
        ? copy.bindingRequired
        : copy.ready;
  return (
    <div className="room-automation-panel">
      <header className="spatial-panel room-automation-header">
        <div className="room-automation-header-main">
          <div className="min-w-0">
            <h2>{copy.title}</h2>
            <p>{copy.subtitle}</p>
          </div>
          <div className="room-automation-header-actions">
            {draft.enabled && (
              <Button
                size="sm"
                variant="secondary"
                disabled={editorDisabled || dirty || !validation?.valid || participantsMissingRoomTools.length > 0
                  || (!foregroundMouse && (!binding?.ready || !!bindingError)) || !onSaveLaunchScheme}
                onClick={() => void onSaveLaunchScheme?.(participantAccountIds)}
              >{copy.saveLaunchScheme}</Button>
            )}
            <Toggle
              checked={draft.enabled}
              label={copy.enableLabel}
              disabled={editorDisabled}
              ariaLabel={copy.enabled}
              descriptionId={!draft.enabled ? "room-automation-module-description" : undefined}
              onChange={(enabled) => {
                if (enabled) enableRoomAutomation();
                else updateDraft((current) => ({ ...current, enabled: false }));
              }}
            />
          </div>
        </div>
        <div className="room-automation-readiness" data-tone={statusTone} role="status" aria-live="polite">
          <span className="room-automation-semantic-status" data-tone={statusTone}>
            <span className="room-automation-state-dot" data-tone={statusTone} aria-hidden="true" />
            {statusTitle}
          </span>
          <span className="room-automation-save-state" data-dirty={dirty ? "true" : undefined}>
            {saving ? copy.applying : dirty ? copy.unsaved : copy.applied}
          </span>
        </div>
        {!draft.enabled && (
          <p id="room-automation-module-description" className="room-automation-disabled-note">
            {copy.disabledDescription}
          </p>
        )}
      </header>

      {(stale || operationError) && (
        <div className="room-automation-state room-automation-state-block" data-tone="danger" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <div>
            <strong>{stale ? copy.staleHint : copy.error}</strong>
            {operationError && <p>{operationError}</p>}
          </div>
          {stale ? <Button size="sm" onClick={reload}>{copy.retryLoad}</Button>
            : dirty && operationError && <Button size="sm" disabled={!validation?.valid || !!operation}
              onClick={() => void persistDraft(cloneConfig(draft))}>{copy.retrySave}</Button>}
        </div>
      )}

      <div className="room-automation-setup">
      <section className="spatial-panel room-automation-pane room-automation-section" aria-labelledby="room-participants-title">
        <div className="room-automation-section-heading">
          <UsersRound size={16} aria-hidden="true" />
          <div>
            <h3 id="room-participants-title">{copy.participants}</h3>
            <p>{copy.participantsHelp}</p>
          </div>
        </div>
        {eligibleAccounts.length === 0 ? (
          <p className="room-automation-empty">{copy.noAccounts}</p>
        ) : (
          <>
            <div className="room-automation-field">
              <ChoiceField label={copy.primary} value={draft.primary_account_id}
                options={eligibleAccounts.map((account) => ({ value: account.id, label: accountLabel(account) }))}
                placeholder={copy.selectPrimary}
                disabled={editorDisabled}
                invalid={!!validation?.fieldErrors.primary}
                onChange={(primary_account_id) => updateDraft((current) => {
                  const follower_account_ids = current.follower_account_ids.filter((id) => id !== primary_account_id);
                  return {
                    ...current,
                    primary_account_id,
                    follower_account_ids,
                  };
                })}
              />
              {validation?.fieldErrors.primary && <small role="alert">{validation.fieldErrors.primary}</small>}
            </div>

            <fieldset className="room-automation-followers" disabled={editorDisabled}>
              <legend>{copy.followers}</legend>
              <div className="room-automation-followers-toolbar">
                <span>{copy.followersSelected(draft.follower_account_ids.length)}</span>
                <div>
                  <Button size="sm" variant="ghost"
                    disabled={eligibleAccounts.every((account) => account.id === draft.primary_account_id
                      || draft.follower_account_ids.includes(account.id))}
                    onClick={() => updateDraft((current) => ({
                      ...current,
                      follower_account_ids: [...current.follower_account_ids,
                        ...eligibleAccounts.filter((account) => account.id !== current.primary_account_id
                          && !current.follower_account_ids.includes(account.id)).map((account) => account.id)],
                    }))}>{copy.selectAllFollowers}</Button>
                  <Button size="sm" variant="ghost" disabled={!draft.follower_account_ids.length}
                    onClick={() => updateDraft((current) => ({ ...current, follower_account_ids: [] }))}
                  >{copy.clearFollowers}</Button>
                </div>
              </div>
              <div className="room-automation-account-list">
                {eligibleAccounts.filter((account) => account.id !== draft.primary_account_id).map((account) => {
                  const checked = draft.follower_account_ids.includes(account.id);
                  return (
                    <label className="room-automation-account-row" data-selected={checked ? "true" : "false"} key={account.id}>
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={(event) => updateDraft((current) => {
                          const follower_account_ids = event.target.checked
                            ? [...current.follower_account_ids, account.id]
                            : current.follower_account_ids.filter((id) => id !== account.id);
                          return {
                            ...current,
                            follower_account_ids,
                          };
                        })}
                      />
                      <span className="room-automation-account-name">{accountLabel(account)}</span>
                    </label>
                  );
                })}
              </div>
              {(foregroundMouse || followerJoinMode === "interval") && draft.follower_account_ids.length > 0 && (
                <div className="room-automation-follower-order">
                  <div className="room-automation-follower-order-heading">
                    <strong>{copy.followerOrder}</strong>
                    <span>{foregroundMouse ? copy.foregroundQueueHelp : copy.followerJoinOrderHelp}</span>
                  </div>
                  <ol>
                    {draft.follower_account_ids.map((accountId, index) => {
                      const account = eligibleAccounts.find((candidate) => candidate.id === accountId);
                      const label = account ? accountLabel(account) : accountId;
                      return (
                        <li key={accountId}>
                          <span className="room-automation-follower-position" aria-hidden="true">{index + 1}</span>
                          <span className="room-automation-follower-order-name">{label}</span>
                          <span className="room-automation-follower-order-actions">
                            <button
                              type="button"
                              disabled={index === 0}
                              aria-label={copy.moveFollowerUp(label)}
                              title={copy.moveFollowerUp(label)}
                              onClick={() => updateDraft((current) => {
                                const follower_account_ids = [...current.follower_account_ids];
                                [follower_account_ids[index - 1], follower_account_ids[index]] = [
                                  follower_account_ids[index],
                                  follower_account_ids[index - 1],
                                ];
                                return { ...current, follower_account_ids };
                              })}
                            >
                              <ArrowUp size={13} aria-hidden="true" />
                            </button>
                            <button
                              type="button"
                              disabled={index === draft.follower_account_ids.length - 1}
                              aria-label={copy.moveFollowerDown(label)}
                              title={copy.moveFollowerDown(label)}
                              onClick={() => updateDraft((current) => {
                                const follower_account_ids = [...current.follower_account_ids];
                                [follower_account_ids[index], follower_account_ids[index + 1]] = [
                                  follower_account_ids[index + 1],
                                  follower_account_ids[index],
                                ];
                                return { ...current, follower_account_ids };
                              })}
                            >
                              <ArrowDown size={13} aria-hidden="true" />
                            </button>
                          </span>
                        </li>
                      );
                    })}
                  </ol>
                </div>
              )}
              {validation?.fieldErrors.followers && <small role="alert">{validation.fieldErrors.followers}</small>}
            </fieldset>

            {participantAccountIds.length > 0 && (
              <div className="room-automation-capsules" aria-label={copy.participantCapsules}>
                <div className="room-automation-capsules-heading">
                  <strong>{copy.participantCapsules}</strong>
                  <span>{copy.participantCapsulesHelp}</span>
                </div>
                {participantAccountIds.map((accountId) => {
                  const account = eligibleAccounts.find((candidate) => candidate.id === accountId);
                  if (!account) return null;
                  const selection = capsuleSelectionForAccount(modCapsulePool, accountId);
                  const selected = selectedCapsuleForAccount(modCapsulePool, accountId);
                  const compatible = compatibleCapsulesForAccount(modCapsulePool, accountId)
                    .filter((capsule) => capsule.feature_groups.includes(ROOM_TOOLS_CAPSULE_FEATURE));
                  const processable = compatibleCapsulesForAccount(modCapsulePool, accountId)
                    .filter((capsule) => capsule.source_eligible);
                  const ready = !!selected?.feature_groups.includes(ROOM_TOOLS_CAPSULE_FEATURE) && selected.ready;
                  return (
                    <div className="room-automation-capsule-row" data-ready={ready ? "true" : "false"} key={accountId}>
                      <span className="room-automation-capsule-account">{accountLabel(account)}</span>
                      {compatible.length > 0 || processable.length > 0 ? <ChoiceField
                          label={`${accountLabel(account)} · ${copy.selectCapsule}`}
                          hideLabel
                          options={(compatible.length ? compatible : processable).map((capsule) => ({
                            value: capsule.id, label: capsule.name,
                          }))}
                          placeholder={copy.selectCapsule}
                          value={ready ? selection?.selected_capsule_id ?? "" : ""}
                          disabled={editorDisabled || assigningAccountId === accountId
                            || (compatible.length ? !onAssignModCapsule : !onRequireRoomTools)}
                          onChange={(value) => {
                            const capsule = (compatible.length ? compatible : processable)
                              .find((candidate) => candidate.id === value);
                            if (!capsule) return;
                            if (compatible.length) void onAssignModCapsule?.(accountId, capsule.id);
                            else onRequireRoomTools?.(accountId, capsule.id, !!capsule.processed);
                          }}
                        /> : (
                          <Button
                            size="sm"
                            variant="secondary"
                            disabled={modCapsulePoolLoading || !onRequireRoomTools}
                            onClick={() => onRequireRoomTools?.(accountId)}
                          >{copy.prepareCapsule}</Button>
                        )}
                      <span className="room-automation-capsule-state" data-ready={ready ? "true" : "false"}>
                        {ready ? copy.capsuleReady : selection?.issue || copy.capsuleMissing}
                      </span>
                    </div>
                  );
                })}
              </div>
            )}

          </>
        )}
      </section>

      <section className="spatial-panel room-automation-pane room-automation-section" aria-labelledby="room-mode-title">
        <div className="room-automation-section-heading">
          <KeyRound size={16} aria-hidden="true" />
          <div><h3 id="room-mode-title">{copy.modeAndRoom}</h3></div>
        </div>
        <ChoiceField label={copy.inputMethod} value={draft.input_method ?? "background_keys"}
          options={[
            { value: "background_keys", label: copy.backgroundMethodShort },
            { value: "foreground_mouse", label: copy.foregroundMethodShort },
          ]}
          disabled={editorDisabled}
          help={foregroundMouse ? copy.foregroundMethodSummary : copy.backgroundMethodHelp}
          onChange={(value) => updateDraft((current) => ({
            ...current, input_method: value as RoomAutomationConfig["input_method"],
          }))} />
        <div className="room-automation-behavior-fields">
          <ChoiceField label={copy.followTrigger} value={draft.auto_followers_enabled ? "auto" : "manual"}
            options={[{ value: "manual", label: copy.manualMode }, { value: "auto", label: copy.automaticMode }]}
            disabled={editorDisabled}
            help={draft.auto_followers_enabled ? copy.automaticModeHelp : copy.manualModeHelp}
            onChange={(value) => updateDraft((current) => ({ ...current, auto_followers_enabled: value === "auto" }))} />
          <ChoiceField label={copy.followerJoinMode} value={followerJoinMode}
            options={[
              { value: "simultaneous", label: foregroundMouse ? copy.sequentialJoin : copy.simultaneousJoin },
              { value: "interval", label: copy.intervalJoin },
            ]}
            disabled={editorDisabled}
            help={followerJoinMode === "interval"
              ? (foregroundMouse ? copy.foregroundIntervalHelp : copy.intervalJoinHelp)
              : (foregroundMouse ? copy.foregroundQueueHelp : copy.simultaneousJoinHelp)}
            onChange={(value) => updateDraft((current) => ({
              ...current, follower_join_mode: value as RoomAutomationConfig["follower_join_mode"],
            }))} />
        </div>

        <div className="room-automation-shortcuts">
          {draft.auto_followers_enabled && (
            <NumberField
              label={copy.followerDelay}
              value={draft.auto_followers_delay_secs}
              min={2}
              max={60}
              disabled={editorDisabled}
              onChange={(value) => updateDraft((current) => ({ ...current, auto_followers_delay_secs: value }))}
            />
          )}
          {followerJoinMode === "interval" && (
            <NumberField
              label={copy.followerJoinInterval}
              value={draft.follower_join_interval_secs ?? 3}
              min={1}
              max={60}
              disabled={editorDisabled}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(value) => updateDraft((current) => ({ ...current, follower_join_interval_secs: value }))}
            />
          )}
          <ShortcutField
            label={copy.createShortcut}
            value={draft.shortcut}
            captureHint={copy.shortcutCapture}
            recordingLabel={copy.shortcutRecording}
            disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.shortcuts}
            onChange={(shortcut) => updateDraft((current) => ({ ...current, shortcut }))}
          />
          <ShortcutField
            label={copy.followerShortcut}
            value={draft.join_shortcut}
            captureHint={copy.shortcutCapture}
            recordingLabel={copy.shortcutRecording}
            disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.shortcuts}
            onChange={(join_shortcut) => updateDraft((current) => ({ ...current, join_shortcut }))}
          />
        </div>

        <div className="room-automation-room-builder">
          <div className="room-automation-room-builder-heading">
            <div><strong>{copy.roomNaming}</strong><span>{copy.roomNamingHelp}</span></div>
            <code aria-label={copy.preview} title={copy.preview}>{generatedRoomName(draft)}</code>
          </div>
          <div className="room-automation-room-name-fields">
            <TextField label={copy.prefix} value={draft.name_prefix} maxLength={15} disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.prefix}
            onChange={(name_prefix) => updateDraft((current) => ({ ...current, name_prefix }))} />
            <NumberField label={copy.sequence} value={draft.next_sequence} min={0} max={4_294_967_295} disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.sequence}
            onChange={(next_sequence) => updateDraft((current) => ({ ...current, next_sequence }))} />
            <ChoiceField label={copy.width} value={String(draft.sequence_width)}
              options={[1, 2, 3, 4, 5, 6].map((width) => ({ value: String(width), label: String(width) }))}
              disabled={editorDisabled} invalid={!!validation?.fieldErrors.sequence}
              onChange={(value) => updateDraft((current) => ({ ...current, sequence_width: Number(value) }))} />
          </div>
          <div className="room-automation-password-row">
            <TextField label={copy.password} value={draft.password} maxLength={15} disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.password}
            onChange={(password) => updateDraft((current) => ({ ...current, password }))} />
            <span>{copy.passwordOptional}</span>
          </div>
        </div>
        {(validation?.fieldErrors.shortcuts || validation?.fieldErrors.prefix || validation?.fieldErrors.password || validation?.fieldErrors.sequence) && (
          <p className="room-automation-field-error" role="alert">
            {validation.fieldErrors.shortcuts || validation.fieldErrors.prefix || validation.fieldErrors.password || validation.fieldErrors.sequence}
          </p>
        )}

      </section>
      </div>

      {!foregroundMouse && <section className="spatial-panel room-automation-advanced room-automation-binding-details">
        <div className="room-automation-binding-heading">
          <button
            type="button"
            className="room-automation-binding-toggle"
            aria-expanded={bindingCardOpen}
            aria-controls={bindingCardOpen ? "room-automation-binding-body" : undefined}
            data-open={bindingCardOpen ? "true" : undefined}
            onClick={() => setBindingExpanded(!bindingCardOpen)}
          >
            <span>
              <strong id="room-binding-title">{copy.f13Title}</strong>
              <small>{draft.chat_key === "f13" ? "F13" : "Pause"} · {bindingLoading ? copy.bindingLoading
                : bindingError ? copy.bindingUnavailable : binding?.ready ? copy.bindingReadySummary : copy.bindingNotReady}</small>
            </span>
            <ChevronDown size={15} aria-hidden="true" />
          </button>
          <Button
            size="sm"
            variant="secondary"
            className="room-automation-binding-scan"
            loading={operation === "scan"}
            disabled={editorDisabled || dirty || !binding || bindingLoading || !!bindingError}
            onClick={() => void scanAndInstallBinding()}
          >
            <RefreshCw size={13} aria-hidden="true" />
            {copy.scanAndInstallBinding}
          </Button>
        </div>

        {bindingFeedback && (
          <p className="room-automation-scan-feedback" role="status" aria-live="polite">
            {bindingFeedback}
          </p>
        )}

        {bindingCardOpen && (
          <div id="room-automation-binding-body" className="room-automation-binding-body">
            <ChoiceField label={copy.chatKey} value={draft.chat_key ?? "pause"}
              options={[{ value: "pause", label: copy.pauseKey }, { value: "f13", label: "F13" }]}
              disabled={editorDisabled || saving || bindingLoading || !!binding?.d2rRunning}
              descriptionId="room-chat-key-help"
              onChange={(value) => updateDraft((current) => ({
                ...current, chat_key: value as RoomAutomationConfig["chat_key"],
              }))} />
            <p id="room-chat-key-help" className="room-automation-consent-copy">{copy.f13Description}</p>
            <div className="flex justify-end">
              <Button
                size="sm"
                variant="ghost"
                loading={bindingLoading}
                disabled={bindingLoading || !!operation}
                onClick={() => {
                  setBindingFeedback(null);
                  setBindingReloadKey((current) => current + 1);
                }}
              >
                <RefreshCw size={13} aria-hidden="true" />
                {copy.refreshBinding}
              </Button>
            </div>
            {snapshot.consent_notice?.requires_user_reauthorization && (
              <p className="room-automation-consent-notice" role="note">{copy.f13LegacyNotice}</p>
            )}
            {bindingLoading ? (
              <div className="room-automation-state" role="status" aria-live="polite">
                <span className="room-automation-state-dot" data-tone="neutral" aria-hidden="true" />
                <span>{copy.bindingLoading}</span>
              </div>
            ) : bindingError ? (
              <div className="room-automation-state room-automation-state-block" data-tone="danger" role="alert">
                <AlertCircle size={17} aria-hidden="true" />
                <div>
                  <strong>{copy.bindingUnavailable}</strong>
                  <p>{bindingError}</p>
                </div>
                <Button size="sm" onClick={() => setBindingReloadKey((current) => current + 1)}>
                  {copy.retryBinding}
                </Button>
              </div>
            ) : (
              <div className="room-automation-binding-status" data-ready={binding?.ready ? "true" : "false"}>
                <div>
                  {binding && <span>{copy.bindingFiles(binding.installedFiles, binding.totalFiles)}</span>}
                  {!!binding?.conflictedFiles && <span>{copy.bindingConflicts(binding.conflictedFiles)}</span>}
                  {!!binding?.orphanBackupFiles && <span>{copy.bindingOrphans(binding.orphanBackupFiles)}</span>}
                </div>
                {draft.chat_f13_auto_patch_enabled && (
                  <span className="room-automation-scan-mode">{copy.configScanActive}</span>
                )}
              </div>
            )}
            <p className="room-automation-consent-copy">{copy.f13Consent}</p>
            <p className="room-automation-scan-hint" role="note">{copy.newCharacterScanHint}</p>
            {binding?.d2rRunning && <p className="room-automation-scan-hint" role="note">{copy.gameRunningHint}</p>}
            {binding?.lastWatcherError && <p className="room-automation-field-error" role="alert">{binding.lastWatcherError}</p>}
            {!draft.enabled && (
              <div className="room-automation-actions">
                <Button
                  size="md"
                  loading={operation === "restore"}
                  disabled={editorDisabled || dirty || !binding || bindingLoading
                    || !!bindingError || binding.d2rRunning
                    || (!binding.backupFiles && !binding.consentGranted && !binding.watcherRunning
                      && !draft.chat_f13_auto_patch_enabled)}
                  onClick={() => void updateBinding("restore", gateway.restoreChatBinding)}
                >{copy.restoreBinding}</Button>
              </div>
            )}
          </div>
        )}
      </section>}

      <details className="spatial-panel room-automation-advanced">
        <summary>
          <span><strong>{copy.advanced}</strong><small>{copy.advancedHelp}</small></span>
          <ChevronDown size={15} aria-hidden="true" />
        </summary>
        <fieldset disabled={editorDisabled}>
          <div className="room-automation-fields">
            {foregroundMouse ? FOREGROUND_TIMING_FIELDS.map(([key, label]) => (
              <NumberField key={key} label={copy[label]} value={foregroundTiming[key]}
                min={key === "step_interval_ms" ? 0 : 1} max={2000}
                invalid={!!validation?.fieldErrors.timing}
                onChange={(value) => updateDraft((current) => ({
                  ...current,
                  foreground_timing: { ...foregroundTimingWithDefaults(current.foreground_timing), [key]: value },
                }))}
              />
            )) : <>
            <NumberField label={copy.stepDelay} value={draft.flow.step_delay_ms} min={0} max={2000}
              disabled={foregroundMouse}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(step_delay_ms) => updateDraft((current) => ({ ...current, flow: { ...current.flow, step_delay_ms } }))} />
            <NumberField label={copy.keyHold} value={draft.flow.key_hold_ms ?? 50} min={10} max={250}
              disabled={foregroundMouse}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(key_hold_ms) => updateDraft((current) => ({ ...current, flow: { ...current.flow, key_hold_ms } }))} />
            <NumberField label={copy.characterDelay} value={draft.flow.character_delay_ms} min={10} max={250}
              disabled={foregroundMouse}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(character_delay_ms) => updateDraft((current) => ({ ...current, flow: { ...current.flow, character_delay_ms } }))} />
            <ChoiceField label={copy.backgroundStrategy} value={draft.background_text_strategy}
              options={[{ value: "post_keys", label: copy.postKeys }, { value: "send_keys", label: copy.sendKeys }]}
              disabled={editorDisabled}
              onChange={(value) => updateDraft((current) => ({
                ...current, background_text_strategy: value as RoomAutomationConfig["background_text_strategy"],
              }))} />
            </>}
          </div>
          <p className="room-automation-consent-copy">{foregroundMouse ? copy.foregroundTimingHelp : copy.inputTimingHelp}</p>
          {validation?.fieldErrors.timing && <p className="room-automation-field-error" role="alert">{validation.fieldErrors.timing}</p>}
        </fieldset>
      </details>
    </div>
  );
}

interface ChoiceFieldProps {
  label: string;
  value: string;
  options: ReadonlyArray<{ value: string; label: string }>;
  onChange: (value: string) => void;
  disabled?: boolean;
  invalid?: boolean;
  placeholder?: string;
  help?: string;
  descriptionId?: string;
  hideLabel?: boolean;
}

function ChoiceField({
  label, value, options, onChange, disabled, invalid, placeholder, help, descriptionId, hideLabel,
}: ChoiceFieldProps) {
  const id = useId();
  const describedBy = [descriptionId, help ? `${id}-help` : undefined].filter(Boolean).join(" ") || undefined;
  return (
    <fieldset className="room-automation-choice" disabled={disabled}>
      <legend className={hideLabel ? "room-automation-visually-hidden" : undefined}>{label}</legend>
      {options.length > 3 ? (
        <select className="settings-input" value={value} aria-label={label}
          aria-invalid={invalid || undefined} aria-describedby={describedBy}
          onChange={(event) => onChange(event.target.value)}>
          {placeholder && <option value="">{placeholder}</option>}
          {options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
        </select>
      ) : (
        <div className="room-automation-choice-buttons" data-invalid={invalid || undefined}>
          {options.map((option) => (
            <label key={option.value} data-selected={option.value === value ? "true" : undefined}>
              <input type="radio" name={id} value={option.value} checked={option.value === value}
                aria-invalid={invalid || undefined} aria-describedby={describedBy}
                onChange={() => onChange(option.value)} />
              <span title={option.label}>{option.label}</span>
            </label>
          ))}
        </div>
      )}
      {help && <p id={`${id}-help`} className="room-automation-choice-help">{help}</p>}
    </fieldset>
  );
}

interface TextFieldProps {
  label: string;
  value: string;
  disabled?: boolean;
  invalid?: boolean;
  maxLength?: number;
  onChange: (value: string) => void;
}

function TextField({ label, value, disabled, invalid, maxLength, onChange }: TextFieldProps) {
  return (
    <label className="room-automation-field">
      <span>{label}</span>
      <input
        className="settings-input"
        value={value}
        disabled={disabled}
        maxLength={maxLength}
        aria-invalid={invalid || undefined}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );
}

interface ShortcutFieldProps {
  label: string;
  value: string;
  captureHint: string;
  recordingLabel: string;
  disabled?: boolean;
  invalid?: boolean;
  onChange: (value: string) => void;
}

function ShortcutField({
  label,
  value,
  captureHint,
  recordingLabel,
  disabled,
  invalid,
  onChange,
}: ShortcutFieldProps) {
  const [recording, setRecording] = useState(false);

  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording) return;
    if (event.key === "Tab") {
      setRecording(false);
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      setRecording(false);
      return;
    }
    if (event.metaKey) return;

    const shortcut = parseShortcutFromKeyEvent(event);
    const canonical = shortcut && canonicalizeRoomAutomationShortcut(shortcut);
    if (!canonical) return;
    onChange(canonical);
    setRecording(false);
  };

  return (
    <label className="room-automation-field">
      <span>{label}</span>
      <button
        type="button"
        className="settings-input room-automation-shortcut-input"
        data-recording={recording ? "true" : "false"}
        disabled={disabled}
        aria-label={label}
        aria-invalid={invalid || undefined}
        onClick={() => setRecording(true)}
        onBlur={() => setRecording(false)}
        onKeyDown={handleKeyDown}
      >
        {recording ? recordingLabel : value}
      </button>
      <small className="room-automation-field-hint">{captureHint}</small>
    </label>
  );
}

interface NumberFieldProps {
  label: string;
  value: number;
  min?: number;
  max?: number;
  disabled?: boolean;
  invalid?: boolean;
  onChange: (value: number) => void;
}

function NumberField({ label, value, min, max, disabled, invalid, onChange }: NumberFieldProps) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  return (
    <label className="room-automation-field room-automation-number-field">
      <span>{label}</span>
      <input
        type="number"
        className="settings-input"
        value={text}
        min={min}
        max={max}
        disabled={disabled}
        aria-invalid={invalid || undefined}
        onChange={(event) => {
          setText(event.target.value);
          if (event.target.value !== "" && Number.isFinite(event.target.valueAsNumber)) {
            onChange(event.target.valueAsNumber);
          }
        }}
        onBlur={() => setText(String(value))}
      />
    </label>
  );
}
