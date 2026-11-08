import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { KeyboardEvent } from "react";
import type { GlobalConfig } from "../../store/types";
import { useShortcutBindingController } from "./useShortcutBindingController";

vi.mock("../../hooks/useShortcutRecorder", () => ({
  useShortcutRecorder: () => ({ recordingPos: null, setRecordingPos: vi.fn() }),
  parseShortcutFromKeyEvent: () => "Ctrl+Alt+1",
  validateShortcutAvailability: vi.fn(async () => {}),
}));
vi.mock("../../components/ui/Toast", () => ({ showToast: vi.fn() }));

function event() {
  return { key: "1", repeat: false, preventDefault: vi.fn(), stopPropagation: vi.fn(),
    currentTarget: { blur: vi.fn() } } as unknown as KeyboardEvent<HTMLInputElement>;
}
function setup(occupied = false) {
  const config = { app_language: "zh-CN", shortcut_bindings_json: occupied ? '{"1":"Ctrl+Alt+1"}' : '{}',
    launch_scheme_shortcuts: [{ group_id: "saved", shortcut: "" }] } as GlobalConfig;
  const updateConfig = vi.fn((update: (config: GlobalConfig) => void) => update(config));
  const hook = renderHook(() => useShortcutBindingController({ open: true, activeTab: "shortcuts", config, updateConfig }));
  return { ...hook, config, updateConfig };
}
describe("launch scheme shortcut recording", () => {
  it("records and clears the shortcut while preserving its selected scheme", async () => {
    const { result, config } = setup();
    await act(() => result.current.handleShortcutKeyDown(event(), "scheme:0"));
    expect(config.launch_scheme_shortcuts?.[0]).toEqual({ group_id: "saved", shortcut: "Ctrl+Alt+1" });
    act(() => result.current.handleClearShortcut("scheme:0"));
    expect(config.launch_scheme_shortcuts?.[0]).toEqual({ group_id: "saved", shortcut: "" });
  });
  it("rejects a shortcut already assigned to an account", async () => {
    const { result, config, updateConfig } = setup(true);
    await act(() => result.current.handleShortcutKeyDown(event(), "scheme:0"));
    expect(result.current.shortcutErrors["scheme:0"]).toContain("已用于");
    expect(config.launch_scheme_shortcuts?.[0].shortcut).toBe("");
    expect(updateConfig).not.toHaveBeenCalled();
  });
});
