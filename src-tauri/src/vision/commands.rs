use super::{
    config::{ConfigSnapshot, VisionConfig},
    Status, Vision,
};
use std::sync::Arc;
use tauri::Manager;

fn resource_root(app: &tauri::AppHandle) -> std::path::PathBuf {
    super::resources::resource_root(&app.state::<crate::state::SharedState>().app_data_dir)
}

// Resource management is available independently of the recognition service.
#[tauri::command(async)]
pub fn vision_get_resource_package(
    app: tauri::AppHandle,
    refresh: Option<bool>,
) -> Result<super::resources::PackageStatus, String> {
    super::resources::package_status(&resource_root(&app), refresh.unwrap_or(false))
}

#[tauri::command]
pub async fn vision_prepare_resource_package(
    app: tauri::AppHandle,
) -> Result<super::resources::PackageStatus, String> {
    let root = resource_root(&app);
    let preferred =
        crate::downloads::cached_preference(&app, &app.state::<crate::state::SharedState>());
    let stop = std::sync::atomic::AtomicBool::new(false);
    super::resources::ensure_preferred(&root, &stop, &preferred, |_| {}).await?;
    // A previous automatic preparation may have exited after a network error.
    // Let the supervisor resume requested recognition without changing settings.
    if let Some(service) = app.try_state::<Arc<Vision>>() {
        let _operation = service.operation.lock();
        let finished = service
            .worker
            .lock()
            .as_ref()
            .is_some_and(|w| w.handle.is_finished());
        if finished {
            service.stop_worker()?;
        }
    }
    super::project(&app);
    super::resources::package_status(&root, false)
}

#[tauri::command(async)]
pub fn vision_open_resource_directory(app: tauri::AppHandle) -> Result<(), String> {
    let root = resource_root(&app);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    crate::resource_install::no_links(&root)?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg(&root)
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(not(target_os = "windows"))]
    std::process::Command::new("open")
        .arg(&root)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn service(app: &tauri::AppHandle) -> Result<Arc<Vision>, String> {
    app.try_state::<Arc<Vision>>()
        .map(|s| s.inner().clone())
        .ok_or_else(|| "当前模式未加载视觉识别".to_string())
}
#[tauri::command(async)]
pub fn vision_get_config(app: tauri::AppHandle) -> Result<ConfigSnapshot, String> {
    super::config::load(&service(&app)?.store)
}
#[tauri::command(async)]
pub fn vision_get_status(app: tauri::AppHandle) -> Result<Status, String> {
    Ok(service(&app)?.status.lock().clone())
}
#[tauri::command(async, rename_all = "camelCase")]
pub fn vision_preview(
    app: tauri::AppHandle,
    account_id: Option<String>,
    roi: super::config::Roi,
    kind: Option<String>,
) -> Result<Preview, String> {
    service(&app)?;
    if !roi.valid() {
        return Err("截图区域无效".into());
    }
    let state = app.state::<crate::state::SharedState>();
    if !state.optional_runtime_ready() {
        return Err("运行服务尚未启用".into());
    }
    let account_id = if kind.as_deref() == Some("chat") {
        let slot = app
            .try_state::<crate::capabilities::room_automation_runtime::RoomAutomationCommandState>()
            .ok_or("自动跟房未加载")?;
        let room = slot.manager()?.get_config().config;
        Some(
            (!room.primary_account_id.is_empty())
                .then_some(room.primary_account_id)
                .ok_or("请先配置主号，聊天截图使用主号窗口")?,
        )
    } else {
        account_id
    };
    let pid = if let Some(account) = account_id.filter(|s| !s.is_empty()) {
        state
            .multi_instance()
            .facade()
            .instance(&account)
            .map(|v| v.pid)
            .ok_or("所选账号没有运行中的游戏")?
    } else {
        crate::rune_audio::monitor::get_rune_audio_status()
            .target_pid
            .ok_or("请先启动声纹识别，确定截图目标")?
    };
    let captured = super::capture::capture(pid, Some(roi))?;
    super::diagnostics::event(
        super::configuration().diagnostic_enabled,
        "capture_preview",
        serde_json::json!(captured.metrics),
    );
    let image = captured.image;
    let lines = super::processing::chat_lines(&image);
    let line_analysis = lines.iter().map(super::processing::chat_analysis).collect();
    let mask = super::processing::binary(&image, super::processing::Mask::Text);
    let tz_image_url = None;
    Ok(Preview {
        image_url: png_url(&image)?,
        mask_url: png_url(&mask)?,
        line_analysis,
        candidate_lines: lines.len(),
        tz_image_url,
    })
}
#[derive(serde::Serialize)]
pub struct Preview {
    pub image_url: String,
    pub mask_url: String,
    pub candidate_lines: usize,
    pub line_analysis: Vec<super::processing::ChatAnalysis>,
    pub tz_image_url: Option<String>,
}
fn png_url(image: &image::RgbImage) -> Result<String, String> {
    use base64::Engine;
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
    ))
}

#[tauri::command(async, rename_all = "camelCase")]
pub fn vision_save_config(
    app: tauri::AppHandle,
    expected_generation: u64,
    config: VisionConfig,
) -> Result<ConfigSnapshot, String> {
    config.validate()?;
    let previous = super::configuration();
    let was_diagnostic = previous.diagnostic_enabled;
    let diagnostics_only = previous.only_diagnostics_changed(&config);
    let service = service(&app)?;
    let _operation = service.operation.lock();
    let saved = service
        .store
        .save_if_generation(expected_generation, config.clone())
        .map_err(|e| e.to_string())?;
    if !diagnostics_only {
        service.stop_worker()?;
    }
    *super::CONFIG.get().ok_or("视觉配置未初始化")?.write() = config.clone();
    if !diagnostics_only {
        super::CONFIG_EPOCH.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }
    super::diagnostics::event(
        was_diagnostic || config.diagnostic_enabled,
        "diagnostic_mode",
        serde_json::json!({"enabled":config.diagnostic_enabled,"generation":saved.generation}),
    );
    super::project(&app);
    let state = app.state::<crate::state::SharedState>();
    if let Some(global) = state.configuration().snapshot() {
        crate::capabilities::apply_configuration(state.inner(), Some(&app), &global);
    }
    Ok(ConfigSnapshot {
        generation: saved.generation,
        config,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_png_round_trips_with_data_url() {
        use base64::Engine;
        let source = image::RgbImage::from_pixel(32, 16, image::Rgb([200, 40, 50]));
        let url = png_url(&source).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(url.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap();
        let restored = image::load_from_memory(&bytes).unwrap().to_rgb8();
        assert_eq!(restored, source);
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        let csp = config["app"]["security"]["csp"].as_str().unwrap();
        assert!(csp
            .split(';')
            .any(|directive| directive.trim().starts_with("img-src ")
                && directive.split_whitespace().any(|token| token == "data:")));
    }
}

#[tauri::command(async)]
pub fn open_drop_screenshots_dir(app: tauri::AppHandle) -> Result<(), String> {
    let root = crate::rune_audio::screenshots::directory(
        &app.state::<crate::state::SharedState>().app_data_dir,
    );
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    open::that(root).map_err(|e| e.to_string())
}
