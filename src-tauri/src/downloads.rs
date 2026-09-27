//! Anonymous dual-source transport shared by installers and optional resources.
use crate::{
    application::task_runtime::TaskHandle, domain::config::DownloadSource,
    infrastructure::durable_fs, state::SharedState,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mirror {
    pub platform: String,
    pub url: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Payload {
    pub version: String,
    pub size: u64,
    pub sha256: String,
    pub mirrors: Vec<Mirror>,
}
pub const GITHUB_REPO: &str = "gjy991229/D2rHub";
pub const GITEE_REPO: &str = "garyi7e/d2-rhub_-resource";
static INDEX_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static REGION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub fn index_url(platform: &str, kind: &str) -> String {
    let tag = if kind == "software" {
        "hub-update-index-v2"
    } else {
        "mod-update-index-v2"
    };
    if platform == "gitee" {
        format!("https://gitee.com/api/v5/repos/{GITEE_REPO}/releases/tags/{tag}")
    } else {
        format!("https://api.github.com/repos/{GITHUB_REPO}/releases/tags/{tag}")
    }
}
pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("D2RHub-Downloads/2")
        .https_only(true)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(900))
        .build()
        .map_err(|e| e.to_string())
}
pub fn validate_payload(p: &Payload) -> Result<(), String> {
    if p.size == 0
        || p.size > 2 * 1024 * 1024 * 1024
        || p.sha256.len() != 64
        || !p.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || p.mirrors.is_empty()
        || p.mirrors.len() > 2
    {
        return Err("下载清单大小、摘要或来源无效".into());
    }
    let mut platforms = std::collections::HashSet::new();
    for m in &p.mirrors {
        let u = reqwest::Url::parse(&m.url).map_err(|e| e.to_string())?;
        let permitted = match m.platform.as_str() {
            "github" => {
                u.host_str() == Some("github.com")
                    && u.path()
                        .starts_with(&format!("/{GITHUB_REPO}/releases/download/"))
            }
            "gitee" => {
                u.host_str() == Some("gitee.com")
                    && u.path()
                        .starts_with(&format!("/{GITEE_REPO}/releases/download/"))
            }
            _ => false,
        };
        if !permitted
            || !platforms.insert(&m.platform)
            || u.scheme() != "https"
            || !u.username().is_empty()
            || u.password().is_some()
            || u.port().is_some()
            || u.query().is_some()
            || u.fragment().is_some()
        {
            return Err("下载来源不属于指定发布仓库".into());
        }
    }
    Ok(())
}
pub fn digest(path: &Path) -> Result<String, String> {
    let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
pub fn verify(path: &Path, p: &Payload) -> Result<(), String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() != p.size
        || digest(path)? != p.sha256.to_lowercase()
    {
        return Err("下载文件大小或 SHA-256 校验失败".into());
    }
    Ok(())
}
pub fn save_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("缺少父目录")?).map_err(|e| e.to_string())?;
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        f.write_all(&serde_json::to_vec(value).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        drop(f);
        if path.exists() {
            durable_fs::durable_sibling_replace(&temp, path)
        } else {
            durable_fs::durable_sibling_rename(&temp, path)
        }
        .map_err(|e| e.to_string())
    })();
    let _ = fs::remove_file(temp);
    result
}
pub fn cache_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("downloads"))
}
pub fn version_newer(candidate: &str, current: &str) -> bool {
    match (
        semver::Version::parse(candidate.trim_start_matches('v')),
        semver::Version::parse(current.trim_start_matches('v')),
    ) {
        (Ok(a), Ok(b)) => a > b,
        _ => false,
    }
}
pub fn compatible(min: &str, max: &str) -> bool {
    let parse = |s: &str| semver::Version::parse(s.trim_start_matches('v')).ok();
    match (parse(min), parse(max), parse(env!("CARGO_PKG_VERSION"))) {
        (Some(lo), Some(hi), Some(v)) => lo <= v && v < hi,
        _ => false,
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
#[derive(Serialize, Deserialize)]
struct RegionCache {
    country: Option<String>,
    preferred: String,
    expires: u64,
}
pub fn cached_preference(app: &tauri::AppHandle, state: &SharedState) -> String {
    match state
        .configuration()
        .snapshot()
        .map(|c| c.download_source)
        .unwrap_or_default()
    {
        DownloadSource::Github => return "github".into(),
        DownloadSource::Gitee => return "gitee".into(),
        DownloadSource::Auto => {}
    }
    cache_root(app)
        .ok()
        .and_then(|r| fs::read(r.join("region.json")).ok())
        .and_then(|b| serde_json::from_slice::<RegionCache>(&b).ok())
        .filter(|c| c.expires > now() && matches!(c.preferred.as_str(), "gitee" | "github"))
        .map(|c| c.preferred)
        .unwrap_or_else(|| "gitee".into())
}
pub async fn preference(app: &tauri::AppHandle, state: &SharedState) -> Result<String, String> {
    let pref = state
        .configuration()
        .snapshot()
        .map(|c| c.download_source)
        .unwrap_or_default();
    if pref != DownloadSource::Auto {
        return Ok(if pref == DownloadSource::Gitee {
            "gitee"
        } else {
            "github"
        }
        .into());
    }
    let _region_lock = REGION_LOCK.lock().await;
    let path = cache_root(app)?.join("region.json");
    if let Ok(bytes) = fs::read(&path) {
        if let Ok(c) = serde_json::from_slice::<RegionCache>(&bytes) {
            if c.expires > now() && matches!(c.preferred.as_str(), "gitee" | "github") {
                return Ok(c.preferred);
            }
        }
    }
    // Store only the country, never the public IP returned by the service.
    let region: Result<String, String> = async {
        let r = client()?
            .get("https://api.country.is/")
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let bytes = r.bytes().await.map_err(|e| e.to_string())?;
        if bytes.len() > 4096 {
            return Err("地区响应过大".into());
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        v["country"]
            .as_str()
            .filter(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_uppercase()))
            .map(str::to_owned)
            .ok_or("地区未知".into())
    }
    .await;
    if let Ok(country) = region {
        let preferred = if country == "CN" { "gitee" } else { "github" }.to_string();
        let _ = save_json(
            &path,
            &RegionCache {
                country: Some(country),
                preferred: preferred.clone(),
                expires: now() + 7 * 86400,
            },
        );
        return Ok(preferred);
    }
    let probe = |platform: &'static str| async move {
        match client() {
            Ok(c) => c
                .get(index_url(platform, "resources"))
                .timeout(Duration::from_secs(3))
                .send()
                .await
                .is_ok_and(|r| r.status().is_success()),
            Err(_) => false,
        }
    };
    let (gitee, github) = tokio::join!(probe("gitee"), probe("github"));
    let preferred = if gitee {
        "gitee"
    } else if github {
        "github"
    } else {
        "gitee"
    }
    .to_string();
    let _ = save_json(
        &path,
        &RegionCache {
            country: None,
            preferred: preferred.clone(),
            expires: now() + 3600,
        },
    );
    Ok(preferred)
}
async fn index_body(platform: &str, kind: &str) -> Result<Value, String> {
    let mut r = client()?
        .get(index_url(platform, kind))
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let mut body = Vec::new();
    while let Some(chunk) = r.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + chunk.len() > 256 * 1024 {
            return Err("版本清单过大".into());
        }
        body.extend_from_slice(&chunk);
    }
    let envelope: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    serde_json::from_str(envelope["body"].as_str().ok_or("缺少发布清单")?)
        .map_err(|e| e.to_string())
}
pub fn select_index(current: Option<Value>, candidate: Value) -> Result<Value, String> {
    let n = candidate["revision"].as_u64().ok_or("清单缺少 revision")?;
    match current {
        Some(c) => {
            let old = c["revision"].as_u64().ok_or("缓存 revision 无效")?;
            if old == n && c != candidate {
                return Err("同一清单版本内容冲突，拒绝更新".into());
            }
            // Version identity is immutable even when the conflicting mirror has
            // an older catalog revision and would otherwise be ignored.
            for next in candidate["assets"].as_array().ok_or("清单缺少资源")? {
                if let Some(prev) = c["assets"]
                    .as_array()
                    .and_then(|a| a.iter().find(|a| a["id"] == next["id"]))
                {
                    if prev["version"] == next["version"]
                        && (prev["sha256"] != next["sha256"] || prev["size"] != next["size"])
                    {
                        return Err("同版本资源内容冲突，拒绝更新".into());
                    }
                }
            }
            if n > old {
                for next in candidate["assets"].as_array().ok_or("清单缺少资源")? {
                    if let Some(prev) = c["assets"]
                        .as_array()
                        .and_then(|a| a.iter().find(|a| a["id"] == next["id"]))
                    {
                        if (prev["version"] == next["version"]
                            && (prev["sha256"] != next["sha256"] || prev["size"] != next["size"]))
                            || next["sequence"].as_u64().unwrap_or(0)
                                < prev["sequence"].as_u64().unwrap_or(0)
                            || (next["version"] != prev["version"]
                                && next["sequence"].as_u64().unwrap_or(0)
                                    <= prev["sequence"].as_u64().unwrap_or(0))
                            || (matches!(next["id"].as_str(), Some("hub" | "processor"))
                                && version_newer(
                                    prev["version"].as_str().unwrap_or(""),
                                    next["version"].as_str().unwrap_or(""),
                                ))
                        {
                            return Err("清单试图降级或替换同版本的不同文件".into());
                        }
                    }
                }
            }
            Ok(if old >= n { c } else { candidate })
        }
        None => Ok(candidate),
    }
}
pub async fn fetch_index(
    app: &tauri::AppHandle,
    state: &SharedState,
    kind: &str,
    validate: impl Fn(&Value) -> Result<(), String>,
) -> Result<Value, String> {
    let _index_lock = INDEX_LOCK.lock().await;
    let path = cache_root(app)?.join(format!("{kind}-v2.json"));
    let best = fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .filter(|v| validate(v).is_ok());
    let first = preference(app, state).await?;
    let second = if first == "gitee" { "github" } else { "gitee" };
    // Both small indices are checked so a lagging reachable mirror cannot hide a
    // newer release. Neither request depends on the other platform succeeding.
    let (a, b) = tokio::join!(index_body(&first, kind), index_body(second, kind));
    let best = choose_indices(best, [a, b], validate)?;
    save_json(&path, &best)?;
    Ok(best)
}
fn choose_indices(
    anchor: Option<Value>,
    results: [Result<Value, String>; 2],
    validate: impl Fn(&Value) -> Result<(), String>,
) -> Result<Value, String> {
    let mut best = anchor.clone();
    let mut errors = Vec::new();
    let mut online = false;
    for result in results {
        let checked = result.and_then(|v| {
            validate(&v)?;
            // A malformed update from one mirror must not poison the cache or
            // prevent the other mirror from supplying a valid update.
            if anchor.is_some() {
                select_index(anchor.clone(), v.clone())?;
            }
            Ok(v)
        });
        match checked {
            Ok(v) => {
                best = Some(select_index(best, v)?);
                online = true;
            }
            Err(e) => errors.push(e),
        }
    }
    if !online {
        return Err(format!(
            "未取得兼容的更新清单，请检查网络或稍后重试：{}",
            errors.join("；")
        ));
    }
    best.ok_or_else(|| "两个下载源均不可用，请重试".into())
}

