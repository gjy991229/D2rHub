import type { ForegroundTiming } from "./types";

export const DEFAULT_FOREGROUND_TIMING: Readonly<ForegroundTiming> = {
  step_interval_ms: 1,
  window_focus_ms: 100,
  mouse_hold_ms: 20,
  form_response_ms: 150,
  focus_response_ms: 100,
  select_response_ms: 100,
  paste_response_ms: 100,
  chord_hold_ms: 50,
  submit_hold_ms: 20,
};

export function foregroundTimingWithDefaults(timing?: ForegroundTiming): ForegroundTiming {
  return { ...DEFAULT_FOREGROUND_TIMING, ...timing };
}

export const FOREGROUND_TIMING_FIELDS = [
  ["step_interval_ms", "timingStepInterval"],
  ["window_focus_ms", "timingWindowFocus"],
  ["mouse_hold_ms", "timingMouseHold"],
  ["form_response_ms", "timingFormResponse"],
  ["focus_response_ms", "timingFocusResponse"],
  ["select_response_ms", "timingSelectResponse"],
  ["paste_response_ms", "timingPasteResponse"],
  ["chord_hold_ms", "timingChordHold"],
  ["submit_hold_ms", "timingSubmitHold"],
] as const satisfies readonly (readonly [keyof ForegroundTiming, string])[];
