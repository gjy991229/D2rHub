//! Only this small manifest is embedded. Models, dictionary metadata and ORT are downloaded.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const ONLINE: &str =
    "https://raw.githubusercontent.com/gjy991229/D2rHub/main/resources/vision-resources-v1.json";
static PREPARATION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static PACKAGE: parking_lot::Mutex<Option<PackageStatus>> = parking_lot::Mutex::new(None);

pub(super) fn resource_root(app_data_dir: &str) -> PathBuf {
    // Explorer needs native separators even though filesystem APIs accept mixed ones.
    Path::new(app_data_dir).join("resources").join("vision")
}

#[derive(Clone, Serialize)]
pub struct PackageStatus {
    pub ready: bool,
    pub preparing: bool,
    pub progress: Option<u8>,
    pub message: String,
    pub size: u64,
    pub path: PathBuf,
}

fn local_manifest(root: &Path) -> Result<Manifest, String> {
    let mut manifest: Manifest =
        serde_json::from_str(include_str!("../../../resources/vision-resources-v1.json"))
            .map_err(err)?;
    validate(&manifest)?;
    if let Ok(bytes) = fs::read(root.join("manifest.json")) {
        if let Ok(cached) = serde_json::from_slice(&bytes) {
            adopt_newer_compatible_manifest(&mut manifest, cached);
        }
    }
    Ok(manifest)
}

pub fn package_status(root: &Path, refresh: bool) -> Result<PackageStatus, String> {
    let mut status = PACKAGE.lock();
    if let Some(current) = status
        .as_ref()
        .filter(|s| s.path == root && (!refresh || s.preparing))
    {
        return Ok(current.clone());
    }
    let m = local_manifest(root)?;
    let ready = installed_valid(root, &m);
    let current = PackageStatus {
        ready,
        preparing: false,
        progress: ready.then_some(100),
        message: if ready {
            "资源包已就绪"
        } else {
            "尚未下载；仅拼房跟随需要，可手动下载或应用跟随模式时确认下载"
        }
        .into(),
        size: m.model.size + m.runtime.size,
        path: root.into(),
    };
    *status = Some(current.clone());
    Ok(current)
}

pub(super) fn installed(root: &Path) -> Result<Installed, String> {
    let manifest = local_manifest(root)?;
    if !installed_valid(root, &manifest) {
        return Err(
            "拼房跟随需要视觉资源，请在 Mod 管理中手动下载，或重新应用跟随模式并确认下载".into(),
        );
    }
    Ok(Installed {
        model: root.join(format!("{}.onnx", manifest.model.sha256)),
        runtime: root.join(&manifest.runtime.sha256).join("onnxruntime.dll"),
    })
}

fn report(progress: &impl Fn(String), message: String, percent: Option<u8>) {
    if let Some(status) = PACKAGE.lock().as_mut() {
        status.message = message.clone();
        status.progress = percent;
    }
    progress(message);
}

