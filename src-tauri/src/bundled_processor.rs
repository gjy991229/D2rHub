//! The installed processor is an application resource, never a downloaded asset.
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tauri::Manager;

#[derive(serde::Serialize)]
pub struct ProcessorStatus {
    ready: bool,
    capabilities: Vec<String>,
    blocking_reason: Option<String>,
}

pub(crate) async fn resolve_processor(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let path = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("processor/d2r-audio-mod.exe");
    let bytes = std::fs::read(&path).map_err(|_| "内置 Mod 加工器缺失，请修复或重新安装 D2RHub")?;
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../processor/d2r-audio-mod.json"))
            .map_err(|_| "内置 Mod 加工器构建清单无效，请修复 D2RHub")?;
    if manifest["sha256"].as_str() != Some(format!("{:x}", Sha256::digest(&bytes)).as_str()) {
        return Err("内置 Mod 加工器文件损坏，请修复或重新安装 D2RHub".into());
    }
    Ok(path)
}

#[tauri::command]
pub async fn get_bundled_processor_status(app: tauri::AppHandle) -> ProcessorStatus {
    let error = resolve_processor(&app).await.err();
    ProcessorStatus {
        ready: error.is_none(),
        capabilities: if error.is_none() {
            serde_json::from_str::<serde_json::Value>(include_str!(
                "../processor/d2r-audio-mod.json"
            ))
            .ok()
            .and_then(|value| serde_json::from_value(value["capabilities"].clone()).ok())
            .unwrap_or_default()
        } else {
            Vec::new()
        },
        blocking_reason: error,
    }
}
