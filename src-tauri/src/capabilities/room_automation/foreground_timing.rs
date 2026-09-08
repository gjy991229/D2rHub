//! Shared foreground response timings for creating and joining rooms.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ForegroundTiming {
    pub step_interval_ms: u64,
    pub window_focus_ms: u64,
    pub mouse_hold_ms: u64,
    #[serde(alias = "join_form_response_ms")]
    pub form_response_ms: u64,
    #[serde(alias = "join_focus_response_ms")]
    pub focus_response_ms: u64,
    #[serde(alias = "join_select_response_ms")]
    pub select_response_ms: u64,
    #[serde(alias = "join_paste_response_ms")]
    pub paste_response_ms: u64,
    #[serde(alias = "join_key_hold_ms")]
    pub chord_hold_ms: u64,
    #[serde(alias = "key_hold_ms")]
    pub submit_hold_ms: u64,
}

impl Default for ForegroundTiming {
    fn default() -> Self {
        Self {
            step_interval_ms: 1,
            window_focus_ms: 100,
            mouse_hold_ms: 20,
            // Responses start after the input is released, rather than
            // being consumed by mouse movement or the key-down hold.
            form_response_ms: 150,
            focus_response_ms: 100,
            select_response_ms: 100,
            paste_response_ms: 100,
            chord_hold_ms: 50,
            submit_hold_ms: 20,
        }
    }
}

impl ForegroundTiming {
    pub(super) fn normalize(&mut self) {
        self.step_interval_ms = self.step_interval_ms.min(2_000);
        for value in [
            &mut self.window_focus_ms,
            &mut self.mouse_hold_ms,
            &mut self.form_response_ms,
            &mut self.focus_response_ms,
            &mut self.select_response_ms,
            &mut self.paste_response_ms,
            &mut self.chord_hold_ms,
            &mut self.submit_hold_ms,
        ] {
            *value = (*value).clamp(1, 2_000);
        }
    }

    pub(super) fn valid(&self) -> bool {
        self.step_interval_ms <= 2_000
            && [
                self.window_focus_ms,
                self.mouse_hold_ms,
                self.form_response_ms,
                self.focus_response_ms,
                self.select_response_ms,
                self.paste_response_ms,
                self.chord_hold_ms,
                self.submit_hold_ms,
            ]
            .into_iter()
            .all(|value| (1..=2_000).contains(&value))
    }
}
