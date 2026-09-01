import { describe, expect, it } from "vitest";
import { hasAudioTelemetry } from "./audioModuleModel";

describe("audio feature group truth", () => {
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
