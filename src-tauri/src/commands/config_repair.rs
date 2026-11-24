use crate::application::multi_instance::AccountLeaseManager;
use crate::commands::account::AccountManager;
use crate::domain::account::AccountMeta;
use crate::infrastructure::durable_fs::durable_sibling_replace;
use crate::state::SharedState;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Default, Serialize)]
pub(crate) struct ConfigRepairReport {
    pub repaired: Vec<PathBuf>,
    pub issues: Vec<String>,
}

#[tauri::command(async)]
pub(crate) fn check_and_repair_configs(
    app: tauri::AppHandle,
    global: tauri::State<'_, SharedState>,
) -> Result<ConfigRepairReport, String> {
    let _profile = global
        .runtime_activation_lock
        .try_lock()
        .ok_or_else(|| "模式切换或模块操作进行中，请稍后重试".to_string())?;
    let config = global
        .configuration()
        .snapshot()
        .ok_or("尚未加载全局配置")?;
    let mut report = ConfigRepairReport::default();
    repair_account_configs(
        &config.accounts_dir,
        global.multi_instance().account_leases(),
        &mut report,
    );
    if let Err(error) = super::room_automation::reload_after_config_repair(&app, &global) {
        report.issues.push(format!("自动跟房：{error}"));
    }
    Ok(report)
}

fn repair_account_configs(
    accounts_dir: &str,
    leases: &AccountLeaseManager,
    report: &mut ConfigRepairReport,
) {
    for id in AccountManager::list_ids(accounts_dir) {
        let path = Path::new(accounts_dir).join(&id).join("account.json");
        match repair_account_config(&path, &id, leases) {
            Ok(Some(archive)) => report.repaired.push(archive),
            Ok(None) => {}
            Err(error) => report.issues.push(format!("{}: {error}", path.display())),
        }
    }
}

fn repair_account_config(
    path: &Path,
    id: &str,
    leases: &AccountLeaseManager,
) -> Result<Option<PathBuf>, String> {
    let Some(original) = read_regular_file(path)? else {
        let _lease = leases.try_acquire(id).map_err(|error| error.to_string())?;
        if path.exists() || path.with_extension("json.tmp").exists() {
            return Err("配置正在变化，请重试".into());
        }
        let backup = path.with_extension("json.bak");
        let bytes = read_regular_file(&backup)?
            .filter(|bytes| valid_account(bytes, id))
            .ok_or("缺少账号配置且没有有效备份")?;
        let staged = path.with_extension(format!("json.repair-{}", uuid::Uuid::new_v4()));
        let result = write_new_file(&staged, &bytes).and_then(|()| {
            crate::infrastructure::durable_fs::durable_sibling_rename(&staged, path)
        });
        if let Err(error) = result {
            let _ = fs::remove_file(&staged);
            return Err(format!("恢复配置失败：{error}"));
        }
        return Ok(Some(backup));
    };
    if valid_account(&original, id) {
        return Ok(None);
    }

    let _lease = leases.try_acquire(id).map_err(|error| error.to_string())?;
    if read_regular_file(path)?.as_deref() != Some(&original) {
        return Err("文件已变化，请重试".into());
    }
    let parse_error = serde_json::from_slice::<serde_json::Value>(&original).err();
    let repaired = parse_error
        .as_ref()
        .and_then(|error| insert_missing_array_comma(&original, error))
        .filter(|candidate| valid_account(candidate, id));
    let replacement = match repaired {
        Some(repaired) => repaired,
        None => read_regular_file(&path.with_extension("json.bak"))?
            .filter(|backup| valid_account(backup, id))
            .ok_or_else(|| {
                let reason = parse_error.map_or_else(
                    || "账号数据结构或 ID 校验失败".into(),
                    |error| error.to_string(),
                );
                format!("{reason}；没有有效备份，原文件未修改")
            })?,
    };
    let suffix = uuid::Uuid::new_v4();
    let archive = path.with_extension(format!("json.corrupt-{suffix}"));
    let staged = path.with_extension(format!("json.repair-{suffix}"));
    write_new_file(&archive, &original).map_err(|error| format!("保留原文件失败：{error}"))?;
    let result =
        write_new_file(&staged, &replacement).and_then(|()| durable_sibling_replace(&staged, path));
    if let Err(error) = result {
        let _ = fs::remove_file(&staged);
        return Err(format!(
            "写回失败：{error}；原件留存于 {}",
            archive.display()
        ));
    }
    Ok(Some(archive))
}

