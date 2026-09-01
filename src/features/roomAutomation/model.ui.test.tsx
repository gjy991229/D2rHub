import { describe, expect, it } from "vitest";
import { ROOM_AUTOMATION_COPY } from "./copy";
import {
  canonicalizeRoomAutomationShortcut,
  roomAutomationConfigsEqual,
  validateRoomAutomationConfig,
} from "./model";
import type { RoomAutomationConfig } from "./types";

const validConfig: RoomAutomationConfig = {
  enabled: true,
  chat_f13_auto_patch_enabled: false,
  primary_account_id: "one",
  follower_account_ids: ["two"],
  auto_followers_enabled: false,
  auto_followers_delay_secs: 5,
  shortcut: "Ctrl+Alt+R",
  join_shortcut: "Ctrl+Alt+J",
  name_prefix: "run-",
  password: "pw",
  next_sequence: 7,
  sequence_width: 3,
  background_text_strategy: "post_keys",
  strategy_version: 16,
  standard_flow: { step_delay_ms: 80, character_delay_ms: 10 },
  direct_lobby_flow: { step_delay_ms: 60, character_delay_ms: 10 },
  account_flow_bindings: {},
};

describe("room automation configuration validation", () => {
  it("allows an incomplete legacy draft to remain persisted while the module is disabled", () => {
    const result = validateRoomAutomationConfig({
      ...validConfig,
      enabled: false,
      primary_account_id: "",
      follower_account_ids: [],
      shortcut: "",
      join_shortcut: "",
      name_prefix: "",
      auto_followers_delay_secs: 0,
    }, ROOM_AUTOMATION_COPY["en-US"], []);

    expect(result).toEqual({ valid: true, fieldErrors: {} });
  });

  it("rejects values that cannot deserialize into the backend integer contract", () => {
    const fractionalTiming = validateRoomAutomationConfig({
      ...validConfig,
      standard_flow: { ...validConfig.standard_flow, step_delay_ms: 1.5 },
    }, ROOM_AUTOMATION_COPY["en-US"], ["one", "two"]);
    const oversizedSequence = validateRoomAutomationConfig({
      ...validConfig,
      next_sequence: 4_294_967_296,
    }, ROOM_AUTOMATION_COPY["en-US"], ["one", "two"]);

    expect(fractionalTiming.fieldErrors.timing).toBeTruthy();
    expect(oversizedSequence.fieldErrors.sequence).toBeTruthy();
  });

  it("uses the backend shortcut grammar and canonical modifier order", () => {
    expect(canonicalizeRoomAutomationShortcut(" shift + alt + control + f12 ")).toBe("Ctrl+Alt+Shift+F12");
    expect(canonicalizeRoomAutomationShortcut("Ctrl+Ctrl+R")).toBeNull();
    expect(canonicalizeRoomAutomationShortcut("Meta+R")).toBeNull();
    expect(canonicalizeRoomAutomationShortcut("Ctrl+F25")).toBeNull();
    expect(canonicalizeRoomAutomationShortcut(" ctrl + num+ ")).toBe("Ctrl+Num+");

    const invalid = validateRoomAutomationConfig({
      ...validConfig,
      shortcut: "Ctrl+Ctrl+R",
    }, ROOM_AUTOMATION_COPY["en-US"], ["one", "two"]);
    const canonicalConflict = validateRoomAutomationConfig({
      ...validConfig,
      shortcut: "alt + control + r",
      join_shortcut: "Ctrl+Alt+R",
    }, ROOM_AUTOMATION_COPY["en-US"], ["one", "two"]);
    expect(invalid.fieldErrors.shortcuts).toBe(ROOM_AUTOMATION_COPY["en-US"].shortcutInvalid);
    expect(canonicalConflict.fieldErrors.shortcuts).toBe(ROOM_AUTOMATION_COPY["en-US"].shortcutConflict);
  });

  it("treats object key order as the same persisted configuration", () => {
    expect(roomAutomationConfigsEqual(
      { ...validConfig, account_flow_bindings: { two: "direct_lobby", one: "standard" } },
      { ...validConfig, account_flow_bindings: { one: "standard", two: "direct_lobby" } },
    )).toBe(true);
    expect(roomAutomationConfigsEqual(validConfig, { ...validConfig, name_prefix: "other-" })).toBe(false);
  });
});
