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

#[derive(serde::Deserialize)]
struct ProcessorManifest {
    sha256: String,
    #[serde(default)]
    capabilities: Vec<String>,
}

async fn verified_processor(
    app: &tauri::AppHandle,
) -> Result<(PathBuf, ProcessorManifest), String> {
    let path = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("processor/d2r-audio-mod.exe");
    tokio::task::spawn_blocking(move || {
        let manifest: ProcessorManifest =
            serde_json::from_str(include_str!("../processor/d2r-audio-mod.json"))
                .map_err(|_| "内置 Mod 加工器构建清单无效，请修复 D2RHub")?;
        let mut file = std::fs::File::open(&path)
            .map_err(|_| "内置 Mod 加工器缺失，请修复或重新安装 D2RHub")?;
        let mut hash = Sha256::new();
        std::io::copy(&mut file, &mut hash)
            .map_err(|error| format!("读取内置 Mod 加工器失败：{error}"))?;
        if manifest.sha256 != format!("{:x}", hash.finalize()) {
            return Err("内置 Mod 加工器文件损坏，请修复或重新安装 D2RHub".into());
        }
        Ok((path, manifest))
    })
    .await
    .map_err(|error| error.to_string())?
}

pub(crate) async fn resolve_processor(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    verified_processor(app).await.map(|(path, _)| path)
}

#[tauri::command]
pub async fn get_bundled_processor_status(app: tauri::AppHandle) -> ProcessorStatus {
    match verified_processor(&app).await {
        Ok((_, manifest)) => ProcessorStatus {
            ready: true,
            capabilities: manifest.capabilities,
            blocking_reason: None,
        },
        Err(error) => ProcessorStatus {
            ready: false,
            capabilities: Vec::new(),
            blocking_reason: Some(error),
        },
    }
}
