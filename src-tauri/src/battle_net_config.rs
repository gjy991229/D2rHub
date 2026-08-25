use std::path::Path;

use crate::error::AppError;

/// Read the edition-specific additional launch arguments from Battle.net.config.
///
/// Account discovery treats a malformed legacy file as "no reusable value"; callers that
/// mutate the file use [`update_mod_args`] and receive a structured error instead.
pub(crate) fn try_read_mod_args(config_path: &Path, game_key: &str) -> Option<String> {
    let content = std::fs::read_to_string(config_path).ok()?;
    let config: serde_json::Value = serde_json::from_str(&content).ok()?;
    config
        .as_object()?
        .get("Games")?
        .as_object()?
        .get(game_key)?
        .as_object()?
        .get("AdditionalLaunchArguments")?
        .as_str()
        .map(str::to_string)
}

/// Update only the requested edition entry, preserving every unrelated Battle.net setting.
pub(crate) fn update_mod_args(
    config_path: &Path,
    game_key: &str,
    mod_args: &str,
) -> Result<(), AppError> {
    let content = std::fs::read_to_string(config_path)?;
    let mut config: serde_json::Value = serde_json::from_str(&content)?;
    let game = config
        .as_object_mut()
        .ok_or_else(|| {
            AppError::ConfigReadError("Battle.net.config 根节点不是 JSON 对象".to_string())
        })?
        .entry("Games")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            AppError::ConfigReadError("Battle.net.config Games 不是 JSON 对象".to_string())
        })?
        .entry(game_key)
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            AppError::ConfigReadError(format!("Battle.net.config Games.{game_key} 不是 JSON 对象"))
        })?;

    if mod_args.is_empty() {
        game.remove("AdditionalLaunchArguments");
    } else {
        game.insert(
            "AdditionalLaunchArguments".to_string(),
            serde_json::Value::String(mod_args.to_string()),
        );
    }

    std::fs::write(config_path, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{try_read_mod_args, update_mod_args};

    #[test]
    fn update_is_scoped_to_the_requested_edition() {
        let root = std::env::temp_dir().join(format!(
            "d2rhub_bnet_config_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("Battle.net.config");
        std::fs::write(
            &path,
            r#"{"Games":{"osic":{"AdditionalLaunchArguments":"-mod cn"},"osi":{}}}"#,
        )
        .unwrap();

        update_mod_args(&path, "osi", "-mod global").unwrap();

        assert_eq!(
            try_read_mod_args(&path, "osi").as_deref(),
            Some("-mod global")
        );
        assert_eq!(try_read_mod_args(&path, "osic").as_deref(), Some("-mod cn"));
        let _ = std::fs::remove_dir_all(root);
    }
}
