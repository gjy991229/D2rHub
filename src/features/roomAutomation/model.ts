import type { RoomAutomationConfig, RoomAutomationWorkflowStatus } from "./types";

export interface RoomAutomationValidation {
  valid: boolean;
  fieldErrors: Partial<Record<
    | "primary"
    | "followers"
    | "shortcuts"
    | "prefix"
    | "password"
    | "sequence"
    | "timing",
    string
  >>;
}

const NAMED_SHORTCUT_KEYS: Record<string, string> = {
  space: "Space",
  enter: "Enter",
  tab: "Tab",
  escape: "Escape",
  esc: "Escape",
  backspace: "Backspace",
  delete: "Delete",
  del: "Delete",
  insert: "Insert",
  ins: "Insert",
  home: "Home",
  end: "End",
  pageup: "PageUp",
  pagedown: "PageDown",
  up: "Up",
  arrowup: "Up",
  down: "Down",
  arrowdown: "Down",
  left: "Left",
  arrowleft: "Left",
  right: "Right",
  arrowright: "Right",
  printscreen: "PrintScreen",
  scrolllock: "ScrollLock",
  pause: "Pause",
  numlock: "NumLock",
  "num*": "Num*",
  "num+": "Num+",
  "num-": "Num-",
  "num.": "Num.",
  "num/": "Num/",
};

function canonicalShortcutKey(value: string): string | null {
  if (value.length === 1 && /^[\x21-\x7e]$/.test(value) && value !== "+") {
    return value.toUpperCase();
  }
  const lower = value.toLowerCase();
  if (NAMED_SHORTCUT_KEYS[lower]) return NAMED_SHORTCUT_KEYS[lower];
  const functionKey = /^f(\d{1,2})$/.exec(lower);
  if (functionKey && Number(functionKey[1]) >= 1 && Number(functionKey[1]) <= 24) {
    return `F${Number(functionKey[1])}`;
  }
  const numpadKey = /^num(\d)$/.exec(lower);
  if (numpadKey) return `Num${numpadKey[1]}`;
  const virtualKey = /^vk([0-9a-f]{1,4})$/i.exec(value);
  return virtualKey ? `VK${virtualKey[1].toUpperCase()}` : null;
}

/** Mirrors the backend shortcut grammar and stable Ctrl/Alt/Shift order. */
export function canonicalizeRoomAutomationShortcut(value: string): string | null {
  if (!value.trim()) return null;
  const modifiers = { ctrl: false, alt: false, shift: false };
  let key: string | null = null;

  const rawComponents = value.split("+");
  if (rawComponents[rawComponents.length - 1]?.trim() === ""
    && rawComponents[rawComponents.length - 2]?.trim().toLowerCase() === "num") {
    rawComponents.splice(-2, 2, "Num+");
  }
  for (const rawComponent of rawComponents) {
    const component = rawComponent.trim();
    if (!component) return null;
    switch (component.toLowerCase()) {
      case "ctrl":
      case "control":
        if (modifiers.ctrl) return null;
        modifiers.ctrl = true;
        break;
      case "alt":
        if (modifiers.alt) return null;
        modifiers.alt = true;
        break;
      case "shift":
        if (modifiers.shift) return null;
        modifiers.shift = true;
        break;
      case "win":
      case "meta":
      case "cmd":
      case "command":
        return null;
      default: {
        if (key) return null;
        key = canonicalShortcutKey(component);
        if (!key) return null;
      }
    }
  }

  if (!key) return null;
  return [
    modifiers.ctrl ? "Ctrl" : null,
    modifiers.alt ? "Alt" : null,
    modifiers.shift ? "Shift" : null,
    key,
  ].filter(Boolean).join("+");
}

export function roomAutomationConfigsEqual(
  left: RoomAutomationConfig | null,
  right: RoomAutomationConfig | null,
): boolean {
  if (!left || !right) return left === right;
  const comparable = (config: RoomAutomationConfig) => ({
    ...config,
    account_flow_bindings: Object.fromEntries(
      Object.entries(config.account_flow_bindings).sort(([a], [b]) => a.localeCompare(b)),
    ),
  });
  return JSON.stringify(comparable(left)) === JSON.stringify(comparable(right));
}

export function generatedRoomName(config: RoomAutomationConfig): string {
  return `${config.name_prefix}${String(config.next_sequence).padStart(config.sequence_width, "0")}`;
}