struct Preparing(bool);
impl Drop for Preparing {
    fn drop(&mut self) {
        if !self.0 {
            if let Some(status) = PACKAGE.lock().as_mut() {
                status.preparing = false;
                status.ready = false;
                status.message = "资源准备已中断，可重试".into();
            }
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Asset {
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default)]
    pub mirrors: Vec<crate::downloads::Mirror>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub revision: u64,
    pub engine_abi: String,
    pub model: Asset,
    pub runtime: Asset,
}
pub struct Installed {
    pub model: PathBuf,
    pub runtime: PathBuf,
}
struct PartialFile(PathBuf);
impl Drop for PartialFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn validate(m: &Manifest) -> Result<(), String> {
    let runtime_version = semver::Version::parse(&m.runtime.version).map_err(err)?;
    if m.schema != 1
        || m.engine_abi != "ort-1.22"
        || runtime_version.major != 1
        || runtime_version.minor != 22
        || !runtime_version.pre.is_empty()
    {
        return Err("视觉资源与本软件推理接口不兼容".into());
    }
    for a in [&m.model, &m.runtime] {
        let u = reqwest::Url::parse(&a.url).map_err(err)?;
        let allowed = (u.host_str() == Some("www.modelscope.cn")
            && u.path().starts_with("/models/RapidAI/RapidOCR/resolve/"))
            || (u.host_str() == Some("github.com")
                && (u
                    .path()
                    .starts_with("/microsoft/onnxruntime/releases/download/")
                    || u.path().starts_with("/gjy991229/D2rHub/releases/download/")));
        if !allowed
            || u.scheme() != "https"
            || !u.username().is_empty()
            || u.password().is_some()
            || u.query().is_some()
            || u.fragment().is_some()
            || u.port().is_some()
            || a.size == 0
            || a.size > 256 * 1024 * 1024
            || a.sha256.len() != 64
            || !a.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("视觉资源下载来源或摘要无效".into());
        }
        if a.mirrors.len() > 1 {
            return Err("视觉资源镜像数量无效".into());
        }
        for mirror in &a.mirrors {
            // Keep upstream URLs for older clients; only the owned Gitee mirror
            // can be added to this backwards-compatible manifest.
            crate::downloads::validate_payload(&crate::downloads::Payload {
                version: a.version.clone(),
                size: a.size,
                sha256: a.sha256.clone(),
                mirrors: vec![mirror.clone()],
            })?;
            if mirror.platform != "gitee" {
                return Err("视觉资源镜像必须来自 Gitee 资源库".into());
            }
        }
    }
    Ok(())
}

fn adopt_newer_compatible_manifest(current: &mut Manifest, candidate: Manifest) {
    if candidate.revision > current.revision && validate(&candidate).is_ok() {
        *current = candidate;
    }
}
fn valid_file(p: &Path, a: &Asset) -> bool {
    crate::resource_install::no_links(p).is_ok()
        && fs::metadata(p).is_ok_and(|m| m.len() == a.size)
        && crate::downloads::digest(p).is_ok_and(|h| h.eq_ignore_ascii_case(&a.sha256))
}

fn installed_valid(root: &Path, m: &Manifest) -> bool {
    let model = root.join(format!("{}.onnx", m.model.sha256));
    let archive = root.join(format!("{}.zip", m.runtime.sha256));
    let runtime = root.join(&m.runtime.sha256).join("onnxruntime.dll");
    if !valid_file(&model, &m.model)
        || !valid_file(&archive, &m.runtime)
        || crate::resource_install::no_links(&runtime).is_err()
    {
        return false;
    }
    let check = || -> Result<bool, String> {
        let mut zip = zip::ZipArchive::new(fs::File::open(archive).map_err(err)?).map_err(err)?;
        let mut entry = zip
            .by_name(&format!(
                "onnxruntime-win-x64-{}/lib/onnxruntime.dll",
                m.runtime.version
            ))
            .map_err(err)?;
        if entry.size() > 128 * 1024 * 1024 {
            return Ok(false);
        }
        let mut expected = Vec::new();
        entry.read_to_end(&mut expected).map_err(err)?;
        Ok(fs::read(runtime).map_err(err)? == expected)
    };
    check().unwrap_or(false)
}

