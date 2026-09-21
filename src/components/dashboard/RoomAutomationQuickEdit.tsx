import {
  AlertCircle,
  Check,
  ChevronDown,
  DoorOpen,
  LoaderCircle,
  Settings2,
} from "lucide-react";
import { useEffect, useId, useMemo, useRef, useState } from "react";

import { roomAutomationGateway } from "../../features/roomAutomation/gateway";
import type {
  RoomAutomationConfig,
  RoomAutomationConfigSnapshot,
} from "../../features/roomAutomation/types";
import {
  normalizeSettingsLanguage,
  type SettingsLanguage,
} from "../../features/settings/settingsRegistry";
import { showToast } from "../ui/Toast";

interface RoomAutomationQuickEditProps {
  active: boolean;
  language?: string | null;
  onOpenSettings: () => void;
}

interface RoomNamingDraft {
  prefix: string;
  password: string;
  nextSequence: string;
  sequenceWidth: string;
}

interface QuickEditCopy {
  trigger: string;
  triggerTitle: string;
  title: string;
  description: string;
  preview: string;
  prefix: string;
  sequence: string;
  width: string;
  password: string;
  passwordPlaceholder: string;
  fullSettings: string;
  cancel: string;
  save: string;
  saving: string;
  saved: string;
  savedWithWarning: string;
  saveFailed: string;
  invalidRoomText: string;
  roomTooLong: string;
  invalidSequence: string;
}

const QUICK_EDIT_COPY: Record<SettingsLanguage, QuickEditCopy> = {
  "zh-CN": {
    trigger: "跟房配置",
    triggerTitle: "修改下一局跟房配置",
    title: "下一局房间",
    description: "只改房名与密码；参与账号、快捷键等仍在完整设置中管理。",
    preview: "预览",
    prefix: "房名开头",
    sequence: "下一个序号",
    width: "位数",
    password: "房间密码",
    passwordPlaceholder: "无密码",
    fullSettings: "完整设置",
    cancel: "取消",
    save: "应用",
    saving: "应用中",
    saved: "下一局房间已更新",
    savedWithWarning: "房间命名已保存，但运行配置未能立即应用",
    saveFailed: "无法更新下一局房间",
    invalidRoomText: "房名前缀不能为空；支持中文和可粘贴的特殊字符，不支持换行或控制字符。",
    roomTooLong: "生成后的房名和密码不能超过 15 个字符。",
    invalidSequence: "序号应为 0–4294967295 的整数，位数应为 1–6。",
  },
  "en-US": {
    trigger: "Follow Config",
    triggerTitle: "Edit the next room automation settings",
    title: "Next room",
    description: "Edit naming here; manage accounts and shortcuts in full settings.",
    preview: "Preview",
    prefix: "Room prefix",
    sequence: "Next sequence",
    width: "Width",
    password: "Room password",
    passwordPlaceholder: "No password",
    fullSettings: "Full settings",
    cancel: "Cancel",
    save: "Apply",
    saving: "Applying",
    saved: "Next room updated",
    savedWithWarning: "Room naming was saved, but the runtime could not apply it immediately",
    saveFailed: "Could not update the next room",
    invalidRoomText: "A room name prefix is required. Chinese and pasteable special characters are supported; line breaks and control characters are not.",
    roomTooLong: "Generated room names and passwords are limited to 15 UTF-16 characters.",
    invalidSequence: "Sequence must be an integer from 0–4294967295; width must be 1–6.",
  },
};