export function validateRoomAutomationConfig(
  config: RoomAutomationConfig,
  copy: {
    primaryRequired: string;
    followersRequired: string;
    duplicateParticipants: string;
    shortcutRequired: string;
    shortcutInvalid: string;
    shortcutConflict: string;
    invalidRoomText: string;
    roomTooLong: string;
    invalidSequence: string;
    invalidTiming: string;
    accountUnavailable: string;
  },
  knownAccountIds?: readonly string[],
): RoomAutomationValidation {
  const fieldErrors: RoomAutomationValidation["fieldErrors"] = {};

  // Match the backend compatibility contract: an optional module may always
  // be switched off while retaining an incomplete legacy draft for later.
  if (!config.enabled) return { valid: true, fieldErrors };

  const followers = config.follower_account_ids.map((id) => id.trim()).filter(Boolean);
  const participantKeys = [config.primary_account_id, ...followers].map((id) => id.trim().toLowerCase());

  if (!config.primary_account_id.trim()) fieldErrors.primary = copy.primaryRequired;
  if (followers.length === 0) fieldErrors.followers = copy.followersRequired;
  if (knownAccountIds) {
    const known = new Set(knownAccountIds.map((id) => id.toLowerCase()));
    if (config.primary_account_id && !known.has(config.primary_account_id.toLowerCase())) {
      fieldErrors.primary = copy.accountUnavailable;
    }
    if (followers.some((id) => !known.has(id.toLowerCase()))) {
      fieldErrors.followers = copy.accountUnavailable;
    }
  }
  if (new Set(participantKeys.filter(Boolean)).size !== participantKeys.filter(Boolean).length) {
    fieldErrors.followers = copy.duplicateParticipants;
  }
  const primaryShortcut = canonicalizeRoomAutomationShortcut(config.shortcut);
  const followerShortcut = canonicalizeRoomAutomationShortcut(config.join_shortcut);
  if (!config.shortcut.trim() || !config.join_shortcut.trim()) {
    fieldErrors.shortcuts = copy.shortcutRequired;
  } else if (!primaryShortcut || !followerShortcut) {
    fieldErrors.shortcuts = copy.shortcutInvalid;
  } else if (primaryShortcut === followerShortcut) {
    fieldErrors.shortcuts = copy.shortcutConflict;
  }

  const asciiRoomText = /^[A-Za-z0-9_-]*$/;
  if (!config.name_prefix || !asciiRoomText.test(config.name_prefix)) fieldErrors.prefix = copy.invalidRoomText;
  if (!asciiRoomText.test(config.password)) fieldErrors.password = copy.invalidRoomText;
  if (generatedRoomName(config).length > 15 || config.password.length > 15) {
    if (generatedRoomName(config).length > 15) fieldErrors.prefix = copy.roomTooLong;
    if (config.password.length > 15) fieldErrors.password = copy.roomTooLong;
  }
  if (!Number.isSafeInteger(config.next_sequence) || config.next_sequence < 0
    || config.next_sequence > 4_294_967_295
    || !Number.isInteger(config.sequence_width) || config.sequence_width < 1 || config.sequence_width > 6) {
    fieldErrors.sequence = copy.invalidSequence;
  }
  const flows = [config.standard_flow, config.direct_lobby_flow];
  if (!Number.isSafeInteger(config.auto_followers_delay_secs)
    || config.auto_followers_delay_secs < 2 || config.auto_followers_delay_secs > 60
    || flows.some((flow) => !Number.isSafeInteger(flow.step_delay_ms)
      || !Number.isSafeInteger(flow.character_delay_ms)
      || flow.step_delay_ms < 0 || flow.step_delay_ms > 2000
      || flow.character_delay_ms < 10 || flow.character_delay_ms > 250)) {
    fieldErrors.timing = copy.invalidTiming;
  }

  return { valid: Object.keys(fieldErrors).length === 0, fieldErrors };
}

export function shouldEnableFollowerAction(status: RoomAutomationWorkflowStatus | null): boolean {
  return !!status && status.phase === "waiting" && status.waiting_mode?.mode === "manual";
}

export function shouldEnablePrimaryAction(status: RoomAutomationWorkflowStatus | null): boolean {
  if (!status) return true;
  if (status.phase === "waiting") return status.waiting_mode?.mode === "manual";
  return status.phase !== "primary" && status.phase !== "followers";
}

export function shouldEnableRetry(status: RoomAutomationWorkflowStatus | null): boolean {
  return !!status
    && (status.phase === "error" || status.phase === "cancelled")
    && status.recovery_action !== null;
}
