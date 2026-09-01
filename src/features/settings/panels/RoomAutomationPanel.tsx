import {
  AlertCircle,
  CheckCircle2,
  ChevronDown,
  KeyRound,
  Package,
  Play,
  RefreshCw,
  RotateCcw,
  Save,
  UsersRound,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { Button } from "../../../components/ui/Button";
import { Toggle } from "../../../components/ui/Toggle";
import { parseShortcutFromKeyEvent } from "../../../hooks/useShortcutRecorder";
import type { AccountMeta } from "../../../store/types";
import { ROOM_AUTOMATION_COPY } from "../../roomAutomation/copy";
import {
  roomAutomationGateway,
  type RoomAutomationGateway,
} from "../../roomAutomation/gateway";
import {
  canonicalizeRoomAutomationShortcut,
  generatedRoomName,
  roomAutomationConfigsEqual,
  shouldEnableFollowerAction,
  shouldEnablePrimaryAction,
  shouldEnableRetry,
  validateRoomAutomationConfig,
} from "../../roomAutomation/model";
import type {
  RoomAutomationConfig,
  RoomAutomationConfigSnapshot,
  RoomAutomationWorkflowStatus,
  RoomChatBindingStatus,
  RoomFlowProfile,
} from "../../roomAutomation/types";
import { normalizeSettingsLanguage } from "../settingsRegistry";

interface RoomAutomationPanelProps {
  accounts: AccountMeta[];
  language?: string | null;
  gateway?: RoomAutomationGateway;
  onDirtyChange?: (dirty: boolean) => void;
  onOpenAudioModSettings?: () => void;
}

type Operation = "save" | "primary" | "followers" | "retry" | "cancel" | "install" | "restore";

function cloneConfig(config: RoomAutomationConfig): RoomAutomationConfig {
  return {
    ...config,
    follower_account_ids: [...config.follower_account_ids],
    standard_flow: { ...config.standard_flow },
    direct_lobby_flow: { ...config.direct_lobby_flow },
    account_flow_bindings: { ...config.account_flow_bindings },
  };
}

function keepSelectedFlowBindings(
  bindings: RoomAutomationConfig["account_flow_bindings"],
  primaryId: string,
  followerIds: string[],
): RoomAutomationConfig["account_flow_bindings"] {
  const selected = new Set([primaryId, ...followerIds].filter(Boolean));
  return Object.fromEntries(Object.entries(bindings).filter(([accountId]) => selected.has(accountId)));
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
  onDirtyChange,
  onOpenAudioModSettings,
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
  const dirtyRef = useRef(false);
  const operationRef = useRef<Operation | null>(null);
  const dirty = useMemo(
    () => !roomAutomationConfigsEqual(snapshot?.config ?? null, draft),
    [draft, snapshot],
  );

  useEffect(() => {
    dirtyRef.current = dirty;
    onDirtyChange?.(dirty);
  }, [dirty, onDirtyChange]);

  useEffect(() => {
    operationRef.current = operation;
  }, [operation]);

  const commitConfig = useCallback((next: RoomAutomationConfigSnapshot) => {
    if (snapshotRef.current && next.generation < snapshotRef.current.generation) return false;
    snapshotRef.current = next;
    setSnapshot(next);
    setDraft(cloneConfig(next.config));
    dirtyRef.current = false;
    setStale(false);
    return true;
  }, []);

  const acceptConfig = useCallback((next: RoomAutomationConfigSnapshot) => {
    if (dirtyRef.current) {
      if (!snapshotRef.current || next.generation > snapshotRef.current.generation) setStale(true);
      return;
    }
    if (operationRef.current === "save") return;
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

  const validation = useMemo(() => draft
    ? validateRoomAutomationConfig(draft, copy, eligibleAccounts.map((account) => account.id))
    : null, [copy, draft, eligibleAccounts]);
  const workflowActive = isWorkflowActive(status);
  const manualWaiting = status?.phase === "waiting" && status.waiting_mode?.mode === "manual";
  const saving = operation === "save";
  const editorDisabled = loading || !!unavailable || workflowActive || !!operation;
  const operationBlocked = loading || !!unavailable || dirty || !!operation || !draft?.enabled;
  const workflowBlocked = operationBlocked || !binding?.ready || !!bindingError || bindingLoading;
  const followerProgress = status
    ? `${status.completed_follower_account_ids.length}/${status.follower_account_ids.length}`
    : "0/0";

  const apply = async () => {
    if (!snapshot || !draft || !validation?.valid || workflowActive || operationRef.current) return;
    operationRef.current = "save";
    setOperation("save");
    setOperationError(null);
    try {
      const outcome = await gateway.saveConfig(snapshot.generation, draft);
      commitConfig(outcome.snapshot);
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
  };

  const runWorkflowAction = async (
    kind: Exclude<Operation, "save" | "install" | "restore">,
    action: () => Promise<RoomAutomationWorkflowStatus>,
  ) => {
    const blocked = kind === "cancel"
      ? loading || !!unavailable || !workflowActive
      : workflowBlocked
        || (kind === "primary" && !shouldEnablePrimaryAction(status))
        || (kind === "followers" && !shouldEnableFollowerAction(status))
        || (kind === "retry" && !shouldEnableRetry(status));
    if (blocked || operationRef.current) return;
    operationRef.current = kind;
    setOperation(kind);
    setOperationError(null);
    try {
      acceptStatus(await action());
    } catch (error) {
      setOperationError(`${copy.actionFailed}: ${errorMessage(error)}`);
    } finally {
      operationRef.current = null;
      setOperation(null);
    }
  };

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

  const primary = eligibleAccounts.find((account) => account.id === draft.primary_account_id);
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
  const retryLabel = status?.recovery_action === "resume_followers"
    ? copy.retryFollowers
    : status?.recovery_action === "retry_primary"
      ? copy.retryPrimary
      : copy.retry;

  return (
    <div className="room-automation-panel">
      <header className="room-automation-header">
        <div className="min-w-0">
          <div className="room-automation-title-line">
            <h2>{copy.title}</h2>
            <span className="room-automation-semantic-status" data-tone={statusTone} role="status">
              <span className="room-automation-state-dot" data-tone={statusTone} aria-hidden="true" />
              {statusTitle}
            </span>
          </div>
          <p>{copy.subtitle}</p>
        </div>
        <Toggle
          checked={draft.enabled}
          disabled={editorDisabled}
          ariaLabel={copy.enabled}
          descriptionId={!draft.enabled ? "room-automation-module-description" : undefined}
          onChange={(enabled) => updateDraft((current) => ({ ...current, enabled }))}
        />
      </header>
      {!draft.enabled && (
        <p id="room-automation-module-description" className="room-automation-disabled-note">
          {copy.disabledDescription}
        </p>
      )}

      <div className="room-automation-apply-bar" data-dirty={dirty ? "true" : "false"}>
        <span aria-live="polite">
          {dirty ? copy.unsaved : copy.applied}
          <span className="room-automation-generation"> · v{snapshot.generation}</span>
        </span>
        <div className="room-automation-apply-actions">
          {dirty && (
            <Button
              size="sm"
              variant="ghost"
              disabled={!!operation}
              onClick={reload}
            >
              {copy.discard}
            </Button>
          )}
          <Button
            size="sm"
            variant="primary"
            loading={saving}
            disabled={!dirty || stale || !validation?.valid || workflowActive || (!!operation && !saving)}
            onClick={() => void apply()}
          >
            <Save size={13} aria-hidden="true" />
            {saving ? copy.applying : copy.apply}
          </Button>
        </div>
      </div>

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

      <section className="room-automation-prerequisite" aria-labelledby="room-mod-prerequisite-title">
        <Package size={17} aria-hidden="true" />
        <div>
          <strong id="room-mod-prerequisite-title">{copy.modPrerequisiteTitle}</strong>
          <p>{copy.modPrerequisiteDescription}</p>
          <code>{copy.modPrerequisiteFeature}</code>
        </div>
        {onOpenAudioModSettings && (
          <Button size="sm" variant="secondary" onClick={onOpenAudioModSettings}>
            {copy.manageModFeatures}
          </Button>
        )}
      </section>

      <section className="spatial-panel room-automation-section" aria-labelledby="room-participants-title">
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
                    account_flow_bindings: keepSelectedFlowBindings(
                      current.account_flow_bindings,
                      primary_account_id,
                      follower_account_ids,
                    ),
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
                            account_flow_bindings: keepSelectedFlowBindings(
                              current.account_flow_bindings,
                              current.primary_account_id,
                              follower_account_ids,
                            ),
                          };
                        })}
                      />
                      <span className="room-automation-account-name">{accountLabel(account)}</span>
                      <span className="room-automation-account-id">{account.id}</span>
                    </label>
                  );
                })}
              </div>
              {validation?.fieldErrors.followers && <small role="alert">{validation.fieldErrors.followers}</small>}
            </fieldset>

            {(primary || draft.follower_account_ids.length > 0) && (
              <div className="room-automation-flow-bindings">
                {[...(primary ? [primary] : []), ...eligibleAccounts.filter((account) => draft.follower_account_ids.includes(account.id))]
                  .map((account) => (
                    <label key={account.id}>
                      <span>{accountLabel(account)}</span>
                      <span className="sr-only">{copy.flowLabel}</span>
                      <select
                        className="settings-input"
                        disabled={editorDisabled}
                        value={draft.account_flow_bindings[account.id] ?? "standard"}
                        onChange={(event) => updateDraft((current) => ({
                          ...current,
                          account_flow_bindings: {
                            ...current.account_flow_bindings,
                            [account.id]: event.target.value as RoomFlowProfile,
                          },
                        }))}
                      >
                        <option value="standard">{copy.standardFlow}</option>
                        <option value="direct_lobby">{copy.directFlow}</option>
                      </select>
                    </label>
                  ))}
              </div>
            )}
          </>
        )}
      </section>

      <section className="spatial-panel room-automation-section" aria-labelledby="room-mode-title">
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

        <div className="room-automation-fields">
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
          <TextField label={copy.prefix} value={draft.name_prefix} maxLength={15} disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.prefix}
            onChange={(name_prefix) => updateDraft((current) => ({ ...current, name_prefix }))} />
          <TextField label={copy.password} value={draft.password} maxLength={15} disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.password}
            onChange={(password) => updateDraft((current) => ({ ...current, password }))} />
          <NumberField label={copy.sequence} value={draft.next_sequence} min={0} max={4_294_967_295} disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.sequence}
            onChange={(next_sequence) => updateDraft((current) => ({ ...current, next_sequence }))} />
          <NumberField label={copy.width} value={draft.sequence_width} min={1} max={6} disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.sequence}
            onChange={(sequence_width) => updateDraft((current) => ({ ...current, sequence_width }))} />
        </div>
        {(validation?.fieldErrors.shortcuts || validation?.fieldErrors.prefix || validation?.fieldErrors.password || validation?.fieldErrors.sequence) && (
          <p className="room-automation-field-error" role="alert">
            {validation.fieldErrors.shortcuts || validation.fieldErrors.prefix || validation.fieldErrors.password || validation.fieldErrors.sequence}
          </p>
        )}
        <div className="room-automation-preview">
          <span>{copy.preview}</span>
          <code>{generatedRoomName(draft)}</code>
        </div>
      </section>

      <section className="spatial-panel room-automation-section" aria-labelledby="room-workflow-title">
        <div className="room-automation-section-heading room-automation-workflow-heading">
          {status?.phase === "complete"
            ? <CheckCircle2 size={16} aria-hidden="true" />
            : status?.phase === "error"
              ? <AlertCircle size={16} aria-hidden="true" />
              : <Play size={16} aria-hidden="true" />}
          <div>
            <h3 id="room-workflow-title">{copy.workflow}</h3>
            <p aria-live="polite">{status ? copy.phase[status.phase] : copy.phase.idle}</p>
          </div>
        </div>
        {status && status.phase !== "idle" && (
          <dl className="room-automation-workflow-data">
            {status.room_name && <div><dt>{copy.room}</dt><dd>{status.room_name}</dd></div>}
            <div><dt>{copy.attempt}</dt><dd>{status.attempt}</dd></div>
            <div><dt>{copy.followerProgress}</dt><dd>{followerProgress}</dd></div>
          </dl>
        )}
        {status?.waiting_mode && (
          <p className="room-automation-waiting-note">
            {status.waiting_mode.mode === "automatic"
              ? copy.waitAutomatic(status.waiting_mode.delay_secs)
              : copy.waitManual}
          </p>
        )}
        {status?.last_error && <p className="room-automation-field-error" role="alert">{status.last_error}</p>}
        <div className="room-automation-actions">
          <Button
            size="sm"
            variant="primary"
            loading={operation === "primary"}
            disabled={workflowBlocked || !shouldEnablePrimaryAction(status)}
            onClick={() => void runWorkflowAction("primary", gateway.startPrimary)}
          ><Play size={13} aria-hidden="true" />{manualWaiting ? copy.nextPrimary : copy.startPrimary}</Button>
          <Button
            size="sm"
            loading={operation === "followers"}
            disabled={workflowBlocked || !shouldEnableFollowerAction(status)}
            onClick={() => void runWorkflowAction("followers", gateway.startFollowers)}
          ><UsersRound size={13} aria-hidden="true" />{copy.startFollowers}</Button>
          <Button
            size="sm"
            loading={operation === "retry"}
            disabled={workflowBlocked || !shouldEnableRetry(status)}
            onClick={() => void runWorkflowAction("retry", gateway.retry)}
          ><RotateCcw size={13} aria-hidden="true" />{retryLabel}</Button>
          <Button
            size="sm"
            variant="danger"
            loading={operation === "cancel"}
            disabled={loading || !!unavailable || !!operation || !workflowActive}
            onClick={() => void runWorkflowAction("cancel", gateway.cancel)}
          ><X size={13} aria-hidden="true" />{copy.cancel}</Button>
        </div>
      </section>

      <section className="spatial-panel room-automation-section" aria-labelledby="room-binding-title">
        <div className="room-automation-section-heading room-automation-section-heading-action">
          <KeyRound size={16} aria-hidden="true" />
          <div>
            <h3 id="room-binding-title">{copy.f13Title}</h3>
            <p>{copy.f13Description}</p>
          </div>
          <Button
            size="sm"
            variant="ghost"
            loading={bindingLoading}
            disabled={bindingLoading || !!operation}
            onClick={() => setBindingReloadKey((current) => current + 1)}
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
              <strong>{binding?.ready ? copy.bindingReady : copy.bindingNotReady}</strong>
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
        <div className="room-automation-actions">
          <Button
            size="sm"
            variant="primary"
            loading={operation === "install"}
            disabled={editorDisabled || dirty || !draft.enabled || !binding || bindingLoading
              || !!bindingError || binding.d2rRunning}
            onClick={() => void updateBinding("install", gateway.installChatBinding)}
          >{copy.installBinding}</Button>
          <Button
            size="sm"
            loading={operation === "restore"}
            disabled={editorDisabled || dirty || !binding || bindingLoading
              || !!bindingError || binding.d2rRunning
              || (!binding.backupFiles && !binding.consentGranted && !binding.watcherRunning
                && !draft.chat_f13_auto_patch_enabled)}
            onClick={() => void updateBinding("restore", gateway.restoreChatBinding)}
          >{copy.restoreBinding}</Button>
        </div>
      </section>

      <details className="spatial-panel room-automation-advanced">
        <summary>
          <span><strong>{copy.advanced}</strong><small>{copy.advancedHelp}</small></span>
          <ChevronDown size={15} aria-hidden="true" />
        </summary>
        <fieldset disabled={editorDisabled}>
          <div className="room-automation-fields">
            <NumberField label={copy.standardStep} value={draft.standard_flow.step_delay_ms} min={0} max={2000}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(step_delay_ms) => updateDraft((current) => ({ ...current, standard_flow: { ...current.standard_flow, step_delay_ms } }))} />
            <NumberField label={copy.standardCharacter} value={draft.standard_flow.character_delay_ms} min={10} max={250}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(character_delay_ms) => updateDraft((current) => ({ ...current, standard_flow: { ...current.standard_flow, character_delay_ms } }))} />
            <NumberField label={copy.directStep} value={draft.direct_lobby_flow.step_delay_ms} min={0} max={2000}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(step_delay_ms) => updateDraft((current) => ({ ...current, direct_lobby_flow: { ...current.direct_lobby_flow, step_delay_ms } }))} />
            <NumberField label={copy.directCharacter} value={draft.direct_lobby_flow.character_delay_ms} min={10} max={250}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(character_delay_ms) => updateDraft((current) => ({ ...current, direct_lobby_flow: { ...current.direct_lobby_flow, character_delay_ms } }))} />
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
