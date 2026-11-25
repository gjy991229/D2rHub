use super::validation::processing_manifest_path;
use crate::{
    application::task_runtime::TaskHandle,
    domain::mod_processing::{
        parse_feature_groups, JCY_INVENTORY_FEATURE_ID, JCY_INVENTORY_FEATURE_RECIPE_VERSION,
    },
    downloads::{self, Payload},
    infrastructure::durable_fs,
    resource_install,
    state::SharedState,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MANIFEST: &str = include_str!("../../../resources/jcy-inventory-v1.json");

pub(super) fn needed(requested: bool, source: Option<&Path>) -> Result<bool, String> {
    let Some(manifest) = source.and_then(processing_manifest_path) else {
        return Ok(requested);
    };
    let bytes = fs::read(manifest).map_err(|e| format!("读取源 Mod 清单失败：{e}"))?;
    let document =
        serde_json::from_slice(&bytes).map_err(|e| format!("解析源 Mod 清单失败：{e}"))?;
    let groups = parse_feature_groups(&document)?;
    // The processor validates and copies a current recipe. Older inherited
    // recipes need the preset even if the checkbox is locked by the source Mod.
    Ok(
        match groups
            .iter()
            .find(|group| group.id == JCY_INVENTORY_FEATURE_ID)
        {
            Some(group) => group.recipe_version < JCY_INVENTORY_FEATURE_RECIPE_VERSION,
            None => requested,
        },
    )
}

struct PartialFile(PathBuf);

impl Drop for PartialFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn cache_entry(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            resource_install::no_links(path)?;
            if metadata.is_file() {
                Ok(true)
            } else {
                Err("JCY 资源缓存路径不是文件".into())
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

fn install_verified(source: &Path, target: &Path, asset: &Payload) -> Result<(), String> {
    downloads::verify(source, asset)?;
    let result = if cache_entry(target)? {
        if downloads::verify(target, asset).is_ok() {
            return Ok(());
        }
        durable_fs::durable_sibling_replace(source, target)
    } else {
        durable_fs::durable_sibling_rename(source, target)
    };
    if let Err(error) = result {
        // Another Windows session can finish installing the same digest while
        // this session is between the cache check and an atomic rename.
        if !matches!(cache_entry(target), Ok(true)) || downloads::verify(target, asset).is_err() {
            return Err(format!("保存 JCY 资源包失败：{error}"));
        }
    }
    Ok(())
}

pub(super) async fn ensure(
    app: &tauri::AppHandle,
    state: &SharedState,
    task: &TaskHandle,
    progress_ceiling: u8,
    on_progress: &(dyn Fn(u8, &str) + Sync),
) -> Result<PathBuf, String> {
    let asset: Payload = serde_json::from_str(MANIFEST).map_err(|e| e.to_string())?;
    downloads::validate_payload(&asset)?;
    let resources = Path::new(&state.app_data_dir).join("resources");
    let root = resources.join("jcy-inventory");
    fs::create_dir_all(&resources).map_err(|e| e.to_string())?;
    resource_install::no_links(&resources)?;
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    resource_install::no_links(&root)?;
    let target = root.join(format!("{}.preset.gz", asset.sha256));
    if cache_entry(&target)? && downloads::verify(&target, &asset).is_ok() {
        return Ok(target);
    }

    let partial = PartialFile(root.join(format!(".download-{}.part", uuid::Uuid::new_v4())));
    downloads::download_with_progress(
        app,
        state,
        &asset,
        &partial.0,
        task,
        progress_ceiling,
        Some(on_progress),
    )
    .await?;
    if task.cancellation_requested() {
        return Err("下载已取消".into());
    }
    install_verified(&partial.0, &target, &asset)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_mod::filesystem::TemporaryDirectory;

    fn temp() -> TemporaryDirectory {
        TemporaryDirectory::create(
            std::env::temp_dir().join(format!("d2rhub-jcy-test-{}", uuid::Uuid::new_v4())),
        )
        .unwrap()
    }

    #[test]
    fn resource_is_needed_only_for_creating_or_upgrading_the_inventory_feature() {
        let temp = temp();
        assert!(!needed(false, Some(temp.path())).unwrap());
        assert!(needed(true, None).unwrap());
        let manifest = temp.path().join("d2rhub-mod-manifest.json");
        for (recipe_version, expected) in [(3, true), (4, false)] {
            fs::write(
                &manifest,
                serde_json::json!({
                    "feature_groups": [{
                        "id": JCY_INVENTORY_FEATURE_ID,
                        "recipe_version": recipe_version,
                        "fingerprint": "fixture"
                    }]
                })
                .to_string(),
            )
            .unwrap();
            assert_eq!(needed(false, Some(temp.path())).unwrap(), expected);
            assert_eq!(needed(true, Some(temp.path())).unwrap(), expected);
        }
    }

    #[test]
    fn cache_repair_only_replaces_the_file_with_verified_bytes() {
        let temp = temp();
        let partial = temp.path().join("download.part");
        let target = temp.path().join("cached.preset.gz");
        let mut asset: Payload = serde_json::from_str(MANIFEST).unwrap();
        downloads::validate_payload(&asset).unwrap();

        fs::write(&partial, b"valid cache").unwrap();
        asset.size = 11;
        asset.sha256 = downloads::digest(&partial).unwrap();
        install_verified(&partial, &target, &asset).unwrap();
        assert!(cache_entry(&target).unwrap());

        fs::write(&target, b"stale cache").unwrap();
        fs::write(&partial, b"wrong bytes").unwrap();
        assert!(install_verified(&partial, &target, &asset).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"stale cache");

        fs::write(&partial, b"valid cache").unwrap();
        install_verified(&partial, &target, &asset).unwrap();
        downloads::verify(&target, &asset).unwrap();
    }
}
