//! Adapter for the external processor executable and its progress/report stream.
//! Trust validation of the returned report belongs to `validation`.
use super::{inventory_resource, AudioModPrepareProgress};
use crate::application::task_runtime::TaskHandle;
use crate::domain::mod_processing::{GeneratorReport, RequestedFeatureGroups};
use crate::infrastructure::managed_process;
use crate::state::SharedState;
use std::path::Path;
use tauri::Emitter;

pub(super) struct GeneratorInvocation<'a> {
    pub(super) processor: &'a Path,
    pub(super) account_id: &'a str,
    pub(super) game_directory: &'a Path,
    pub(super) output_directory: &'a Path,
    pub(super) mod_name: &'a str,
    pub(super) source_directory: Option<&'a Path>,
    pub(super) requested_features: RequestedFeatureGroups,
    pub(super) progress_ceiling: u8,
    pub(super) audio_options: Option<&'a super::rebuild::AudioOptions>,
}

pub(super) fn emit_prepare_progress(
    app: &tauri::AppHandle,
    task: Option<&TaskHandle>,
    account_id: &str,
    phase: &str,
    percent: u8,
    message: impl Into<String>,
) {
    let message = message.into();
    if let Some(task) = task {
        let _ = task.update(percent.min(99), phase, &message);
    }
    let _ = app.emit(
        "audio-mod-prepare-progress",
        AudioModPrepareProgress {
            account_id: account_id.to_string(),
            phase: phase.to_string(),
            percent: percent.min(100),
            message,
        },
    );
}

pub(super) async fn run_audio_mod_generator(
    app: &tauri::AppHandle,
    state: &SharedState,
    task: &TaskHandle,
    invocation: GeneratorInvocation<'_>,
) -> Result<GeneratorReport, String> {
    let GeneratorInvocation {
        processor,
        account_id,
        game_directory,
        output_directory,
        mod_name,
        source_directory,
        requested_features,
        progress_ceiling,
        audio_options,
    } = invocation;
    let command_name = if source_directory.is_some() {
        "augment"
    } else {
        "minimal"
    };
    let default_options = super::rebuild::AudioOptions::default();
    let options = audio_options.unwrap_or(&default_options);
    let mut arguments = vec![
        command_name.to_string(),
        "--game".to_string(),
        game_directory.to_string_lossy().into_owned(),
        "--output".to_string(),
        output_directory.to_string_lossy().into_owned(),
        "--name".to_string(),
        mod_name.to_string(),
        "--areas".to_string(),
        options.areas.clone(),
        "--track".to_string(),
        options.track.clone(),
        "--gain".to_string(),
        options.gain.to_string(),
        "--features".to_string(),
        requested_features.generator_value().to_string(),
    ];
    if let Some(source) = source_directory {
        arguments.push("--source".to_string());
        arguments.push(source.to_string_lossy().into_owned());
    }
    let mut progress_floor = 0;
    if inventory_resource::needed(requested_features.jcy_inventory, source_directory)? {
        emit_prepare_progress(
            app,
            Some(task),
            account_id,
            "inventory_resource",
            1,
            "正在准备 JCY 背包与仓库资源…",
        );
        progress_floor = 12.min(progress_ceiling);
        let preset =
            inventory_resource::ensure(app, state, task, progress_floor, &|percent, message| {
                emit_prepare_progress(
                    app,
                    None, // The download adapter already updated the task center.
                    account_id,
                    "inventory_resource",
                    percent.max(1),
                    message,
                );
            })
            .await
            .map_err(|error| format!("准备 JCY 背包与仓库资源失败：{error}"))?;
        arguments.push("--inventory-preset".to_string());
        arguments.push(preset.to_string_lossy().into_owned());
    }
    arguments.push("--events".to_string());

    let mut command = managed_process::command(processor);
    command
        .args(arguments)
        .env_remove("D2RHUB_INVENTORY_PRESET");
    let mut report: Option<GeneratorReport> = None;
    let mut reported_error = String::new();
    let output = managed_process::run(
        &mut command,
        None,
        || task.cancellation_requested(),
        |line| {
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
                return Ok(());
            };
            match value.get("type").and_then(serde_json::Value::as_str) {
                Some("progress") => {
                    let reported_percent = value
                        .get("percent")
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|percent| u8::try_from(percent).ok())
                        .unwrap_or(0);
                    emit_prepare_progress(
                        app,
                        Some(task),
                        account_id,
                        value
                            .get("phase")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("working"),
                        progress_floor
                            + ((u16::from(reported_percent.min(100))
                                * u16::from(progress_ceiling - progress_floor))
                                / 100) as u8,
                        value
                            .get("message")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("正在准备…"),
                    );
                }
                Some("completed") => {
                    report = Some(
                        serde_json::from_value(value.get("report").cloned().unwrap_or_default())
                            .map_err(|error| format!("生成器返回了无效结果: {error}"))?,
                    );
                }
                Some("error") => {
                    if let Some(message) = value.get("message").and_then(serde_json::Value::as_str)
                    {
                        reported_error = managed_process::bounded_diagnostic(message);
                    }
                }
                _ => {}
            }
            Ok(())
        },
    )
    .await?;
    if output.exit_code != Some(0) {
        let error = if reported_error.trim().is_empty() {
            output.stderr
        } else {
            reported_error
        };
        return Err(if error.trim().is_empty() {
            format!("识别 Mod 生成失败（退出码 {:?}）", output.exit_code)
        } else {
            error.trim().to_string()
        });
    }
    report.ok_or_else(|| "生成器没有返回完成结果".to_string())
}
