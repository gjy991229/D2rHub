//! Hostility handling consumes the shared process-audio stream, independently of tracking.
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub(super) struct TriggerGate {
    last: Option<Instant>,
}
impl TriggerGate {
    pub(super) fn accept(&mut self, now: Instant) -> bool {
        if self
            .last
            .is_some_and(|last| now.duration_since(last) < Duration::from_secs(15))
        {
            return false;
        }
        self.last = Some(now);
        true
    }
}

pub(super) fn trigger(app: &tauri::AppHandle, account_id: &str, source_id: &str, pid: u32) {
    let result = dispatch(app, account_id, source_id, pid);
    if let Err(error) = result {
        crate::logger::log_msg("WARN", "HostileRestart", &error);
        let _ = app.emit(
            "hostile-restart-status",
            serde_json::json!({"pid":pid,"error":error}),
        );
    }
}

fn dispatch(
    app: &tauri::AppHandle,
    account_id: &str,
    source_id: &str,
    pid: u32,
) -> Result<(), String> {
    let state = app.state::<crate::state::SharedState>();
    let config = state.configuration().snapshot().ok_or("设置尚未加载")?;
    if !config.hostile_auto_restart_enabled {
        return Ok(());
    }
    let mut targets = Vec::new();
    if let Some(external) = &config.rune_audio_external_target {
        if super::external::source_id(&config, &external.edition) != source_id
            || !super::external::current_session_matches(&config, external, Some(pid))
        {
            return Err("监听窗口已变化，取消敌对重开".into());
        }
        let game = super::external::installation_directory(&config, &external.edition)?;
        let args = crate::audio_mod::arguments_with_audio_mod("", &external.mod_name)?;
        crate::audio_mod::validate_runtime_feature_directory(&game, &args, "hostile_auto_restart")?;
        targets.push((String::new(), pid));
    } else {
        if config.rune_audio_target_account != account_id {
            return Err("监听窗口已变化，取消敌对重开".into());
        }
        let instance = state
            .multi_instance()
            .instances()
            .get(account_id)
            .ok_or("监听窗口已退出")?;
        if instance.pid != pid {
            return Err("监听进程已变化".into());
        }
        let args = instance
            .launch
            .as_ref()
            .map(|s| s.mod_args.as_str())
            .ok_or("无法确认监听窗口实际加载的 Mod，请通过 Hub 启动")?;
        crate::audio_mod::validate_runtime_feature(pid, args, "hostile_auto_restart")?;
        targets.push((account_id.to_string(), pid));
    }
    let targets = execution_targets(
        targets.remove(0),
        config.hostile_auto_restart_all_windows,
        state
            .multi_instance()
            .instances()
            .list()
            .into_iter()
            .map(|i| (i.account_id, i.pid)),
    );
    std::thread::scope(|scope| {
        for (id, target_pid) in targets {
            let state = &state;
            let config = &config;
            scope.spawn(move || {
                let operation = (|| {
                    if !id.is_empty() {
                        let instance = state
                            .multi_instance()
                            .instances()
                            .get(&id)
                            .ok_or("目标窗口已退出")?;
                        if instance.pid != target_pid {
                            return Err("目标进程已变化".to_string());
                        }
                        let args = instance
                            .launch
                            .as_ref()
                            .map(|s| s.mod_args.as_str())
                            .ok_or("无法确认目标窗口实际加载的 Mod，请通过 Hub 启动")?;
                        crate::audio_mod::validate_runtime_feature(
                            target_pid,
                            args,
                            "esc_next_game",
                        )?;
                    }
                    crate::capabilities::room_automation_windows::hostile_escape_burst(
                        target_pid,
                        || {
                            state.configuration().snapshot().is_some_and(|c| {
                                c.hostile_auto_restart_enabled
                                    && c.hostile_auto_restart_all_windows
                                        == config.hostile_auto_restart_all_windows
                                    && c.rune_audio_target_account
                                        == config.rune_audio_target_account
                                    && c.rune_audio_external_target
                                        == config.rune_audio_external_target
                            }) && super::monitor::get_rune_audio_status().running
                        },
                    )
                })();
                let error = operation.err();
                crate::logger::log_msg(
                    if error.is_some() { "WARN" } else { "INFO" },
                    "HostileRestart",
                    &format!(
                        "PID {target_pid}: {}",
                        error
                            .as_deref()
                            .unwrap_or("已发送四次 Esc；游戏内结果待确认")
                    ),
                );
                let _ = app.emit(
                    "hostile-restart-status",
                    serde_json::json!({"pid":target_pid,"error":error}),
                );
            });
        }
    });
    Ok(())
}

fn execution_targets(
    listening: (String, u32),
    all: bool,
    managed: impl Iterator<Item = (String, u32)>,
) -> Vec<(String, u32)> {
    let mut targets = vec![listening];
    if all {
        for target in managed {
            if !targets.iter().any(|(_, pid)| *pid == target.1) {
                targets.push(target);
            }
        }
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn action_scope_does_not_expand_listening_and_deduplicates_pids() {
        let managed = vec![
            ("one".into(), 10),
            ("two".into(), 20),
            ("duplicate".into(), 20),
        ];
        assert_eq!(
            execution_targets(("one".into(), 10), false, managed.clone().into_iter()),
            vec![("one".into(), 10)]
        );
        assert_eq!(
            execution_targets(("one".into(), 10), true, managed.into_iter()),
            vec![("one".into(), 10), ("two".into(), 20)]
        );
    }
    #[test]
    fn suppresses_repeated_packets_without_sliding_cooldown() {
        let mut gate = TriggerGate::default();
        let now = Instant::now();
        assert!(gate.accept(now));
        assert!(!gate.accept(now + Duration::from_millis(150)));
        assert!(!gate.accept(now + Duration::from_secs(14)));
        assert!(gate.accept(now + Duration::from_secs(15)));
    }
}