fn read_regular_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            fs::read(path).map(Some).map_err(|error| error.to_string())
        }
        Ok(_) => Err(format!("不是普通文件：{}", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn valid_account(bytes: &[u8], id: &str) -> bool {
    serde_json::from_slice::<AccountMeta>(bytes)
        .is_ok_and(|account| account.id.eq_ignore_ascii_case(id))
}

fn insert_missing_array_comma(bytes: &[u8], error: &serde_json::Error) -> Option<Vec<u8>> {
    if !error
        .to_string()
        .starts_with("expected `,` or `]` at line ")
    {
        return None;
    }
    let line_index = error.line().checked_sub(1)?;
    let start_of_line = bytes
        .split_inclusive(|byte| *byte == b'\n')
        .take(line_index)
        .map(<[u8]>::len)
        .sum::<usize>();
    let offset = start_of_line.checked_add(error.column().checked_sub(1)?)?;
    let next = *bytes.get(offset)?;
    if !matches!(
        next,
        b'"' | b'{' | b'[' | b'-' | b'0'..=b'9' | b't' | b'f' | b'n'
    ) {
        return None;
    }
    let mut candidate = bytes.to_vec();
    candidate.insert(offset, b',');
    Some(candidate)
}

fn write_new_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_repair_keeps_original_repairs_comma_and_checks_backups() {
        let root = std::env::temp_dir().join(format!("d2rhub-repair-{}", uuid::Uuid::new_v4()));
        let missing_comma = b"{\r\n \"id\": \"acount1\",\r\n \"mod_list\": [\r\n \"first\"\r\n    \"second\"\r\n ], \"extra\": 4\r\n}\r\n";
        let broken = b"{\"id\": \"acount3\", \"mod_list\": [";
        for (id, content) in [
            ("acount1", missing_comma.as_slice()),
            ("acount2", b"{\"id\":"),
            ("acount3", broken.as_slice()),
            ("acount4", b"{\"id\":\"acount4\"}"),
        ] {
            let dir = root.join(id);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("account.json"), content).unwrap();
        }
        let backup = br#"{"id":"acount2","display_name":"backup"}"#;
        fs::write(root.join("acount2/account.json.bak"), backup).unwrap();
        fs::write(root.join("acount3/account.json.bak"), backup).unwrap();

        let repair = || {
            let mut report = ConfigRepairReport::default();
            repair_account_configs(
                root.to_str().unwrap(),
                &AccountLeaseManager::default(),
                &mut report,
            );
            report
        };
        let missing = root.join("acount5");
        fs::create_dir(&missing).unwrap();
        let valid_backup = br#"{"id":"acount5"}"#;
        fs::write(missing.join("account.json.bak"), valid_backup).unwrap();
        let report = repair();
        assert_eq!(
            fs::read(missing.join("account.json")).unwrap(),
            valid_backup
        );
        assert_eq!(
            fs::read(missing.join("account.json.bak")).unwrap(),
            valid_backup
        );
        assert_eq!(report.repaired.len(), 3);
        assert_eq!(fs::read(&report.repaired[0]).unwrap(), missing_comma);
        let fixed = fs::read(root.join("acount1/account.json")).unwrap();
        assert_eq!(fixed.len(), missing_comma.len() + 1);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fixed).unwrap()["extra"],
            4
        );
        assert_eq!(fs::read(root.join("acount2/account.json")).unwrap(), backup);
        assert_eq!(fs::read(root.join("acount3/account.json")).unwrap(), broken);
        assert_eq!(report.issues.len(), 1);
        assert!(report.issues[0].contains("acount3"));
        assert!(repair().repaired.is_empty());

        fs::remove_dir_all(root).unwrap();
    }
}
