import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GlobalConfig } from "../../store/types";
import { usePetCompanion } from "./usePetCompanion";

const mocks = vi.hoisted(() => ({
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  disconnect: vi.fn(),
  settle: vi.fn(async () => ({ rewards: [] })),
}));

vi.mock("../../platform/tauri", () => ({
  listenEvent: vi.fn(async (name: string, listener: (event: { payload: unknown }) => void) => {
    mocks.listeners.set(name, listener);
    return () => mocks.listeners.delete(name);
  }),
}));
vi.mock("./wardrobeStore", () => ({
  connectWardrobe: () => mocks.disconnect,
  settlePetActivity: mocks.settle,
  usePetWardrobe: Object.assign(
    (select: (state: { snapshot: null }) => unknown) => select({ snapshot: null }),
    { setState: vi.fn() },
  ),
}));

const config = { enable_bongo_cat: true, bongo_cat_chatterbox: false } as GlobalConfig;
let now = 100;
const input = (payload: string) => {
  now += 60;
  act(() => mocks.listeners.get("global-input-event")?.({ payload }));
};

beforeEach(() => {
  vi.useFakeTimers();
  now = 100;
  vi.spyOn(performance, "now").mockImplementation(() => now);
});
afterEach(async () => {
  cleanup();
  await Promise.resolve();
  mocks.listeners.clear();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("desktop companion input", () => {
  it("does not announce an unconfirmed login as a failure, but still announces real failures", () => {
    const { result } = renderHook(() => usePetCompanion({ ...config, bongo_cat_chatterbox: true }));
    act(() => mocks.listeners.get("launch-ended")?.({
      payload: { success: false, login_unconfirmed: true },
    }));
    expect(result.current.activeDrops).toHaveLength(0);
    act(() => mocks.listeners.get("launch-ended")?.({
      payload: { success: false, login_unconfirmed: false },
    }));
    expect(result.current.activeDrops).toHaveLength(1);
    expect(result.current.activeDrops[0].color).toBe("#b54040");
  });

  it("lets keyboard and either mouse button choose both hands from the same random source", () => {
    const random = vi.spyOn(Math, "random");
    const { result } = renderHook(() => usePetCompanion(config));
    for (const source of ["Keyboard", "MouseLeft", "MouseRight"]) {
      random.mockReturnValue(0.1);
      input(source);
      expect(result.current.frame).toBe("left");
      random.mockReturnValue(0.9);
      input(source);
      expect(result.current.frame).toBe("right");
    }
    expect(result.current.clickCount).toBe(6);
  });

  it("coalesces bursts, returns to idle and releases listeners and timers on unmount", async () => {
    const random = vi.spyOn(Math, "random").mockReturnValue(0.1);
    const { result, unmount } = renderHook(() => usePetCompanion(config));
    input("MouseRight");
    now += 1;
    act(() => mocks.listeners.get("global-input-event")?.({ payload: "Keyboard" }));
    expect(random).toHaveBeenCalledTimes(1);
    expect(result.current.frame).toBe("left");
    now += 120;
    act(() => vi.advanceTimersByTime(120));
    expect(result.current.frame).toBe("up");
    expect(result.current.clickCount).toBe(2);
    unmount();
    await Promise.resolve();
    expect(vi.getTimerCount()).toBe(0);
    expect(mocks.listeners.size).toBe(0);
    expect(mocks.disconnect).toHaveBeenCalled();
    expect(mocks.settle).not.toHaveBeenCalled();
  });

  it("ignores unknown events and stops accepting input when the pet is disabled", () => {
    const random = vi.spyOn(Math, "random").mockReturnValue(0.1);
    const { result, rerender } = renderHook(({ enabled }) => usePetCompanion({ ...config, enable_bongo_cat: enabled }), {
      initialProps: { enabled: false },
    });
    input("Keyboard");
    input("MouseLeft");
    input("MouseRight");
    expect(result.current.clickCount).toBe(0);
    expect(random).not.toHaveBeenCalled();
    rerender({ enabled: true });
    input("MouseMove");
    expect(result.current.clickCount).toBe(0);
    input("Keyboard");
    expect(result.current.clickCount).toBe(1);
  });
});