function namingDraft(config: RoomAutomationConfig): RoomNamingDraft {
  return {
    prefix: config.name_prefix,
    password: config.password,
    nextSequence: String(config.next_sequence),
    sequenceWidth: String(config.sequence_width),
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function validateDraft(draft: RoomNamingDraft, copy: QuickEditCopy): string | null {
  const hasRoomControlCharacters = (value: string) => Array.from(value).some((character) => {
    const code = character.codePointAt(0)!;
    return code < 0x20 || (code >= 0x7f && code <= 0x9f) || code === 0x2028 || code === 0x2029;
  });
  if (!draft.prefix || hasRoomControlCharacters(draft.prefix) || hasRoomControlCharacters(draft.password)) {
    return copy.invalidRoomText;
  }
  const nextSequence = Number(draft.nextSequence);
  const sequenceWidth = Number(draft.sequenceWidth);
  if (!draft.nextSequence.trim() || !draft.sequenceWidth.trim()
    || !Number.isSafeInteger(nextSequence) || nextSequence < 0 || nextSequence > 4_294_967_295
    || !Number.isInteger(sequenceWidth) || sequenceWidth < 1 || sequenceWidth > 6) {
    return copy.invalidSequence;
  }
  const roomName = `${draft.prefix}${String(nextSequence).padStart(sequenceWidth, "0")}`;
  const utf16Length = (value: string) => value.length;
  if (utf16Length(roomName) > 15 || utf16Length(draft.password) > 15) return copy.roomTooLong;
  return null;
}

export function RoomAutomationQuickEdit({
  active,
  language,
  onOpenSettings,
}: RoomAutomationQuickEditProps) {
  const locale = normalizeSettingsLanguage(language);
  const copy = QUICK_EDIT_COPY[locale];
  const dialogTitleId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const openRef = useRef(false);
  const draftDirtyRef = useRef(false);
  const [snapshot, setSnapshot] = useState<RoomAutomationConfigSnapshot | null>(null);
  const [draft, setDraft] = useState<RoomNamingDraft | null>(null);
  const [open, setOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    openRef.current = open;
  }, [open]);

  useEffect(() => {
    if (!active) {
      setSnapshot(null);
      setDraft(null);
      draftDirtyRef.current = false;
      setOpen(false);
      return;
    }
    let disposed = false;
    let stopSync: (() => void) | undefined;
    void roomAutomationGateway.startSync({
      onConfig: (next) => {
        if (disposed) return;
        setSnapshot((current) => !current || next.generation >= current.generation ? next : current);
        // Keep an untouched open popover live when the settings page commits
        // an edit. Once the user starts typing, preserve that local draft until
        // it is explicitly applied or the editor is reopened.
        if (!openRef.current || !draftDirtyRef.current) {
          setDraft(namingDraft(next.config));
          draftDirtyRef.current = false;
        }
        if (!next.config.enabled) setOpen(false);
      },
      onStatus: () => undefined,
    }).then((stop) => {
      if (disposed) stop();
      else stopSync = stop;
    }).catch(() => {
      if (!disposed) setSnapshot(null);
    });
    return () => {
      disposed = true;
      stopSync?.();
    };
  }, [active]);

  useEffect(() => {
    if (!open) return;
    const closeFromPointer = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const closeFromKeyboard = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setOpen(false);
      triggerRef.current?.focus();
    };
    document.addEventListener("pointerdown", closeFromPointer, true);
    document.addEventListener("keydown", closeFromKeyboard);
    return () => {
      document.removeEventListener("pointerdown", closeFromPointer, true);
      document.removeEventListener("keydown", closeFromKeyboard);
    };
  }, [open]);

  const preview = useMemo(() => {
    if (!draft) return "";
    if (!draft.nextSequence.trim() || !draft.sequenceWidth.trim()) return "—";
    const nextSequence = Number(draft.nextSequence);
    const sequenceWidth = Number(draft.sequenceWidth);
    if (!Number.isSafeInteger(nextSequence) || !Number.isInteger(sequenceWidth)) return "—";
    const previewWidth = Math.min(6, Math.max(1, sequenceWidth));
    return `${draft.prefix}${String(nextSequence).padStart(previewWidth, "0")}`;
  }, [draft]);

  if (!active || !snapshot?.config.enabled) return null;

  const updateDraft = (patch: Partial<RoomNamingDraft>) => {
    setDraft((current) => current ? { ...current, ...patch } : current);
    draftDirtyRef.current = true;
    setError(null);
  };

  const openEditor = () => {
    setError(null);
    setDraft(namingDraft(snapshot.config));
    draftDirtyRef.current = false;
    setOpen((current) => !current);
  };

  const save = async () => {
    if (!draft || saving) return;
    const validationError = validateDraft(draft, copy);
    if (validationError) {
      setError(validationError);
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const latest = await roomAutomationGateway.getConfig();
      const candidate: RoomAutomationConfig = {
        ...latest.config,
        name_prefix: draft.prefix,
        password: draft.password,
        next_sequence: Number(draft.nextSequence),
        sequence_width: Number(draft.sequenceWidth),
      };
      const outcome = await roomAutomationGateway.saveConfig(latest.generation, candidate);
      setSnapshot(outcome.snapshot);
      setDraft(namingDraft(outcome.snapshot.config));
      draftDirtyRef.current = false;
      setOpen(false);
      if (outcome.apply_warning) {
        showToast("warning", `${copy.savedWithWarning}: ${outcome.apply_warning}`);
      } else {
        showToast("success", copy.saved);
      }
    } catch (saveError) {
      setError(`${copy.saveFailed}: ${errorMessage(saveError)}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div ref={rootRef} className="room-automation-quick">
      <button
        ref={triggerRef}
        type="button"
        className="control-btn room-automation-quick-trigger"
        data-active={open ? "true" : undefined}
        title={copy.triggerTitle}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={openEditor}
      >
        <span className="room-automation-quick-dot" aria-hidden="true" />
        <DoorOpen size={13} strokeWidth={1.9} aria-hidden="true" />
        <span>{copy.trigger}</span>
        <ChevronDown size={12} strokeWidth={1.9} aria-hidden="true" />
      </button>

      {open && draft && (
        <form
          className="room-automation-quick-popover"
          role="dialog"
          aria-modal="false"
          aria-labelledby={dialogTitleId}
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          <header className="room-automation-quick-header">
            <div>
              <h2 id={dialogTitleId}>{copy.title}</h2>
              <p>{copy.description}</p>
            </div>
            <div className="room-automation-quick-preview">
              <span>{copy.preview}</span>
              <strong data-i18n-skip>{preview}</strong>
            </div>
          </header>

          <div className="room-automation-quick-fields">
            <label>
              <span>{copy.prefix}</span>
              <input
                autoFocus
                type="text"
                className="line-input"
                maxLength={15}
                value={draft.prefix}
                onChange={(event) => updateDraft({ prefix: event.target.value })}
              />
            </label>
            <label>
              <span>{copy.sequence}</span>
              <input
                type="number"
                className="line-input"
                min={0}
                max={4_294_967_295}
                step={1}
                value={draft.nextSequence}
                onChange={(event) => updateDraft({ nextSequence: event.target.value })}
              />
            </label>
            <label>
              <span>{copy.width}</span>
              <input
                type="number"
                className="line-input"
                min={1}
                max={6}
                step={1}
                value={draft.sequenceWidth}
                onChange={(event) => updateDraft({ sequenceWidth: event.target.value })}
              />
            </label>
            <label className="room-automation-quick-password">
              <span>{copy.password}</span>
              <input
                type="password"
                className="line-input"
                maxLength={15}
                placeholder={copy.passwordPlaceholder}
                value={draft.password}
                onChange={(event) => updateDraft({ password: event.target.value })}
              />
            </label>
          </div>

          {error && (
            <p className="room-automation-quick-error" role="alert">
              <AlertCircle size={13} aria-hidden="true" />
              <span>{error}</span>
            </p>
          )}

          <footer className="room-automation-quick-footer">
            <button
              type="button"
              className="room-automation-quick-settings"
              onClick={() => {
                setOpen(false);
                onOpenSettings();
              }}
            >
              <Settings2 size={12} strokeWidth={1.9} aria-hidden="true" />
              {copy.fullSettings}
            </button>
            <div>
              <button type="button" className="control-btn" disabled={saving} onClick={() => setOpen(false)}>
                {copy.cancel}
              </button>
              <button type="submit" className="primary-cta" disabled={saving}>
                {saving
                  ? <LoaderCircle className="animate-spin" size={12} aria-hidden="true" />
                  : <Check size={12} strokeWidth={2} aria-hidden="true" />}
                {saving ? copy.saving : copy.save}
              </button>
            </div>
          </footer>
        </form>
      )}
    </div>
  );
}
