import { describe, expect, it } from "vitest";
import {
  FRAMERATE_CAP_KEY,
  LEGACY_FRAMERATE_TARGET_KEY,
  readFramerateCap,
  writeFramerateCap,
} from "./gameSettings";

describe("game settings FPS mapping", () => {
  it("reads Framerate Cap even when a conflicting legacy Target exists", () => {
    expect(readFramerateCap({
      [FRAMERATE_CAP_KEY]: 144,
      [LEGACY_FRAMERATE_TARGET_KEY]: 30,
    }, 60)).toBe(144);
  });

  it("writes Cap without inventing a Target key", () => {
    expect(writeFramerateCap({ VSync: true }, 120)).toEqual({
      VSync: true,
      [FRAMERATE_CAP_KEY]: 120,
    });
  });

  it("synchronizes a Target key only when the source already contains one", () => {
    expect(writeFramerateCap({ [LEGACY_FRAMERATE_TARGET_KEY]: 30 }, 240)).toEqual({
      [FRAMERATE_CAP_KEY]: 240,
      [LEGACY_FRAMERATE_TARGET_KEY]: 240,
    });
  });
});
