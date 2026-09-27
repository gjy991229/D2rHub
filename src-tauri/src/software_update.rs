use crate::{
    application::task_runtime::{TaskRequest, TaskState},
    downloads::{self, Payload},
    state::SharedState,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

fn verify_installer(path: &std::path::Path, payload: &Payload) -> Result<(), String> {
    downloads::verify(path, payload)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
        };
        let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // These APIs only read the PE version resource; the installer is not run.
        let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(name.as_ptr()), None) };
        if size == 0 || size > 1024 * 1024 {
            return Err("安装包缺少有效的 Windows 版本信息".into());
        }
        let mut buffer = vec![0u8; size as usize];
        unsafe { GetFileVersionInfoW(PCWSTR(name.as_ptr()), 0, size, buffer.as_mut_ptr().cast()) }
            .map_err(|e| e.to_string())?;
        let root = [92u16, 0];
        let mut pointer = std::ptr::null_mut();
        let mut length = 0;
        if !unsafe {
            VerQueryValueW(
                buffer.as_ptr().cast(),
                PCWSTR(root.as_ptr()),
                &mut pointer,
                &mut length,
            )
        }
        .as_bool()
            || pointer.is_null()
            || (length as usize) < std::mem::size_of::<VS_FIXEDFILEINFO>()
        {
            return Err("无法读取安装器实际版本".into());
        }
        let start = buffer.as_ptr() as usize;
        let position = pointer as usize;
        if position < start
            || position
                .checked_add(std::mem::size_of::<VS_FIXEDFILEINFO>())
                .is_none_or(|end| end > start + buffer.len())
        {
            return Err("安装器版本信息损坏".into());
        }
        let info = unsafe { std::ptr::read_unaligned(pointer.cast::<VS_FIXEDFILEINFO>()) };
        let expected = semver::Version::parse(&payload.version).map_err(|e| e.to_string())?;
        let actual = (
            u64::from(info.dwFileVersionMS >> 16),
            u64::from(info.dwFileVersionMS & 65535),
            u64::from(info.dwFileVersionLS >> 16),
        );
        if info.dwSignature != 0xFEEF04BD
            || info.dwFileVersionLS & 65535 != 0
            || actual != (expected.major, expected.minor, expected.patch)
        {
            return Err("安装器内嵌版本与更新清单不一致，拒绝安装".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("此安装包仅支持 Windows".into())
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Installer {
    id: String,
    #[serde(flatten)]
    payload: Payload,
}
#[derive(Clone, Serialize, Deserialize)]
struct Manifest {
    schema: u32,
    kind: String,
    revision: u64,
    product: String,
    platform: String,
    assets: Vec<Installer>,
}
#[derive(Serialize)]
pub struct SoftwareUpdate {
    pub version: String,
    pub available: bool,
    pub downloaded: bool,
    pub path: String,
}
fn validate(m: &Manifest) -> Result<(), String> {
    if m.schema != 2
        || m.kind != "software"
        || m.product != "D2RHub"
        || m.platform != "windows-x86_64"
        || m.assets.len() != 1
        || m.assets[0].id != "hub"
    {
        return Err("软件清单格式或平台不兼容".into());
    }
    let version =
        semver::Version::parse(&m.assets[0].payload.version).map_err(|e| e.to_string())?;
    if !version.pre.is_empty() {
        return Err("正式更新通道不能自动安装预览版".into());
    }
    downloads::validate_payload(&m.assets[0].payload)
}
fn cached(app: &tauri::AppHandle) -> Result<Manifest, String> {
    let path = downloads::cache_root(app)?.join("software-v2.json");
    let m: Manifest = serde_json::from_slice(&fs::read(path).map_err(|_| "请先检查软件更新")?)
        .map_err(|e| e.to_string())?;
    validate(&m)?;
    Ok(m)
}
fn installer_path(app: &tauri::AppHandle, p: &Payload) -> Result<PathBuf, String> {
    Ok(downloads::cache_root(app)?.join("installers").join(format!(
        "D2RHub-{}-{}-setup.exe",
        p.version,
        &p.sha256[..12]
    )))
}
#[tauri::command]
pub async fn check_software_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<SoftwareUpdate, String> {
    let value = downloads::fetch_index(&app, state.inner(), "software", |v| {
        let m: Manifest = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
        validate(&m)
    })
    .await?;
    let m: Manifest = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let p = &m.assets[0].payload;
    let path = installer_path(&app, p)?;
    Ok(SoftwareUpdate {
        version: p.version.clone(),
        available: downloads::version_newer(&p.version, env!("CARGO_PKG_VERSION")),
        downloaded: verify_installer(&path, p).is_ok(),
        path: path.to_string_lossy().into_owned(),
    })
}
#[tauri::command]
pub async fn download_software_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    version: String,
) -> Result<SoftwareUpdate, String> {
    let m = cached(&app)?;
    let p = &m.assets[0].payload;
    if p.version != version || !downloads::version_newer(&version, env!("CARGO_PKG_VERSION")) {
        return Err("更新已变化或不是更高版本，请重新检查".into());
    }
    let path = installer_path(&app, p)?;
    fs::create_dir_all(path.parent().ok_or("下载目录无效")?).map_err(|e| e.to_string())?;
    let task = state
        .tasks()
        .begin(
            TaskRequest::new("software-update-download")
                .for_subject(&version)
                .with_conflict_key("software-update-download")
                .non_retryable()
                .with_initial_status("download", "准备下载安装包"),
        )
        .map_err(|e| e.to_string())?;
    let temp = path.with_extension(format!("{}.part", uuid::Uuid::new_v4()));
    let result = async {
        if verify_installer(&path, p).is_ok() {
            return Ok(());
        }
        downloads::download(&app, state.inner(), p, &temp, &task, 95).await?;
        verify_installer(&temp, p)?;
        if task.cancellation_requested() {
            return Err("下载已取消".into());
        }
        if path.exists() {
            crate::infrastructure::durable_fs::durable_sibling_replace(&temp, &path)
        } else {
            crate::infrastructure::durable_fs::durable_sibling_rename(&temp, &path)
        }
        .map_err(|e| e.to_string())
    }
    .await;
    let _ = fs::remove_file(&temp);
    match result {
        Ok(()) => {
            let _ = task.succeed("下载完成，点击安装");
            Ok(SoftwareUpdate {
                version,
                available: true,
                downloaded: true,
                path: path.to_string_lossy().into_owned(),
            })
        }
        Err(e) => {
            if task.cancellation_requested() {
                let _ = task.cancelled(&e);
            } else {
                let _ = task.fail("software-download-failed", &e);
            }
            Err(e)
        }
    }
}
#[tauri::command]
pub fn launch_downloaded_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedState>,
    version: String,
) -> Result<(), String> {
    let m = cached(&app)?;
    let p = &m.assets[0].payload;
    if p.version != version || !downloads::version_newer(&version, env!("CARGO_PKG_VERSION")) {
        return Err("拒绝运行过期或降级安装包".into());
    }
    if state
        .tasks()
        .snapshots()
        .iter()
        .any(|t| t.state == TaskState::Running)
    {
        return Err("请等待后台任务结束后安装软件更新".into());
    }
    let path = installer_path(&app, p)?;
    verify_installer(&path, p)?;
    std::process::Command::new(path)
        .spawn()
        .map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Set D2RHUB_TEST_INSTALLER and D2RHUB_TEST_INSTALLER_VERSION to a real NSIS build"]
    fn actual_installer_version_must_match_manifest() {
        let path = PathBuf::from(std::env::var("D2RHUB_TEST_INSTALLER").unwrap());
        let mut payload = Payload {
            version: std::env::var("D2RHUB_TEST_INSTALLER_VERSION").unwrap(),
            size: fs::metadata(&path).unwrap().len(),
            sha256: downloads::digest(&path).unwrap(),
            mirrors: vec![],
        };
        verify_installer(&path, &payload).unwrap();
        payload.version = "99.99.99".into();
        assert!(verify_installer(&path, &payload).is_err());
    }
}
