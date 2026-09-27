//! Directory replacement with retained backups and crash recovery.
use crate::{downloads, infrastructure::durable_fs};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
pub const RECEIPT: &str = ".d2rhub-resource.json";
#[derive(Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub id: String,
    pub version: String,
    pub sequence: u64,
    pub sha256: String,
    pub tree_sha256: String,
}
#[derive(Serialize, Deserialize)]
struct Journal {
    transaction: String,
    target: String,
    stage: String,
    backup: String,
    expected_tree: String,
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn no_links(path: &Path) -> Result<(), String> {
    let m = fs::symlink_metadata(path).map_err(err)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err("资源目录包含重解析点".into());
        }
    }
    if m.file_type().is_symlink() {
        return Err("资源目录包含符号链接".into());
    }
    Ok(())
}
fn files(root: &Path, path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    no_links(path)?;
    for e in fs::read_dir(path).map_err(err)? {
        let p = e.map_err(err)?.path();
        no_links(&p)?;
        if p.is_dir() {
            files(root, &p, out)?;
        } else if p.is_file() {
            if p != root.join(RECEIPT) {
                out.push(p);
            }
        } else {
            return Err("资源包含特殊文件".into());
        }
        if out.len() > 100_000 {
            return Err("资源文件过多".into());
        }
    }
    Ok(())
}
pub fn tree_hash(root: &Path) -> Result<String, String> {
    let mut list = Vec::new();
    files(root, root, &mut list)?;
    list.sort();
    let mut h = Sha256::new();
    for p in list {
        // -txt regenerates these tables on game launch. A standalone .bin with
        // no matching text source remains protected as user content.
        let relative = p
            .strip_prefix(root)
            .map_err(err)?
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        let parts: Vec<_> = relative.split('/').collect();
        if parts.len() == 5
            && parts[0].ends_with(".mpq")
            && parts[1..4] == ["data", "global", "excel"]
            && parts[4].ends_with(".bin")
            && p.with_extension("txt").is_file()
        {
            continue;
        }
        let name = p
            .strip_prefix(root)
            .map_err(err)?
            .to_string_lossy()
            .replace('\\', "/");
        h.update((name.len() as u64).to_le_bytes());
        h.update(name.as_bytes());
        let hash = if name == "generation-manifest.json" {
            let mut v: serde_json::Value =
                serde_json::from_slice(&fs::read(&p).map_err(err)?).map_err(err)?;
            if let Some(o) = v.as_object_mut() {
                o.remove("mod_directory");
            }
            format!("{:x}", Sha256::digest(serde_json::to_vec(&v).map_err(err)?))
        } else {
            downloads::digest(&p)?
        };
        h.update(hash.as_bytes());
    }
    Ok(format!("{:x}", h.finalize()))
}
pub fn receipt(root: &Path) -> Option<Receipt> {
    let p = root.join(RECEIPT);
    if fs::metadata(&p).ok()?.len() > 8192 {
        return None;
    }
    serde_json::from_slice(&fs::read(p).ok()?).ok()
}
pub fn sync_tree(root: &Path) -> Result<(), String> {
    no_links(root)?;
    for item in fs::read_dir(root).map_err(err)? {
        let p = item.map_err(err)?.path();
        no_links(&p)?;
        if p.is_dir() {
            sync_tree(&p)?;
        } else {
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(p)
                .map_err(err)?
                .sync_all()
                .map_err(err)?;
        }
    }
    durable_fs::sync_directory(root).map_err(err)
}
pub fn replace(
    parent: &Path,
    output: &Path,
    destination: &Path,
) -> Result<Option<PathBuf>, String> {
    no_links(parent)?;
    sync_tree(output)?;
    if !destination.exists() {
        durable_fs::durable_rename(output, destination).map_err(err)?;
        return Ok(None);
    }
    no_links(destination)?;
    let transaction = uuid::Uuid::new_v4().to_string();
    let backup = parent.join(format!(".d2rhub-resource-backup-{transaction}"));
    let stage = output
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|p| p.to_str())
        .ok_or("暂存目录无效")?
        .to_string();
    let target = destination
        .file_name()
        .and_then(|p| p.to_str())
        .ok_or("目标目录无效")?
        .to_string();
    let journal = Journal {
        transaction: transaction.clone(),
        target,
        stage,
        backup: backup.file_name().unwrap().to_string_lossy().into_owned(),
        expected_tree: tree_hash(output)?,
    };
    let record = parent.join(format!(".d2rhub-resource-update-{transaction}.json"));
    downloads::save_json(&record, &journal)?;
    durable_fs::durable_rename(destination, &backup).map_err(err)?;
    if let Err(error) = durable_fs::durable_rename(output, destination) {
        if !destination.exists() {
            durable_fs::durable_rename(&backup, destination).map_err(|restore| {
                format!(
                    "安装失败：{error}；恢复失败：{restore}，备份保留在 {}",
                    backup.display()
                )
            })?;
        }
        let _ = fs::remove_file(record);
        return Err(error.to_string());
    }
    fs::remove_file(record).map_err(err)?;
    durable_fs::sync_directory(parent).map_err(err)?;
    Ok(Some(backup))
}
fn safe_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() < 150
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
pub fn recover(parent: &Path) -> Result<(), String> {
    if !parent.exists() {
        return Ok(());
    }
    no_links(parent)?;
    for e in fs::read_dir(parent).map_err(err)? {
        let p = e.map_err(err)?.path();
        let name = p.file_name().unwrap_or_default().to_string_lossy();
        if !name.starts_with(".d2rhub-resource-update-") || !name.ends_with(".json") {
            continue;
        }
        no_links(&p)?;
        if fs::metadata(&p).map_err(err)?.len() > 8192 {
            return Err("资源恢复记录过大".into());
        }
        let j: Journal = serde_json::from_slice(&fs::read(&p).map_err(err)?).map_err(err)?;
        if uuid::Uuid::parse_str(&j.transaction).is_err()
            || name != format!(".d2rhub-resource-update-{}.json", j.transaction)
            || j.backup != format!(".d2rhub-resource-backup-{}", j.transaction)
            || !safe_name(&j.target)
            || !j.stage.starts_with(".d2rhub-resource-")
            || !safe_name(&j.stage)
        {
            return Err("资源恢复记录路径无效".into());
        }
        let target = parent.join(j.target);
        let backup = parent.join(j.backup);
        if backup.exists() {
            no_links(&backup)?;
            if !target.exists() {
                durable_fs::durable_rename(&backup, &target).map_err(err)?;
            } else if tree_hash(&target)? != j.expected_tree {
                return Err(format!(
                    "资源更新中断且目标已改变，保留备份：{}",
                    backup.display()
                ));
            }
        } else if !target.exists() {
            return Err("资源更新恢复缺少目标与备份".into());
        }
        fs::remove_file(p).map_err(err)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("hub-install-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn replaces_complete_directory_and_retains_backup() {
        let r = Scratch::new();
        let old = r.0.join("LiteHub");
        fs::create_dir(&old).unwrap();
        fs::write(old.join("data"), b"old").unwrap();
        let stage = r.0.join(".d2rhub-resource-test");
        let new = stage.join("LiteHub");
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("data"), b"new").unwrap();
        let backup = replace(&r.0, &new, &old).unwrap().unwrap();
        assert_eq!(fs::read(old.join("data")).unwrap(), b"new");
        assert_eq!(fs::read(backup.join("data")).unwrap(), b"old");
        recover(&r.0).unwrap();
        assert!(backup.exists());
    }
    #[test]
    fn interrupted_swap_restores_backup_and_rejects_escape_records() {
        let r = Scratch::new();
        let id = uuid::Uuid::new_v4().to_string();
        let backup = format!(".d2rhub-resource-backup-{id}");
        fs::create_dir(r.0.join(&backup)).unwrap();
        fs::write(r.0.join(&backup).join("data"), b"original").unwrap();
        let record = r.0.join(format!(".d2rhub-resource-update-{id}.json"));
        let mut j = Journal {
            transaction: id,
            target: "LiteHub".into(),
            stage: ".d2rhub-resource-test".into(),
            backup,
            expected_tree: "unused".into(),
        };
        downloads::save_json(&record, &j).unwrap();
        recover(&r.0).unwrap();
        assert_eq!(fs::read(r.0.join("LiteHub/data")).unwrap(), b"original");
        j.backup = "../outside".into();
        downloads::save_json(&record, &j).unwrap();
        assert!(recover(&r.0).is_err());
    }
    #[test]
    fn receipt_does_not_change_tree_identity_but_user_edits_do() {
        let r = Scratch::new();
        fs::write(r.0.join("file"), b"official").unwrap();
        let first = tree_hash(&r.0).unwrap();
        fs::write(r.0.join(RECEIPT), b"receipt").unwrap();
        assert_eq!(first, tree_hash(&r.0).unwrap());
        fs::write(r.0.join("file"), b"user edit").unwrap();
        assert_ne!(first, tree_hash(&r.0).unwrap());
    }
    #[test]
    fn invalid_stage_keeps_existing_directory() {
        let r = Scratch::new();
        let old = r.0.join("LiteHub");
        fs::create_dir(&old).unwrap();
        fs::write(old.join("file"), b"keep").unwrap();
        assert!(replace(&r.0, &r.0.join("missing"), &old).is_err());
        assert_eq!(fs::read(old.join("file")).unwrap(), b"keep");
    }
    #[test]
    fn generated_bins_do_not_block_updates_but_modified_text_still_does() {
        let r = Scratch::new();
        let table = r.0.join("LiteHub.mpq/data/global/excel");
        fs::create_dir_all(&table).unwrap();
        fs::write(table.join("misc.txt"), b"official").unwrap();
        let first = tree_hash(&r.0).unwrap();
        fs::write(table.join("misc.bin"), b"game generated cache").unwrap();
        assert_eq!(first, tree_hash(&r.0).unwrap());
        fs::write(table.join("misc.txt"), b"user change").unwrap();
        assert_ne!(first, tree_hash(&r.0).unwrap());
    }
}
