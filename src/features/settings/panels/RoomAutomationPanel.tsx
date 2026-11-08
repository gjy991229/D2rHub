import { switchPrimary } from "../../roomAutomation/roles";
import { RoomCollaborationMode } from "../../roomAutomation/RoomCollaborationMode";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { copyRoomRule, parseRoomRule } from "../../roomAutomation/ruleShare";
import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  ChevronDown,
  KeyRound,
  UsersRound,
} from "lucide-react";
import { useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { Button } from "../../../components/ui/Button";
import { Toggle } from "../../../components/ui/Toggle";
import { parseShortcutFromKeyEvent, useShortcutRecorder, validateShortcutAvailability } from "../../../hooks/useShortcutRecorder";
import { useGlobalConfig } from "../../../store/globalConfig";
import { normalizeShortcut } from "../../../utils/shortcut";
import type { AccountMeta, ModCapsulePool } from "../../../store/types";

import { ROOM_AUTOMATION_COPY } from "../../roomAutomation/copy";
import {
  roomAutomationGateway,
  type RoomAutomationGateway,
} from "../../roomAutomation/gateway";
import {
  canonicalizeRoomAutomationShortcut,
  DEFAULT_ROOM_FLOW_TIMING,
  generatedRoomName,
  roomAutomationConfigsEqual,
  validateRoomAutomationConfig,
} from "../../roomAutomation/model";
import type {
  RoomAutomationConfig,
  RoomAutomationConfigSnapshot,
  RoomAutomationWorkflowStatus,
} from "../../roomAutomation/types";
import { normalizeSettingsLanguage } from "../settingsRegistry";
import "../roomAutomationParticipants.css";
import { VisualRecognitionSettings } from "./VisualRecognitionSettings";

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

type Operation = "save";

function cloneConfig(config: RoomAutomationConfig): RoomAutomationConfig {
  return {
    ...config,
    follower_join_mode: config.follower_join_mode ?? "simultaneous",
    follower_join_interval_secs: config.follower_join_interval_secs ?? 3,
    follower_account_ids: [...config.follower_account_ids],
    flow: { ...config.flow,
      key_hold_ms: config.flow.key_hold_ms ?? DEFAULT_ROOM_FLOW_TIMING.key_hold_ms,
      chord_hold_ms: config.flow.chord_hold_ms ?? DEFAULT_ROOM_FLOW_TIMING.chord_hold_ms,
      form_settle_ms: config.flow.form_settle_ms ?? DEFAULT_ROOM_FLOW_TIMING.form_settle_ms,
      physical_ctrl_settle_ms: config.flow.physical_ctrl_settle_ms ?? DEFAULT_ROOM_FLOW_TIMING.physical_ctrl_settle_ms,
      sync_message_timeout_ms: config.flow.sync_message_timeout_ms ?? DEFAULT_ROOM_FLOW_TIMING.sync_message_timeout_ms,
    },
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
  onSaveLaunchScheme,
}: RoomAutomationPanelProps) {
  const locale = normalizeSettingsLanguage(language);
  const copy = ROOM_AUTOMATION_COPY[locale];
  const eligibleAccounts = useMemo(() => accounts.filter((account) => account.initialized), [accounts]);
  const [snapshot, setSnapshot] = useState<RoomAutomationConfigSnapshot | null>(null);
  const [section, setSection] = useState("room");
  const [draft, setDraft] = useState<RoomAutomationConfig | null>(null);
  const [status, setStatus] = useState<RoomAutomationWorkflowStatus | null>(null);
  const [ruleMessage, setRuleMessage] = useState("");
  const [resetChatHistory, setResetChatHistory] = useState(0);
  const resetChatHistoryRef = useRef(0);
  const [loading, setLoading] = useState(true);
  const [unavailable, setUnavailable] = useState<string | null>(null);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [stale, setStale] = useState(false);
  const [operation, setOperation] = useState<Operation | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const snapshotRef = useRef<RoomAutomationConfigSnapshot | null>(null);
  const draftRef = useRef<RoomAutomationConfig | null>(null);
  const dirtyRef = useRef(false);
  const operationRef = useRef<Operation | null>(null);
  const mountedRef = useRef(true);
  const persistDraftRef = useRef<((candidate: RoomAutomationConfig) => Promise<void>) | null>(null);
  const dirty = useMemo(
    () => resetChatHistory > 0 || !roomAutomationConfigsEqual(snapshot?.config ?? null, draft),
    [draft, snapshot, resetChatHistory],
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
    resetChatHistoryRef.current = 0;
    setResetChatHistory(0);
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

  const updateDraft = useCallback((updater: (current: RoomAutomationConfig) => RoomAutomationConfig) => {
    setDraft((current) => {
      if (!current) return current;
      const next = updater(cloneConfig(current));
      draftRef.current = next;
      dirtyRef.current = resetChatHistoryRef.current > 0 || !roomAutomationConfigsEqual(snapshotRef.current?.config ?? null, next);
      return next;
    });
    setOperationError(null);
  }, []);

  const reload = () => {
    resetChatHistoryRef.current = 0;
    setResetChatHistory(0);
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
  const saving = operation === "save";
  const editorDisabled = loading || !!unavailable || (!!operation && operation !== "save");
  const independentRooms = draft?.collaboration_mode === "uber_diablo";
  const sharedFollower = draft?.collaboration_mode === "follower";
  const followerJoinMode = draft?.follower_join_mode ?? "simultaneous";
  const participantAccountIds = useMemo(() => draft
    ? [draft.primary_account_id, ...draft.follower_account_ids].filter(Boolean)
    : [], [draft]);
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
    const resetRequest = resetChatHistoryRef.current;
    try {
      const outcome = resetRequest
        ? await gateway.saveConfig(currentSnapshot.generation, candidate, true)
        : await gateway.saveConfig(currentSnapshot.generation, candidate);
      if (resetChatHistoryRef.current === resetRequest) {
        resetChatHistoryRef.current = 0;
        setResetChatHistory(0);
      }
      saved = true;
      snapshotRef.current = outcome.snapshot;
      setSnapshot(outcome.snapshot);
      if (roomAutomationConfigsEqual(draftRef.current, candidate)) {
        const committed = cloneConfig(outcome.snapshot.config);
        draftRef.current = committed;
        setDraft(committed);
        dirtyRef.current = resetChatHistoryRef.current > 0;
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

  const checkShortcutConflict = (shortcut: string, field: "shortcut" | "join_shortcut") => {
    const config = useGlobalConfig.getState().config;
    let bindings: Record<string, string> = {};
    try { bindings = JSON.parse(config?.shortcut_bindings_json || "{}"); } catch { /* No saved bindings. */ }
    const otherField = field === "shortcut" ? "join_shortcut" : "shortcut";
    const assigned = [
      ...Object.entries(bindings).map(([position, value]) => ({
        shortcut: value,
        label: locale === "en-US" ? `account position #${position}` : `账号位置 #${position}`,
      })),
      { shortcut: config?.show_main_window_shortcut || config?.hide_main_window_shortcut || "", label: locale === "en-US" ? "Toggle main window" : "切换主面板" },
      { shortcut: draftRef.current?.[otherField] || "", label: otherField === "shortcut" ? copy.createShortcut : copy.followerShortcut },
    ];
    const conflict = assigned.find(entry => normalizeShortcut(entry.shortcut).toLowerCase() === shortcut.toLowerCase());
    if (conflict) {
      throw new Error(locale === "en-US"
        ? `${shortcut} is already assigned to ${conflict.label}. Previous binding kept.`
        : `快捷键 ${shortcut} 已用于${conflict.label}，原设置保持不变`);
    }
  };

  useEffect(() => {
    if (!draft || !dirty || !validation?.valid || stale || operationError || operationRef.current) return;
    const timer = window.setTimeout(() => void persistDraft(cloneConfig(draft)), 450);
    return () => window.clearTimeout(timer);
  }, [dirty, draft, operationError, persistDraft, saving, stale, validation?.valid]);

  const enableRoomAutomation = () => {
    if (!draft) return;
    const candidate = {
      ...draft,
      enabled: true,
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

  const statusNeedsAttention = status?.phase === "error" || stale || !!operationError;
  const runtimeEnabled = snapshot?.config.enabled ?? false;
  const statusTone = statusNeedsAttention ? "danger" : workflowActive && status?.phase !== "waiting" ? "success" : runtimeEnabled ? "warning" : "neutral";
  const statusTitle = statusNeedsAttention ? (locale === "en-US" ? "Needs attention" : "需要处理")
    : !runtimeEnabled ? copy.disabled
      : status?.phase === "waiting" ? (locale === "en-US" ? "Waiting for followers" : "等待跟随")
        : workflowActive ? (locale === "en-US" ? "Running" : "运行中")
          : (locale === "en-US" ? "Configured · Waiting for action" : "已配置 · 等待操作");
  return (
    <div className="room-automation-panel settings-guided-page">
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
                disabled={editorDisabled || dirty || !validation?.valid
                  || !onSaveLaunchScheme}
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
          <span className="room-automation-semantic-status runtime-state" data-state={statusNeedsAttention ? "attention" : !runtimeEnabled ? "off" : workflowActive && status?.phase !== "waiting" ? "running" : "waiting"} data-tone={statusTone}>
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
      <nav className="settings-section-tabs" aria-label={locale === "en-US" ? "Room automation sections" : "跟房设置分页"}>
        {[["room", locale === "en-US" ? "Room & actions" : "房间与操作"], ["accounts", locale === "en-US" ? "Accounts & Mods" : "参与账号"], ["advanced", locale === "en-US" ? "Advanced timing" : "高级时序"]].map(([id, label]) =>
          <button type="button" key={id} aria-current={section === id ? "page" : undefined} onClick={() => setSection(id)}>{label}{id === "accounts" && <span className="settings-section-count"> {participantAccountIds.length}{(validation?.fieldErrors.primary || validation?.fieldErrors.followers) && <span className="text-warning"> · {locale === "en-US" ? "Setup needed" : "待完善"}</span>}</span>}</button>)}
      </nav>

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

      <div className="room-automation-setup settings-section-body" hidden={section !== "accounts"}>
      <section className="spatial-panel room-automation-pane room-automation-section" aria-labelledby="room-participants-title">
        <div className="room-automation-section-heading">
          <UsersRound size={16} aria-hidden="true" />
          <div>
            <h3 id="room-participants-title">{copy.participants}</h3>
            <p>{sharedFollower ? (locale === "en-US" ? "The primary listens for signals and joins together with the followers." : "主号负责监听聊天信号，并与小号一起加入房间。") : copy.participantsHelp}</p>
          </div>
        </div>
        {eligibleAccounts.length === 0 ? (
          <p className="room-automation-empty">{copy.noAccounts}</p>
        ) : (
          <>
            <p className="feature-help">{sharedFollower
              ? (locale === "en-US" ? "Keep your normal account setup. The primary is included automatically and does not create rooms in this mode." : "沿用平时的账号配置，主号会自动加入队伍，本机不执行创建。无需将主号重复勾选为小号。")
              : (locale === "en-US" ? "The join shortcut can join the local rule directly. The sequence advances after all join commands are delivered." : "未创建房间时，也可用跟房快捷键直接加入本机规则房间。全部加入指令送达后序号递增。")}</p>
            <div className="room-automation-field">
              <ChoiceField label={copy.primary} value={draft.primary_account_id}
                options={eligibleAccounts.map((account) => ({ value: account.id, label: accountLabel(account) }))}
                placeholder={copy.selectPrimary}
                disabled={editorDisabled}
                invalid={!!validation?.fieldErrors.primary}
                onChange={(id) => updateDraft((current) => switchPrimary(current, id))}
              />
              {validation?.fieldErrors.primary && <small role="alert">{validation.fieldErrors.primary}</small>}
            </div>

            <fieldset className="room-automation-followers" disabled={editorDisabled}>
              <legend>{copy.followers}</legend>
              <div className="room-automation-followers-toolbar">
                <span>{locale === "en-US" ? "Followers" : "跟随"} · {copy.followersSelected(draft.follower_account_ids.length)}</span>
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
              {(followerJoinMode === "interval") && draft.follower_account_ids.length > 0 && (
                <div className="room-automation-follower-order">
                  <div className="room-automation-follower-order-heading">
                    <strong>{copy.followerOrder}</strong>
                    <span>{copy.followerJoinOrderHelp}</span>
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

            <p className="feature-help">{locale === "en-US" ? "Room tools are checked against each participating account’s launch arguments before starting." : "启动前会检查本次参与账号实际使用的 Mod 是否支持局内房间工具。"}</p>

          </>
        )}
      </section>

      </div>
      <div className="settings-section-body room-daily-settings" hidden={section !== "room"}>
        <button type="button" className="settings-context-link" onClick={() => setSection("accounts")}>
          <UsersRound size={14} aria-hidden="true" /><span>{locale === "en-US" ? "Primary" : "主号"} · {eligibleAccounts.find(account => account.id === draft.primary_account_id)?.display_name || draft.primary_account_id || (locale === "en-US" ? "Choose an account" : "尚未选择")}</span>
          <span>{locale === "en-US" ? "Followers" : "跟随"} · {copy.followersSelected(draft.follower_account_ids.length)}</span><span className="settings-context-action">{locale === "en-US" ? "Configure" : "配置账号"} →</span>
        </button>
      <RoomCollaborationMode compact value={draft.collaboration_mode} onChange={collaboration_mode => updateDraft(current => ({ ...current, collaboration_mode }))} disabled={editorDisabled} en={locale === "en-US"} />
      <section className="spatial-panel room-automation-pane room-automation-section" aria-labelledby="room-mode-title">
        <div className="room-automation-section-heading">
          <KeyRound size={16} aria-hidden="true" />
          <div><h3 id="room-mode-title">{copy.modeAndRoom}</h3></div>
        </div>
        <div className="room-automation-room-builder">
          <div className="room-automation-room-builder-heading">
            <div><strong>{copy.roomNaming}</strong><span>{copy.roomNamingHelp}</span></div>
            <code aria-label={copy.preview} title={copy.preview}>{generatedRoomName(draft)}</code>
          </div>
          <div className="room-rule-sharing">
            <p>{language === "en-US" ? "Share before creating the next room. Includes the password." : "建房前分享规则，双方使用相同序号。复制内容包含密码。"}</p>
            <div>
              <Button size="sm" disabled={editorDisabled} onClick={() => void writeText(copyRoomRule(draft))
                .then(() => setRuleMessage("规则已复制"), () => setRuleMessage("复制失败，请重试"))}>{language === "en-US" ? "Copy" : "复制"}</Button>
              <Button size="sm" disabled={editorDisabled} onClick={async () => {
                try { const rule = parseRoomRule(await readText());
                  resetChatHistoryRef.current += 1;
                  setResetChatHistory(resetChatHistoryRef.current);
                  updateDraft(current => ({ ...current, ...rule }));
                  setRuleMessage("规则已粘贴，保存后重新同步聊天信号，请确认跟随账号");
                } catch (error) { setRuleMessage(error instanceof Error ? error.message : "粘贴失败"); }
              }}>{language === "en-US" ? "Paste" : "粘贴"}</Button>
            </div>
          </div>
          {ruleMessage && <p className="text-xs text-text-muted" role="status">{ruleMessage}</p>}
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
        <details className="settings-help-disclosure room-join-options">
          <summary><span>{locale === "en-US" ? "Shortcuts & join behavior" : "快捷键与进房方式"}<small>{draft.shortcut} · {draft.join_shortcut}</small></span><ChevronDown size={14} aria-hidden="true" /></summary>
          <div className="settings-disclosure-content">
        <p className="room-automation-consent-copy">{independentRooms ? (locale === "en-US" ? "Create assigns one separate room per account, primary first. Password is shared. Reserved numbers are not reused after failure." : "创建时主号优先，再按小号配置顺序各建一间，共用密码。整段序号在操作前预留，失败后不复用。") : sharedFollower ? (locale === "en-US" ? "A new chat signal starts a group join after 1 second plus the configured wait. Both shortcuts can also join manually." : "识别到新聊天信号后，等待 1 秒加下方等待时间，再让本机主号和小号加入。两个快捷键也可手动触发加入。") : copy.backgroundMethodHelp}</p>
        {!independentRooms && <div className="room-automation-behavior-fields">
          {!sharedFollower && <ChoiceField label={copy.followTrigger} value={draft.auto_followers_enabled ? "auto" : "manual"}
            options={[{ value: "manual", label: copy.manualMode }, { value: "auto", label: copy.automaticMode }]}
            disabled={editorDisabled}
            help={draft.auto_followers_enabled ? copy.automaticModeHelp : copy.manualModeHelp}
            onChange={(value) => updateDraft((current) => ({ ...current, auto_followers_enabled: value === "auto" }))} />}
          <ChoiceField label={sharedFollower ? (locale === "en-US" ? "Group join method" : "本机队伍进房方式") : copy.followerJoinMode} value={followerJoinMode}
            options={[
              { value: "simultaneous", label: copy.simultaneousJoin },
              { value: "interval", label: copy.intervalJoin },
            ]}
            disabled={editorDisabled}
            help={followerJoinMode === "interval"
              ? copy.intervalJoinHelp
              : copy.simultaneousJoinHelp}
            onChange={(value) => updateDraft((current) => ({
              ...current, follower_join_mode: value as RoomAutomationConfig["follower_join_mode"],
            }))} />
        </div>}

        <div className="room-automation-shortcuts">
          {!independentRooms && (draft.auto_followers_enabled || sharedFollower) && (
            <NumberField
              label={sharedFollower ? (locale === "en-US" ? "Signal wait (seconds)" : "信号后等待（秒）") : copy.followerDelay}
              value={draft.auto_followers_delay_secs}
              min={0.5}
              step={0.5}
              max={60}
              disabled={editorDisabled}
              onChange={(value) => updateDraft((current) => ({ ...current, auto_followers_delay_secs: value }))}
            />
          )}
          {!independentRooms && followerJoinMode === "interval" && (
            <NumberField
              label={copy.followerJoinInterval}
              value={draft.follower_join_interval_secs ?? 3}
              min={0.5}
              step={0.5}
              max={60}
              disabled={editorDisabled}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(value) => updateDraft((current) => ({ ...current, follower_join_interval_secs: value }))}
            />
          )}
          <ShortcutField
            label={draft.collaboration_mode === "follower" ? (locale === "en-US" ? "Join next room" : "加入下一局") : copy.createShortcut}
            checkingLabel={locale === "en-US" ? "Checking availability..." : "正在检查占用..."}
            checkConflict={shortcut => checkShortcutConflict(shortcut, "shortcut")}
            value={draft.shortcut}
            captureHint={copy.shortcutCapture}
            recordingLabel={copy.shortcutRecording}
            disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.shortcuts}
            onChange={(shortcut) => updateDraft((current) => ({ ...current, shortcut }))}
          />
          <ShortcutField
            label={independentRooms ? (locale === "en-US" ? "Join shortcut (unused in this mode)" : "跟房快捷键（此模式不使用）") : sharedFollower ? (locale === "en-US" ? "Manual group join" : "手动加入队伍") : copy.followerShortcut}
            checkingLabel={locale === "en-US" ? "Checking availability..." : "正在检查占用..."}
            checkConflict={shortcut => checkShortcutConflict(shortcut, "join_shortcut")}
            value={draft.join_shortcut}
            captureHint={copy.shortcutCapture}
            recordingLabel={copy.shortcutRecording}
            disabled={editorDisabled}
            invalid={!!validation?.fieldErrors.shortcuts}
            onChange={(join_shortcut) => updateDraft((current) => ({ ...current, join_shortcut }))}
          />
        </div>

          </div>
        </details>
        {(validation?.fieldErrors.shortcuts || validation?.fieldErrors.prefix || validation?.fieldErrors.password || validation?.fieldErrors.sequence) && (
          <p className="room-automation-field-error" role="alert">
            {validation.fieldErrors.shortcuts || validation.fieldErrors.prefix || validation.fieldErrors.password || validation.fieldErrors.sequence}
          </p>
        )}

      </section>
      </div>

      <div className="settings-section-body" hidden={section !== "advanced"}>
      <VisualRecognitionSettings mode="chat" language={language} accounts={eligibleAccounts} primaryAccountId={snapshot?.config.primary_account_id} collaborationMode={snapshot?.config.collaboration_mode} />
      <details open className="spatial-panel room-automation-advanced">
        <summary>
          <span><strong>{copy.advanced}</strong><small>{copy.advancedHelp}</small></span>
          <ChevronDown size={15} aria-hidden="true" />
        </summary>
        <fieldset disabled={editorDisabled}>
          <div className="room-automation-fields">
            <NumberField label={copy.stepDelay} value={draft.flow.step_delay_ms} min={0} max={2000}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(step_delay_ms) => updateDraft((current) => ({ ...current, flow: { ...current.flow, step_delay_ms } }))} />
            <NumberField label={copy.keyHold} value={draft.flow.key_hold_ms ?? DEFAULT_ROOM_FLOW_TIMING.key_hold_ms} min={10} max={250}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(key_hold_ms) => updateDraft((current) => ({ ...current, flow: { ...current.flow, key_hold_ms } }))} />
            <NumberField label={copy.chordHold} value={draft.flow.chord_hold_ms ?? DEFAULT_ROOM_FLOW_TIMING.chord_hold_ms} min={0} max={1000}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(chord_hold_ms) => updateDraft((current) => ({ ...current, flow: { ...current.flow, chord_hold_ms } }))} />
            <NumberField label={copy.formSettle} value={draft.flow.form_settle_ms ?? DEFAULT_ROOM_FLOW_TIMING.form_settle_ms} min={0} max={2000}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(form_settle_ms) => updateDraft((current) => ({ ...current, flow: { ...current.flow, form_settle_ms } }))} />
            <NumberField label={copy.physicalCtrlSettle} value={draft.flow.physical_ctrl_settle_ms ?? DEFAULT_ROOM_FLOW_TIMING.physical_ctrl_settle_ms} min={0} max={2000}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(physical_ctrl_settle_ms) => updateDraft((current) => ({ ...current, flow: { ...current.flow, physical_ctrl_settle_ms } }))} />
            <NumberField label={copy.characterDelay} value={draft.flow.character_delay_ms ?? DEFAULT_ROOM_FLOW_TIMING.character_delay_ms} min={0} max={250}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(character_delay_ms) => updateDraft((current) => ({ ...current, flow: { ...current.flow, character_delay_ms } }))} />
            <NumberField label={copy.syncMessageTimeout} value={draft.flow.sync_message_timeout_ms ?? DEFAULT_ROOM_FLOW_TIMING.sync_message_timeout_ms} min={250} max={5000}
              invalid={!!validation?.fieldErrors.timing}
              onChange={(sync_message_timeout_ms) => updateDraft((current) => ({ ...current, flow: { ...current.flow, sync_message_timeout_ms } }))} />
            <ChoiceField label={copy.backgroundStrategy} value={draft.background_text_strategy}
              options={[{ value: "post_keys", label: copy.postKeys }, { value: "send_keys", label: copy.sendKeys }]}
              disabled={editorDisabled}
              onChange={(value) => updateDraft((current) => ({
                ...current, background_text_strategy: value as RoomAutomationConfig["background_text_strategy"],
              }))} />
          </div>
          <p className="room-automation-consent-copy">{copy.inputTimingHelp}</p>
          <p className="room-automation-consent-copy">{copy.syncMessageTimeoutHelp}</p>
          <div className="mt-3">
            <Button size="sm" variant="ghost"
              onClick={() => updateDraft((current) => ({ ...current, flow: { ...current.flow, ...DEFAULT_ROOM_FLOW_TIMING } }))}
            >{copy.restoreTiming}</Button>
          </div>
          {validation?.fieldErrors.timing && <p className="room-automation-field-error" role="alert">{validation.fieldErrors.timing}</p>}
        </fieldset>
      </details>
      </div>
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
  checkingLabel: string;
  checkConflict: (value: string) => void;
  disabled?: boolean;
  invalid?: boolean;
  onChange: (value: string) => void;
}

function ShortcutField({
  label,
  value,
  captureHint,
  recordingLabel,
  checkingLabel,
  checkConflict,
  disabled,
  invalid,
  onChange,
}: ShortcutFieldProps) {
  const { recordingPos, setRecordingPos } = useShortcutRecorder();
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestRef = useRef(0);
  const errorId = useId();
  useEffect(() => {
    setChecking(false);
    setError(null);
    return () => { requestRef.current += 1; };
  }, [disabled, value]);
  const recording = recordingPos !== null;
  const setRecording = (active: boolean) => setRecordingPos(active ? "room-shortcut" : null);

  const handleKeyDown = async (event: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording || checking || event.repeat) return;
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
    setRecording(false);
    setError(null);
    const request = ++requestRef.current;
    setChecking(true);
    try {
      checkConflict(canonical);
      await validateShortcutAvailability(canonical);
      if (request !== requestRef.current) return;
      checkConflict(canonical);
      onChange(canonical);
    } catch (failure) {
      if (request === requestRef.current) setError(errorMessage(failure));
    } finally {
      if (request === requestRef.current) setChecking(false);
    }
  };

  return (
    <label className="room-automation-field">
      <span>{label}</span>
      <button
        type="button"
        className="settings-input room-automation-shortcut-input"
        data-recording={recording ? "true" : "false"}
        disabled={disabled || checking}
        aria-label={label}
        aria-invalid={!!error || invalid || undefined}
        aria-describedby={error ? errorId : undefined}
        aria-busy={checking}
        onClick={() => setRecording(true)}
        onBlur={() => setRecording(false)}
        onKeyDown={handleKeyDown}
      >
        {checking ? checkingLabel : recording ? recordingLabel : value}
      </button>
      <small className="room-automation-field-hint">{captureHint}</small>
      {error && <small id={errorId} role="alert" className="room-automation-field-error">{error}</small>}
    </label>
  );
}

interface NumberFieldProps {
  label: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  invalid?: boolean;
  onChange: (value: number) => void;
}

function NumberField({ label, value, min, max, step, disabled, invalid, onChange }: NumberFieldProps) {
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
        step={step}
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