fn download_urls<'a>(a: &'a Asset, preferred: &str) -> Vec<&'a str> {
    let mut urls: Vec<&str> = a.mirrors.iter().map(|m| m.url.as_str()).collect();
    if preferred == "github" {
        urls.insert(0, &a.url);
    } else {
        urls.push(&a.url);
    }
    urls
}
async fn download(
    client: &reqwest::Client,
    a: &Asset,
    p: &Path,
    stop: &AtomicBool,
    progress: &impl Fn(String),
    preferred: &str,
    range: (u64, u64),
) -> Result<(), String> {
    let (base, total) = range;
    if stop.load(Ordering::Acquire) {
        return Err("视觉资源下载已取消".into());
    }
    if valid_file(p, a) {
        report(
            progress,
            "正在校验视觉识别资源包".into(),
            Some(((base + a.size) * 100 / total) as u8),
        );
        return Ok(());
    }
    let urls = download_urls(a, preferred);
    let mut last = String::new();
    for attempt in 0..3 {
        for (index, url) in urls.iter().enumerate() {
            if stop.load(Ordering::Acquire) {
                return Err("视觉资源下载已取消".into());
            }
            if index > 0 {
                report(progress, "资源下载源不可用，切换备用源".into(), None);
            }
            match download_once(client, a, url, p, stop, progress, range).await {
                Ok(()) => return Ok(()),
                Err(error) => last = error,
            }
            if stop.load(Ordering::Acquire) {
                return Err("视觉资源下载已取消".into());
            }
        }
        report(
            progress,
            format!("资源下载失败，重试 {}/3：{last}", attempt + 1),
            None,
        );
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err(last)
}
async fn download_once(
    client: &reqwest::Client,
    a: &Asset,
    url: &str,
    p: &Path,
    stop: &AtomicBool,
    progress: &impl Fn(String),
    range: (u64, u64),
) -> Result<(), String> {
    let (base, total) = range;
    if stop.load(Ordering::Acquire) {
        return Err("视觉资源下载已取消".into());
    }
    if valid_file(p, a) {
        return Ok(());
    }
    let temporary = p.with_extension(format!("{}.part", uuid::Uuid::new_v4()));
    let _partial = PartialFile(temporary.clone());
    let result = async {
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("{e:?}"))?
            .error_for_status()
            .map_err(err)?;
        let mut f = fs::File::create(&temporary).map_err(err)?;
        let mut count = 0u64;
        let mut last_percent = 101;
        while let Some(bytes) = response.chunk().await.map_err(err)? {
            if stop.load(Ordering::Acquire) {
                return Err("视觉资源下载已取消".into());
            }
            count += bytes.len() as u64;
            if count > a.size {
                return Err("视觉资源超过清单大小".into());
            }
            f.write_all(&bytes).map_err(err)?;
            let percent = (base + count) * 100 / total;
            if percent != last_percent {
                report(
                    progress,
                    format!("下载视觉识别资源包：{percent}%"),
                    Some(percent as u8),
                );
                last_percent = percent;
            }
        }
        f.sync_all().map_err(err)?;
        drop(f);
        if !valid_file(&temporary, a) {
            return Err("视觉资源 SHA-256 或大小校验失败".into());
        }
        if p.exists() {
            crate::infrastructure::durable_fs::durable_sibling_replace(&temporary, p)
                .map_err(err)?;
        } else {
            fs::rename(&temporary, p).map_err(err)?;
        }
        Ok(())
    }
    .await;
    let _ = fs::remove_file(&temporary);
    result
}
#[cfg(test)]
pub async fn ensure(
    root: &Path,
    stop: &AtomicBool,
    progress: impl Fn(String),
) -> Result<Installed, String> {
    ensure_preferred(root, stop, "gitee", progress).await
}

pub async fn ensure_preferred(
    root: &Path,
    stop: &AtomicBool,
    preferred: &str,
    progress: impl Fn(String),
) -> Result<Installed, String> {
    let _exclusive = PREPARATION.lock().await;
    if stop.load(Ordering::Acquire) {
        return Err("视觉资源准备已取消".into());
    }
    package_status(root, false)?;
    if let Some(status) = PACKAGE.lock().as_mut() {
        status.preparing = true;
        status.ready = false;
        status.progress = None;
    }
    let mut preparing = Preparing(false);
    let result = ensure_inner(root, stop, preferred, &progress).await;
    if let Some(status) = PACKAGE.lock().as_mut() {
        status.preparing = false;
        status.ready = result.is_ok();
        status.progress = result.is_ok().then_some(100);
        status.message = match &result {
            Ok(_) => "资源包已就绪".into(),
            Err(error) => error.clone(),
        };
    }
    preparing.0 = true;
    result
}

