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
    });
    expect(audioModFeatureDefaultsForPurpose("room-tools")).toEqual({
      includeAudioTelemetry: false,
      includeRoomTools: true,
    });
    expect(audioModFeatureDefaultsForPurpose("manage")).toEqual({
      includeAudioTelemetry: false,
      includeRoomTools: false,
    });
  });

  it("maps every selection to the camelCase Tauri command arguments", () => {
    expect(audioModFeatureInvokeOptions({
      includeAudioTelemetry: false,
      includeRoomTools: true,
    })).toEqual({
      includeAudioTelemetry: false,
      includeRoomTools: true,
    });
  });

  it("requires one feature for a new Mod and detects additive management", () => {
    expect(hasSelectedAudioModFeature({
      includeAudioTelemetry: false,
      includeRoomTools: false,
    })).toBe(false);
    expect(selectedAudioModFeatureAddsCapability({
      includeAudioTelemetry: true,
      includeRoomTools: true,
    }, ["audio_telemetry"])).toBe(true);
    expect(selectedAudioModFeatureAddsCapability({
      includeAudioTelemetry: true,
      includeRoomTools: true,
    }, ["audio_telemetry", "in_game_room_tools"])).toBe(false);
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