async fn cancelled(task: &TaskHandle) {
    loop {
        if task.cancellation_requested() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
async fn attempt(
    client: &reqwest::Client,
    m: &Mirror,
    p: &Payload,
    target: &Path,
    task: &TaskHandle,
    progress: &mut u8,
    ceiling: u8,
) -> Result<(), String> {
    let send = tokio::time::timeout(Duration::from_secs(15), client.get(&m.url).send());
    let mut response = tokio::select! {_=cancelled(task)=>return Err("下载已取消".into()),r=send=>r.map_err(|_|"连接下载源超时")?.map_err(|e|e.to_string())?.error_for_status().map_err(|e|e.to_string())?};
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(target)
        .map_err(|e| e.to_string())?;
    let mut count = 0u64;
    loop {
        let pending = tokio::time::timeout(Duration::from_secs(20), response.chunk());
        let chunk = tokio::select! {_=cancelled(task)=>return Err("下载已取消".into()),r=pending=>r.map_err(|_|"下载长时间无进度")?.map_err(|e|e.to_string())?};
        let Some(chunk) = chunk else {
            break;
        };
        count += chunk.len() as u64;
        if count > p.size {
            return Err("下载文件大于清单声明".into());
        }
        f.write_all(&chunk).map_err(|e| e.to_string())?;
        *progress = (*progress).max((count * u64::from(ceiling) / p.size) as u8);
        let _ = task.update(
            *progress,
            "download",
            &format!(
                "正在从 {} 下载：{} / {} MB",
                label(&m.platform),
                count / 1048576,
                p.size / 1048576
            ),
        );
    }
    f.sync_all().map_err(|e| e.to_string())?;
    drop(f);
    verify(target, p)
}
fn label(platform: &str) -> &str {
    if platform == "gitee" {
        "Gitee"
    } else {
        "GitHub"
    }
}
pub async fn download(
    app: &tauri::AppHandle,
    state: &SharedState,
    p: &Payload,
    target: &Path,
    task: &TaskHandle,
    ceiling: u8,
) -> Result<(), String> {
    validate_payload(p)?;
    let preferred = tokio::select! {_=cancelled(task)=>return Err("下载已取消".into()),r=preference(app,state)=>r?};
    let mut mirrors = p.mirrors.clone();
    mirrors.sort_by_key(|m| usize::from(m.platform != preferred));
    download_ordered(&client()?, p, &mirrors, target, task, ceiling).await
}
async fn download_ordered(
    client: &reqwest::Client,
    p: &Payload,
    mirrors: &[Mirror],
    target: &Path,
    task: &TaskHandle,
    ceiling: u8,
) -> Result<(), String> {
    if target.exists() {
        return Err("下载临时文件已存在".into());
    }
    let mut progress = 0;
    let mut errors = Vec::new();
    for (i, m) in mirrors.iter().enumerate() {
        if task.cancellation_requested() {
            return Err("下载已取消".into());
        }
        let message = if i == 0 {
            format!("正在从 {} 下载", label(&m.platform))
        } else {
            format!(
                "{} 下载失败，正在切换 {}",
                label(&mirrors[i - 1].platform),
                label(&m.platform)
            )
        };
        let _ = task.update(progress, "download", &message);
        match attempt(client, m, p, target, task, &mut progress, ceiling).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                let _ = fs::remove_file(target);
                if task.cancellation_requested() {
                    return Err("下载已取消".into());
                }
                errors.push(format!("{}：{e}", label(&m.platform)));
            }
        }
    }
    Err(format!("可用下载源均失败，请重试。{}", errors.join("；")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::task_runtime::{TaskRequest, TaskRuntime};
    use std::{
        net::TcpListener,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        thread,
    };
    fn server(
        status: u16,
        body: &'static [u8],
        delay: u64,
    ) -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let count = hits.clone();
        let join = thread::spawn(move || {
            for _ in 0..200 {
                if let Ok((mut s, _)) = listener.accept() {
                    count.fetch_add(1, Ordering::SeqCst);
                    s.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
                    let mut buf = [0; 4096];
                    let _ = s.read(&mut buf);
                    let _ = write!(
                        s,
                        "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    thread::sleep(Duration::from_millis(delay));
                    let _ = s.write_all(body);
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
        });
        (url, hits, join)
    }
    fn payload() -> Payload {
        Payload {
            version: "1.0.0".into(),
            size: 4,
            sha256: format!("{:x}", Sha256::digest(b"good")),
            mirrors: vec![],
        }
    }
    fn run_failover(status: u16, body: &'static [u8], delay: u64) {
        let (a, ha, ja) = server(status, body, delay);
        let (b, hb, jb) = server(200, b"good", 0);
        let mirrors = vec![
            Mirror {
                platform: "gitee".into(),
                url: a,
            },
            Mirror {
                platform: "github".into(),
                url: b,
            },
        ];
        let path = std::env::temp_dir().join(format!("hub-download-test-{}", uuid::Uuid::new_v4()));
        let runtime = TaskRuntime::new(4);
        let task = runtime.begin(TaskRequest::new("download-test")).unwrap();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(180))
            .build()
            .unwrap();
        tauri::async_runtime::block_on(download_ordered(
            &client,
            &payload(),
            &mirrors,
            &path,
            &task,
            95,
        ))
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"good");
        fs::remove_file(path).unwrap();
        ja.join().unwrap();
        jb.join().unwrap();
        assert_eq!(ha.load(Ordering::SeqCst), 1);
        assert_eq!(hb.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn missing_primary_switches_once() {
        run_failover(404, b"nope", 0);
    }
    #[test]
    fn corrupt_primary_switches_once() {
        run_failover(200, b"evil", 0);
    }
    #[test]
    fn stalled_primary_switches_once() {
        run_failover(200, b"good", 350);
    }
    #[test]
    fn cancellation_does_not_try_the_second_source() {
        let (a, ha, ja) = server(200, b"good", 400);
        let (b, hb, jb) = server(200, b"good", 0);
        let mirrors = vec![
            Mirror {
                platform: "gitee".into(),
                url: a,
            },
            Mirror {
                platform: "github".into(),
                url: b,
            },
        ];
        let runtime = TaskRuntime::new(4);
        let task = runtime.begin(TaskRequest::new("download-test")).unwrap();
        let id = task.task_id();
        let monitor = ha.clone();
        let cancel = thread::spawn(move || {
            for _ in 0..100 {
                if monitor.load(Ordering::SeqCst) > 0 {
                    runtime.request_cancel(id).unwrap();
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
        });
        let path =
            std::env::temp_dir().join(format!("hub-download-cancel-{}", uuid::Uuid::new_v4()));
        let result = tauri::async_runtime::block_on(download_ordered(
            &reqwest::Client::new(),
            &payload(),
            &mirrors,
            &path,
            &task,
            95,
        ));
        assert!(result.unwrap_err().contains("取消"));
        assert!(!path.exists());
        cancel.join().unwrap();
        ja.join().unwrap();
        jb.join().unwrap();
        assert_eq!(hb.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn stale_mirror_and_same_revision_conflicts_cannot_replace_latest() {
        let old = serde_json::json!({"revision":1,"assets":[{"id":"hub","version":"1.0.0","sha256":"a","size":4}]});
        let new = serde_json::json!({"revision":2,"assets":[{"id":"hub","version":"1.1.0","sha256":"b","size":4}]});
        assert_eq!(select_index(Some(new.clone()), old.clone()).unwrap(), new);
        let mut split = new.clone();
        split["assets"][0]["sha256"] = serde_json::json!("c");
        assert!(select_index(Some(new.clone()), split).is_err());
        let mut downgrade = old;
        downgrade["revision"] = serde_json::json!(3);
        assert!(select_index(Some(new), downgrade).is_err());
        assert!(!version_newer("0.9.103", "0.9.104"));
        assert!(version_newer("0.9.104", "0.9.103"));
    }
    #[test]
    fn invalid_primary_index_falls_back_without_poisoning_known_versions() {
        let anchor = serde_json::json!({"revision":2,"assets":[{"id":"hub","version":"1.0.0","sequence":1,"size":4,"sha256":"a"}]});
        let next = serde_json::json!({"revision":3,"assets":[{"id":"hub","version":"1.1.0","sequence":2,"size":4,"sha256":"b"}]});
        assert_eq!(
            choose_indices(
                Some(anchor.clone()),
                [Err("primary unavailable".into()), Ok(next.clone())],
                |_| Ok(())
            )
            .unwrap(),
            next
        );
        let mut bad = anchor.clone();
        bad["revision"] = serde_json::json!(4);
        bad["assets"][0]["sha256"] = serde_json::json!("wrong");
        assert_eq!(
            choose_indices(Some(anchor), [Ok(bad), Ok(next.clone())], |_| Ok(())).unwrap(),
            next
        );
        let mut split = next.clone();
        split["revision"] = serde_json::json!(1);
        split["assets"][0]["sha256"] = serde_json::json!("wrong");
        assert!(choose_indices(None, [Ok(next), Ok(split)], |_| Ok(())).is_err());
    }
    #[test]
    fn payload_rejects_foreign_sources_and_duplicate_platforms() {
        let mut p = payload();
        p.mirrors.push(Mirror {
            platform: "gitee".into(),
            url: format!("https://gitee.com/{GITEE_REPO}/releases/download/v1/test.exe"),
        });
        validate_payload(&p).unwrap();
        p.mirrors.push(p.mirrors[0].clone());
        assert!(validate_payload(&p).is_err());
        p.mirrors.pop();
        p.mirrors[0].url = "https://gitee.com/other/repo/releases/download/v1/test.exe".into();
        assert!(validate_payload(&p).is_err());
    }
    #[test]
    #[ignore = "Uses the public anonymous indices on both platforms"]
    fn both_platforms_serve_anonymous_indices() {
        tauri::async_runtime::block_on(async {
            for kind in ["resources", "software"] {
                for platform in ["gitee", "github"] {
                    let v = index_body(platform, kind).await.unwrap();
                    assert_eq!(v["schema"], 2);
                    assert_eq!(v["kind"], kind);
                }
            }
        });
    }
}