async fn ensure_inner(
    root: &Path,
    stop: &AtomicBool,
    preferred: &str,
    progress: &impl Fn(String),
) -> Result<Installed, String> {
    fs::create_dir_all(root).map_err(err)?;
    crate::resource_install::no_links(root)?;
    let client = reqwest::Client::builder()
        .https_only(true)
        .user_agent("D2RHub-Vision/1")
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(err)?;
    let mut manifest = local_manifest(root)?;
    let cache = root.join("manifest.json");
    report(progress, "检查视觉资源版本".into(), None);
    // A missing release index/offline host may use the pinned, verified manifest.
    let remote = async {
        let mut response = client
            .get(ONLINE)
            .timeout(std::time::Duration::from_secs(8))
            .send()
            .await
            .map_err(err)?
            .error_for_status()
            .map_err(err)?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(err)? {
            if bytes.len() + chunk.len() > 65536 {
                return Err("视觉清单过大".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice::<Manifest>(&bytes).map_err(err)
    }
    .await;
    if let Ok(remote) = remote {
        // An incompatible future index must not disable the verified cache or
        // the pinned resources this client can still load.
        adopt_newer_compatible_manifest(&mut manifest, remote);
    }
    if stop.load(Ordering::Acquire) {
        return Err("视觉资源准备已取消".into());
    }
    let model = root.join(format!("{}.onnx", manifest.model.sha256));
    let archive = root.join(format!("{}.zip", manifest.runtime.sha256));
    let total = manifest.model.size + manifest.runtime.size;
    if let Some(status) = PACKAGE.lock().as_mut() {
        status.size = total;
    }
    download(
        &client,
        &manifest.model,
        &model,
        stop,
        progress,
        preferred,
        (0, total),
    )
    .await?;
    download(
        &client,
        &manifest.runtime,
        &archive,
        stop,
        progress,
        preferred,
        (manifest.model.size, total),
    )
    .await?;
    report(progress, "正在校验并安装视觉识别资源包".into(), Some(100));
    let runtime_dir = root.join(&manifest.runtime.sha256);
    fs::create_dir_all(&runtime_dir).map_err(err)?;
    crate::resource_install::no_links(&runtime_dir)?;
    let mut zip = zip::ZipArchive::new(fs::File::open(&archive).map_err(err)?).map_err(err)?;
    // Extract exactly the CPU runtime, never arbitrary archive paths or executables.
    let mut entry = zip
        .by_name(&format!(
            "onnxruntime-win-x64-{}/lib/onnxruntime.dll",
            manifest.runtime.version
        ))
        .map_err(err)?;
    if entry.size() > 128 * 1024 * 1024 {
        return Err("推理运行库大小无效".into());
    }
    let mut data = Vec::new();
    entry.read_to_end(&mut data).map_err(err)?;
    let runtime = runtime_dir.join("onnxruntime.dll");
    if !fs::read(&runtime).is_ok_and(|bytes| bytes == data) {
        let temp = runtime_dir.join(format!("onnxruntime.{}.part", uuid::Uuid::new_v4()));
        let _partial = PartialFile(temp.clone());
        fs::write(&temp, data).map_err(err)?;
        if runtime.exists() {
            crate::resource_install::no_links(&runtime)?;
            crate::infrastructure::durable_fs::durable_sibling_replace(&temp, &runtime)
                .map_err(|e| format!("修复运行库失败，请关闭软件后重试：{e}"))?;
        } else {
            fs::rename(temp, &runtime).map_err(err)?;
        }
    }
    crate::downloads::save_json(&cache, &manifest)?;
    report(progress, "视觉识别资源包已就绪".into(), Some(100));
    Ok(Installed { model, runtime })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn resource_directory_uses_native_separators_for_explorer() {
        let root = resource_root(r"C:\Users\测试 用户\AppData\Roaming\D2RHub");
        // Compare the actual argument, since Path equality also accepts forward slashes.
        assert_eq!(
            root.as_os_str(),
            std::ffi::OsStr::new(r"C:\Users\测试 用户\AppData\Roaming\D2RHub\resources\vision")
        );
    }

    fn pinned() -> Manifest {
        serde_json::from_str(include_str!("../../../resources/vision-resources-v1.json")).unwrap()
    }

    #[test]
    fn old_manifests_remain_valid_and_owned_mirrors_follow_source_preference() {
        let mut m = pinned();
        assert_eq!(
            download_urls(&m.runtime, "gitee")[0],
            m.runtime.mirrors[0].url
        );
        assert_eq!(download_urls(&m.runtime, "github")[0], m.runtime.url);
        let mut legacy = serde_json::to_value(&m).unwrap();
        for kind in ["model", "runtime"] {
            legacy[kind].as_object_mut().unwrap().remove("mirrors");
        }
        let legacy: Manifest = serde_json::from_value(legacy).unwrap();
        assert!(validate(&legacy).is_ok());
        assert_eq!(
            download_urls(&legacy.runtime, "gitee"),
            vec![legacy.runtime.url.as_str()]
        );
        m.runtime.mirrors[0].url =
            "https://gitee.com/another/repo/releases/download/v1/runtime.zip".into();
        assert!(validate(&m).is_err());
    }

    fn serve(body: &'static [u8]) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/asset", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request).unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        });
        (url, worker)
    }

    #[test]
    fn corrupt_mirror_falls_back_and_valid_cache_needs_no_network() {
        use sha2::{Digest, Sha256};
        let (mirror, first) = serve(b"bad!");
        let (upstream, second) = serve(b"good");
        let a = Asset {
            version: "test".into(),
            url: upstream,
            size: 4,
            sha256: format!("{:x}", Sha256::digest(b"good")),
            mirrors: vec![crate::downloads::Mirror {
                platform: "gitee".into(),
                url: mirror,
            }],
        };
        let root = std::env::temp_dir().join(format!("vision-download-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("asset.onnx");
        let stop = AtomicBool::new(false);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let messages = parking_lot::Mutex::new(Vec::new());
        tauri::async_runtime::block_on(download(
            &client,
            &a,
            &path,
            &stop,
            &|s| messages.lock().push(s),
            "gitee",
            (0, 4),
        ))
        .unwrap();
        first.join().unwrap();
        second.join().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"good");
        assert!(messages.lock().iter().any(|s| s.contains("切换备用源")));
        // Both listeners are gone: a second call can only succeed by using the verified cache.
        tauri::async_runtime::block_on(download(
            &client,
            &a,
            &path,
            &stop,
            &|_| {},
            "gitee",
            (0, 4),
        ))
        .unwrap();
        stop.store(true, Ordering::Release);
        assert!(tauri::async_runtime::block_on(download(
            &client,
            &a,
            &path,
            &stop,
            &|_| {},
            "gitee",
            (0, 4)
        ))
        .is_err());
        assert_eq!(
            fs::read_dir(&root).unwrap().count(),
            1,
            "partial files must be cleaned up"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "Downloads both pinned assets to an isolated directory through the Gitee mirror"]
    fn gitee_package_download_smoke() {
        let root = PathBuf::from(
            std::env::var_os("D2RHUB_VISION_MIRROR_SMOKE_ROOT").expect("isolated smoke directory"),
        );
        let stop = AtomicBool::new(false);
        let files = tauri::async_runtime::block_on(ensure_preferred(&root, &stop, "gitee", |s| {
            println!("{s}")
        }))
        .unwrap();
        assert!(files.model.is_file());
        assert!(files.runtime.is_file());
        assert!(package_status(&root, true).unwrap().ready);
        // Loading ORT confirms extraction and model format, beyond archive hashes.
        super::super::ocr::Ocr::new(&files).unwrap();
    }
    #[test]
    fn validates_pinned_manifest_and_rejects_foreign_binary() {
        let mut m: Manifest =
            serde_json::from_str(include_str!("../../../resources/vision-resources-v1.json"))
                .unwrap();
        assert!(validate(&m).is_ok());
        m.runtime.url = "https://example.com/runtime.zip".into();
        assert!(validate(&m).is_err());
    }

    #[test]
    fn incompatible_or_invalid_updates_retain_the_compatible_manifest() {
        let mut current: Manifest =
            serde_json::from_str(include_str!("../../../resources/vision-resources-v1.json"))
                .unwrap();
        let mut candidate = current.clone();
        candidate.revision += 1;
        candidate.engine_abi = "ort-future".into();
        let revision = current.revision;
        adopt_newer_compatible_manifest(&mut current, candidate);
        assert_eq!(current.revision, revision);
        let mut candidate = current.clone();
        candidate.revision += 1;
        candidate.runtime.url = "https://example.com/runtime.zip".into();
        adopt_newer_compatible_manifest(&mut current, candidate);
        assert_eq!(current.revision, revision);
        let mut candidate = current.clone();
        candidate.revision += 1;
        adopt_newer_compatible_manifest(&mut current, candidate);
        assert_eq!(current.revision, revision + 1);
    }
}
