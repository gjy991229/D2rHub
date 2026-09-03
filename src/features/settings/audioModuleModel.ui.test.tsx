import { describe, expect, it } from "vitest";
import {
  audioModFeatureDefaultsForPurpose,
  audioModFeatureInvokeOptions,
  hasAudioTelemetry,
  hasSelectedAudioModFeature,
  selectedAudioModFeatureAddsCapability,
} from "./audioModuleModel";

describe("audio feature group truth", () => {
  it("chooses feature defaults from the processing entry point", () => {
    expect(audioModFeatureDefaultsForPurpose("recognition")).toEqual({
      includeAudioTelemetry: true,
      includeRoomTools: true,
      includeAutoExitOnDeath: false,
    });
    expect(audioModFeatureDefaultsForPurpose("room-tools")).toEqual({
      includeAudioTelemetry: false,
      includeRoomTools: true,
      includeAutoExitOnDeath: false,
    });
    expect(audioModFeatureDefaultsForPurpose("manage")).toEqual({
      includeAudioTelemetry: false,
      includeRoomTools: false,
      includeAutoExitOnDeath: false,
    });
  });

  it("maps every selection to the camelCase Tauri command arguments", () => {
    expect(audioModFeatureInvokeOptions({
      includeAudioTelemetry: false,
      includeRoomTools: true,
      includeAutoExitOnDeath: true,
    })).toEqual({
      includeAudioTelemetry: false,
      includeRoomTools: true,
      includeAutoExitOnDeath: true,
    });
  });

  it("requires one feature and detects only additive modules", () => {
    expect(hasSelectedAudioModFeature({
      includeAudioTelemetry: false,
      includeRoomTools: false,
      includeAutoExitOnDeath: false,
    })).toBe(false);
    expect(selectedAudioModFeatureAddsCapability({
      includeAudioTelemetry: true,
      includeRoomTools: true,
      includeAutoExitOnDeath: false,
    }, ["audio_telemetry"])).toBe(true);
    expect(selectedAudioModFeatureAddsCapability({
      includeAudioTelemetry: true,
      includeRoomTools: true,
      includeAutoExitOnDeath: false,
    }, ["audio_telemetry", "in_game_room_tools"])).toBe(false);
    expect(selectedAudioModFeatureAddsCapability({
      includeAudioTelemetry: true,
      includeRoomTools: true,
      includeAutoExitOnDeath: true,
    }, ["audio_telemetry", "in_game_room_tools"])).toBe(true);
    expect(selectedAudioModFeatureAddsCapability({
      includeAudioTelemetry: true,
      includeRoomTools: true,
      includeAutoExitOnDeath: true,
    }, ["audio_telemetry", "in_game_room_tools", "auto_exit_on_death"])).toBe(false);
  });

  it("uses the final persisted group IDs returned by setup state", () => {
    expect(hasAudioTelemetry(["in_game_room_tools"])).toBe(false);
    expect(hasAudioTelemetry(["audio_telemetry", "in_game_room_tools"])).toBe(true);
  });

  it("uses verified generator groups instead of assuming preparation added audio", () => {
    expect(hasAudioTelemetry([{
      id: "in_game_room_tools",
      recipe_version: 1,
      fingerprint: "room-fingerprint",
      reused_from_source: false,
    }])).toBe(false);
    expect(hasAudioTelemetry([{
      id: "audio_telemetry",
      recipe_version: 1,
      fingerprint: "audio-fingerprint",
      reused_from_source: true,
    }])).toBe(true);
  });
});
