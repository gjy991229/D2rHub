import {
  AlertCircle,
  ChevronDown,
  KeyRound,
  RefreshCw,
  UsersRound,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
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
  recognitionEnabled?: boolean;
  recognitionAccountId?: string;
  onRequireRoomTools?: (accountId: string, capsuleId?: string, autoStart?: boolean) => void;
  onSaveLaunchScheme?: (accountIds: string[]) => Promise<void> | void;
}

type Operation = "save" | "install" | "restore";

function cloneConfig(config: RoomAutomationConfig): RoomAutomationConfig {
  return {
    ...config,
    follower_account_ids: [...config.follower_account_ids],
    flow: { ...config.flow },
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
  recognitionEnabled = false,
  recognitionAccountId = "",
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
  const autoInstallAttemptRef = useRef<string | null>(null);
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
  }, []);

  const reload = () => {
    dirtyRef.current = false;
    if (snapshotRef.current) setDraft(cloneConfig(snapshotRef.current.config));
    setStale(false);
    setOperationError(null);
    setReloadKey((current) => current + 1);
  };

  useEffect(() => {
    if (!draft?.enabled || !recognitionEnabled || !recognitionAccountId
      || draft.primary_account_id === recognitionAccountId
      || !eligibleAccounts.some((account) => account.id === recognitionAccountId)) return;
    updateDraft((current) => ({
      ...current,
      primary_account_id: recognitionAccountId,
      follower_account_ids: [current.primary_account_id, ...current.follower_account_ids]
        .filter((id, index, values) => id && id !== recognitionAccountId && values.indexOf(id) === index),
    }));
  }, [draft?.enabled, draft?.primary_account_id, eligibleAccounts, recognitionAccountId, recognitionEnabled, updateDraft]);

  const validation = useMemo(() => draft
    ? validateRoomAutomationConfig(draft, copy, eligibleAccounts.map((account) => account.id))
    : null, [copy, draft, eligibleAccounts]);
  const workflowActive = isWorkflowActive(status);
  const saving = operation === "save";
  const editorDisabled = loading || !!unavailable || (!!operation && operation !== "save");
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
    try {
      const outcome = await gateway.saveConfig(currentSnapshot.generation, candidate);
      snapshotRef.current = outcome.snapshot;
      setSnapshot(outcome.snapshot);
      if (roomAutomationConfigsEqual(draftRef.current, candidate)) {
        const committed = cloneConfig(outcome.snapshot.config);
        draftRef.current = committed;
        setDraft(committed);
        dirtyRef.current = false;
      }
      setStale(false);
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
    }
  }, [copy, eligibleAccounts, gateway, stale]);

  useEffect(() => {
    if (!draft || !dirty || !validation?.valid || stale || operationRef.current) return;
    void persistDraft(cloneConfig(draft));
  }, [dirty, draft, persistDraft, saving, stale, validation?.valid]);

  const updateBinding = async (
    kind: "install" | "restore",
    action: () => Promise<RoomChatBindingStatus>,
  ) => {
    if (editorDisabled || dirty || (kind === "install" && !draft?.enabled) || !binding || bindingLoading
      || bindingError || binding.d2rRunning || operationRef.current) return;
    operationRef.current = kind;
    setOperation(kind);
    setOperationError(null);
    setBindingError(null);
    try {
      setBinding(await action());
      const committed = await gateway.getConfig();
      commitConfig(committed);
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
    } finally {
      operationRef.current = null;
      setOperation(null);
    }
  };

  const enableRoomAutomation = () => {
    if (!draft) return;
    const primary_account_id = recognitionEnabled && recognitionAccountId
      ? recognitionAccountId
      : draft.primary_account_id;
    const candidate = {
      ...draft,
      enabled: true,
      primary_account_id,
      follower_account_ids: (recognitionEnabled && recognitionAccountId
        ? [draft.primary_account_id, ...draft.follower_account_ids]
        : draft.follower_account_ids)
        .filter((id, index, values) => id && id !== primary_account_id && values.indexOf(id) === index),
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

  useEffect(() => {
    if (!draft?.enabled || dirty || bindingLoading || bindingError || !binding || binding.ready) {
      if (!draft?.enabled || binding?.ready) autoInstallAttemptRef.current = null;
      return;
    }
    if (binding.d2rRunning || operationRef.current) return;
    const attemptKey = [
      binding.d2rRunning,
      binding.totalFiles,
      binding.installedFiles,
      binding.conflictedFiles,
      binding.orphanBackupFiles,
    ].join(":");
    if (autoInstallAttemptRef.current === attemptKey) return;
    autoInstallAttemptRef.current = attemptKey;
    void updateBinding("install", gateway.installChatBinding);
  }, [binding, bindingError, bindingLoading, dirty, draft?.enabled, gateway]);

  useEffect(() => {
    if (!draft?.enabled || binding?.ready || bindingLoading || bindingError) return;
    const timer = window.setInterval(() => {
      void gateway.getChatBinding().then(setBinding).catch(() => undefined);
    }, 1500);
    return () => window.clearInterval(timer);
  }, [binding?.ready, bindingError, bindingLoading, draft?.enabled, gateway]);

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

  const bindingNeedsAttention = draft.enabled && (!binding?.ready || !!bindingError || bindingLoading);
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
                  || !binding?.ready || !!bindingError || !onSaveLaunchScheme}
                onClick={() => void onSaveLaunchScheme?.(participantAccountIds)}
              >保存当前启动方案</Button>
            )}
            <Toggle
              checked={draft.enabled}
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
            {saving || dirty ? copy.applying : copy.applied}
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
          {stale && <Button size="sm" onClick={reload}>{copy.retryLoad}</Button>}
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
            <label className="room-automation-field room-automation-field-wide">
              <span>{copy.primary}</span>
              <select
                className="settings-input"
                value={draft.primary_account_id}
                disabled={editorDisabled}
                aria-invalid={!!validation?.fieldErrors.primary}
                onChange={(event) => updateDraft((current) => {
                  const primary_account_id = event.target.value;
                  const follower_account_ids = current.follower_account_ids.filter((id) => id !== primary_account_id);
                  return {
                    ...current,
                    primary_account_id,
                    follower_account_ids,
                  };
                })}
              >
                <option value="">{copy.selectPrimary}</option>
                {eligibleAccounts.map((account) => <option value={account.id} key={account.id}>{accountLabel(account)}</option>)}
              </select>
              {validation?.fieldErrors.primary && <small role="alert">{validation.fieldErrors.primary}</small>}
            </label>

            <fieldset className="room-automation-followers" disabled={editorDisabled}>
              <legend>{copy.followers}</legend>
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
                      {compatible.length > 0 || processable.length > 0 ? <select
                          className="settings-input"
                          aria-label={`${accountLabel(account)} 选择 Mod`}
                          value={ready ? selection?.selected_capsule_id ?? "" : ""}
                          disabled={editorDisabled || assigningAccountId === accountId
                            || (compatible.length ? !onAssignModCapsule : !onRequireRoomTools)}
                          onChange={(event) => {
                            const capsule = (compatible.length ? compatible : processable)
                              .find((candidate) => candidate.id === event.target.value);
                            if (!capsule) return;
                            if (compatible.length) void onAssignModCapsule?.(accountId, capsule.id);
                            else onRequireRoomTools?.(accountId, capsule.id, !!capsule.processed);
                          }}
                        >
                          <option value="">{copy.selectCapsule}</option>
                          {(compatible.length ? compatible : processable).map((capsule) => (
                            <option value={capsule.id} key={capsule.id}>{capsule.name}</option>
                          ))}
                        </select> : (
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
        <fieldset className="room-automation-mode-options" disabled={editorDisabled}>
          <label data-selected={!draft.auto_followers_enabled ? "true" : "false"}>
            <input
              type="radio"
              name="room-follower-mode"
              checked={!draft.auto_followers_enabled}
              onChange={() => updateDraft((current) => ({ ...current, auto_followers_enabled: false }))}
            />
            <span><strong>{copy.manualMode}</strong><small>{copy.manualModeHelp}</small></span>
          </label>
          <label data-selected={draft.auto_followers_enabled ? "true" : "false"}>
            <input
              type="radio"
              name="room-follower-mode"
              checked={draft.auto_followers_enabled}
              onChange={() => updateDraft((current) => ({ ...current, auto_followers_enabled: true }))}
            />
            <span><strong>{copy.automaticMode}</strong><small>{copy.automaticModeHelp}</small></span>
          </label>
        </fieldset>

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
            <code>{generatedRoomName(draft)}</code>
          </div>
          <div className="room-automation-room-name-fields">
            <TextField label={copy.prefix} value={draft.name_prefix} maxLength={15} disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.prefix}
            onChange={(name_prefix) => updateDraft((current) => ({ ...current, name_prefix }))} />
            <NumberField label={copy.sequence} value={draft.next_sequence} min={0} max={4_294_967_295} disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.sequence}
            onChange={(next_sequence) => updateDraft((current) => ({ ...current, next_sequence }))} />
            <NumberField label={copy.width} value={draft.sequence_width} min={1} max={6} disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.sequence}
            onChange={(sequence_width) => updateDraft((current) => ({ ...current, sequence_width }))} />
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

      <details
        className="spatial-panel room-automation-advanced room-automation-binding-details"
        open={bindingNeedsAttention || undefined}
      >
        <summary>
          <span>
            <strong id="room-binding-title">{copy.f13Title}</strong>
            <small>{binding?.ready ? copy.bindingReady : copy.bindingNotReady}</small>
          </span>
          <ChevronDown size={15} aria-hidden="true" />
        </summary>
        <div className="room-automation-binding-body">
          <p className="room-automation-consent-copy">{copy.f13Description}</p>
          <div className="flex justify-end">
          <Button
            size="sm"
            variant="ghost"
            loading={bindingLoading}
            disabled={bindingLoading || !!operation}
            onClick={() => {
              autoInstallAttemptRef.current = null;
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
            {binding?.watcherRunning && <span className="room-automation-watcher">{copy.watcherActive}</span>}
          </div>
        )}
        <p className="room-automation-consent-copy">{copy.f13Consent}</p>
        {binding?.d2rRunning && <p className="room-automation-field-error" role="alert">{copy.gameMustClose}</p>}
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
      </details>

      <details className="spatial-panel room-automation-advanced">
        <summary>
          <span><strong>{copy.advanced}</strong><small>{copy.advancedHelp}</small></span>
          <ChevronDown size={15} aria-hidden="true" />
        </summary>
        <fieldset disabled={editorDisabled}>
          <div className="room-automation-fields">
            <NumberField label={copy.stepDelay} value={draft.flow.step_delay_ms} min={0} max={2000}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(step_delay_ms) => updateDraft((current) => ({ ...current, flow: { ...current.flow, step_delay_ms } }))} />
            <NumberField label={copy.characterDelay} value={draft.flow.character_delay_ms} min={10} max={250}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(character_delay_ms) => updateDraft((current) => ({ ...current, flow: { ...current.flow, character_delay_ms } }))} />
            <label className="room-automation-field room-automation-field-wide">
              <span>{copy.backgroundStrategy}</span>
              <select
                className="settings-input"
                value={draft.background_text_strategy}
                onChange={(event) => updateDraft((current) => ({
                  ...current,
                  background_text_strategy: event.target.value as RoomAutomationConfig["background_text_strategy"],
                }))}
              >
                <option value="post_keys">{copy.postKeys}</option>
                <option value="send_keys">{copy.sendKeys}</option>
              </select>
            </label>
          </div>
          {validation?.fieldErrors.timing && <p className="room-automation-field-error" role="alert">{validation.fieldErrors.timing}</p>}
        </fieldset>
      </details>
    </div>
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
  return (
    <label className="room-automation-field">
      <span>{label}</span>
      <input
        type="number"
        className="settings-input"
        value={value}
        min={min}
        max={max}
        disabled={disabled}
        aria-invalid={invalid || undefined}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </label>
  );
}
