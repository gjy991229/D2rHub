use crate::state::SharedState;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

const KEY_FILE_VERSION: u16 = 37;
const KEY_FILE_HEADER_SIZE: usize = 2;
const KEY_RECORD_SIZE: usize = 20;
const CHAT_ACTION_INDEX: usize = 5;
const CHAT_ACTION_ID: u32 = 5;
const CHAT_RECORD_OFFSET: usize = KEY_FILE_HEADER_SIZE + CHAT_ACTION_INDEX * KEY_RECORD_SIZE;
const SECONDARY_SLOT_OFFSET: usize = CHAT_RECORD_OFFSET + 10;
#[cfg(test)]
const UNBOUND_KEY: u16 = 0xFFFF;
const VK_RETURN: u16 = 0x000D;
const VK_F13: u16 = 0x007C;
const BOUND_KEY_TYPE: u32 = 1;
const UNBOUND_KEY_TYPE: u32 = 0;
const BACKUP_SUFFIX: &str = ".d2rhub-chat-f13.bak";
const WATCH_POLL_MS: u64 = 50;

static AUTO_PATCH_WATCHER_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatF13BindingStatus {
    pub ready: bool,
    pub total_files: usize,
    pub installed_files: usize,
    pub eligible_files: usize,
    pub conflicted_files: usize,
    pub backup_files: usize,
    pub d2r_running: bool,
    pub auto_patch_enabled: bool,
    pub directories: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingState {
    Eligible,
    Installed,
}

#[derive(Debug)]
struct InspectedFile {
    path: PathBuf,
    bytes: Vec<u8>,
    state: Result<BindingState, String>,
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "键位文件长度不足".to_string())?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "键位文件长度不足".to_string())?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn inspect_key_bytes(bytes: &[u8]) -> Result<BindingState, String> {
    if read_u16(bytes, 0)? != KEY_FILE_VERSION {
        return Err("不是已验证的 D2R v37 键位格式".to_string());
    }
    if bytes.len() <= CHAT_RECORD_OFFSET
        || (bytes.len() - KEY_FILE_HEADER_SIZE) % KEY_RECORD_SIZE != 0
    {
        return Err("键位文件结构或长度不符合预期".to_string());
    }

    let primary_id = read_u32(bytes, CHAT_RECORD_OFFSET)?;
    let primary_key = read_u16(bytes, CHAT_RECORD_OFFSET + 4)?;
    let primary_type = read_u32(bytes, CHAT_RECORD_OFFSET + 6)?;
    let secondary_id = read_u32(bytes, SECONDARY_SLOT_OFFSET)?;
    let secondary_key = read_u16(bytes, SECONDARY_SLOT_OFFSET + 4)?;
    let secondary_type = read_u32(bytes, SECONDARY_SLOT_OFFSET + 6)?;
    if primary_id != CHAT_ACTION_ID
        || primary_key != VK_RETURN
        || primary_type != BOUND_KEY_TYPE
        || secondary_id != CHAT_ACTION_ID
    {
        return Err("动作 5 不是“Enter 主键 + 同动作次键”的原生 Chat 记录".to_string());
    }

    let record_count = (bytes.len() - KEY_FILE_HEADER_SIZE) / KEY_RECORD_SIZE;
    for record in 0..record_count {
        let record_offset = KEY_FILE_HEADER_SIZE + record * KEY_RECORD_SIZE;
        for slot in 0..2 {
            if record == CHAT_ACTION_INDEX && slot == 1 {
                continue;
            }
            let slot_offset = record_offset + slot * 10;
            let key = read_u16(bytes, slot_offset + 4)?;
            let key_type = read_u32(bytes, slot_offset + 6)?;
            if key == VK_F13 && key_type != UNBOUND_KEY_TYPE {
                return Err(format!("F13 已被动作 {record} 的第 {} 键位占用", slot + 1));
            }
        }
    }

    match (secondary_key, secondary_type) {
        (VK_F13, BOUND_KEY_TYPE) => Ok(BindingState::Installed),
        // Newly cloud-downloaded v37 .keyo files can retain a non-FFFF raw
        // key value (observed 0x1070) while type 0 still marks the slot as
        // unbound. Preserve that exact raw value in the backup and treat the
        // slot as eligible; restore copies the full 10-byte slot back.
        (_, UNBOUND_KEY_TYPE) => Ok(BindingState::Eligible),
        _ => Err(format!(
            "原生 Chat 第二键位已被占用（VK=0x{secondary_key:04X}，类型={secondary_type}）"
        )),
    }
}

fn patch_f13(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if inspect_key_bytes(bytes)? != BindingState::Eligible {
        return Err("该键位文件不是可安装状态".to_string());
    }
    let mut patched = bytes.to_vec();
    write_u32(&mut patched, SECONDARY_SLOT_OFFSET, CHAT_ACTION_ID);
    write_u16(&mut patched, SECONDARY_SLOT_OFFSET + 4, VK_F13);
    write_u32(&mut patched, SECONDARY_SLOT_OFFSET + 6, BOUND_KEY_TYPE);
    if inspect_key_bytes(&patched)? != BindingState::Installed {
        return Err("F13 键位写入后的结构校验失败".to_string());
    }
    Ok(patched)
}

fn restore_chat_secondary_slot(current: &[u8], backup: &[u8]) -> Result<Vec<u8>, String> {
    let current_state = inspect_key_bytes(current)?;
    if !matches!(
        current_state,
        BindingState::Installed | BindingState::Eligible
    ) {
        return Err("当前键位文件不是可恢复状态".to_string());
    }
    if inspect_key_bytes(backup)? != BindingState::Eligible {
        return Err("备份中的原生 Chat 第二键位不是未绑定状态".to_string());
    }
    let mut restored = current.to_vec();
    restored[SECONDARY_SLOT_OFFSET..SECONDARY_SLOT_OFFSET + 10]
        .copy_from_slice(&backup[SECONDARY_SLOT_OFFSET..SECONDARY_SLOT_OFFSET + 10]);
    if inspect_key_bytes(&restored)? != BindingState::Eligible {
        return Err("恢复后的键位结构校验失败".to_string());
    }
    Ok(restored)
}

fn backup_path(path: &Path) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .ok_or_else(|| format!("键位文件名无效：{}", path.display()))?
        .to_string_lossy();
    Ok(path.with_file_name(format!("{name}{BACKUP_SUFFIX}")))
}

fn is_key_file(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("key") || value.eq_ignore_ascii_case("keyo")
        })
}

fn collect_key_files(directories: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for directory in directories {
        let entries = std::fs::read_dir(directory)
            .map_err(|error| format!("无法读取存档目录 {}：{error}", directory.display()))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("读取存档目录项失败 {}：{error}", directory.display()))?;
            let path = entry.path();
            if entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
                && is_key_file(&path)
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn configured_saved_games_directories(state: &SharedState) -> Result<Vec<PathBuf>, String> {
    let config = state
        .config
        .read()
        .as_ref()
        .cloned()
        .ok_or_else(|| "尚未加载全局配置".to_string())?;
    let mut seen = HashSet::new();
    let mut directories = Vec::new();
    for configured in [config.cn_saved_games_path, config.global_saved_games_path] {
        let trimmed = configured.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = PathBuf::from(trimmed);
        if !path.is_dir() {
            continue;
        }
        let identity = path
            .canonicalize()
            .unwrap_or_else(|_| path.clone())
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_ascii_lowercase();
        if seen.insert(identity) {
            directories.push(path);
        }
    }
    if directories.is_empty() {
        return Err("没有可用的国服或国际服 D2R 存档目录".to_string());
    }
    Ok(directories)
}

fn inspect_files(directories: &[PathBuf]) -> Result<Vec<InspectedFile>, String> {
    collect_key_files(directories)?
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path)
                .map_err(|error| format!("无法读取键位文件 {}：{error}", path.display()))?;
            let state = inspect_key_bytes(&bytes);
            Ok(InspectedFile { path, bytes, state })
        })
        .collect()
}

fn d2r_running() -> bool {
    !crate::commands::system::get_d2r_pids().is_empty()
}

fn auto_patch_enabled(state: &SharedState) -> bool {
    state
        .config
        .read()
        .as_ref()
        .is_some_and(|config| config.room_rotation.chat_f13_auto_patch_enabled)
}

fn set_auto_patch_enabled(state: &SharedState, enabled: bool) -> Result<(), String> {
    let _config_io = state.config_io.lock();
    let mut config = state
        .config
        .read()
        .as_ref()
        .cloned()
        .ok_or_else(|| "尚未加载全局配置".to_string())?;
    config.room_rotation.chat_f13_auto_patch_enabled = enabled;
    config
        .save(&state.app_data_dir)
        .map_err(|error| format!("无法保存 F13 自动补齐设置：{error}"))?;
    *state.config.write() = Some(config);
    Ok(())
}

fn build_status(state: &SharedState) -> Result<ChatF13BindingStatus, String> {
    let directories = configured_saved_games_directories(state)?;
    let files = inspect_files(&directories)?;
    let total_files = files.len();
    let installed_files = files
        .iter()
        .filter(|file| file.state == Ok(BindingState::Installed))
        .count();
    let eligible_files = files
        .iter()
        .filter(|file| file.state == Ok(BindingState::Eligible))
        .count();
    let conflicted_files = files.iter().filter(|file| file.state.is_err()).count();
    let backup_files = files
        .iter()
        .filter(|file| backup_path(&file.path).is_ok_and(|path| path.is_file()))
        .count();
    let d2r_running = d2r_running();
    let ready = total_files > 0 && installed_files == total_files;
    let auto_patch_enabled = auto_patch_enabled(state);
    let mut message = if ready {
        format!("F13 原生 Chat 备用键已安装：{installed_files}/{total_files} 个键位文件")
    } else if total_files == 0 {
        "存档目录中没有找到 .key/.keyo 键位文件".to_string()
    } else if conflicted_files > 0 {
        format!("发现 {conflicted_files} 个格式不兼容或键位冲突文件；未进行自动覆盖")
    } else {
        format!("尚有 {eligible_files}/{total_files} 个键位文件可以安全安装 F13")
    };
    if d2r_running {
        message.push_str("；D2R 正在运行，安装或恢复前必须全部关闭");
    }
    if auto_patch_enabled {
        message.push_str("；新生成的 .key/.keyo 会自动补齐");
    }
    Ok(ChatF13BindingStatus {
        ready,
        total_files,
        installed_files,
        eligible_files,
        conflicted_files,
        backup_files,
        d2r_running,
        auto_patch_enabled,
        directories: directories
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        message,
    })
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|error| format!("无法打开键位文件 {}：{error}", path.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("无法写入键位文件 {}：{error}", path.display()))
}

fn create_backup(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let backup = backup_path(path)?;
    if backup.is_file() {
        let existing = std::fs::read(&backup)
            .map_err(|error| format!("无法读取已有备份 {}：{error}", backup.display()))?;
        // Restore only copies the Chat secondary slot, so unrelated key edits
        // made after the first backup do not invalidate that backup.
        if inspect_key_bytes(&existing)? != BindingState::Eligible {
            return Err(format!(
                "已有备份不是可恢复的原生 Chat 状态：{}",
                backup.display()
            ));
        }
        return Ok(());
    }
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(&backup)
        .map_err(|error| format!("无法创建键位备份 {}：{error}", backup.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("无法写入键位备份 {}：{error}", backup.display()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
}

fn file_stamp(path: &Path) -> Option<FileStamp> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(FileStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn patch_new_eligible_file(path: &Path) -> Result<bool, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("无法读取新键位文件 {}：{error}", path.display()))?;
    match inspect_key_bytes(&bytes) {
        Ok(BindingState::Installed) => Ok(false),
        Ok(BindingState::Eligible) => {
            create_backup(path, &bytes)?;
            let patched = patch_f13(&bytes)?;
            write_file(path, &patched)?;
            let verified = std::fs::read(path)
                .map_err(|error| format!("无法复核新键位文件 {}：{error}", path.display()))?;
            if inspect_key_bytes(&verified)? != BindingState::Installed {
                return Err(format!("新键位文件写入后复核失败：{}", path.display()));
            }
            Ok(true)
        }
        Err(error) => Err(error),
    }
}

/// Starts before any account launch. Once the user has installed F13, this
/// watches both save directories so a cloud-downloaded or newly created
/// character key file is patched as soon as its complete v37 record appears.
/// A changed file is examined again, which also handles D2R rewriting it once
/// more during character initialization.
pub(crate) fn start_auto_patch_watcher(state: SharedState) {
    if AUTO_PATCH_WATCHER_STARTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    std::thread::spawn(move || {
        let mut observed: HashMap<PathBuf, FileStamp> = HashMap::new();
        loop {
            if !auto_patch_enabled(&state) {
                observed.clear();
                std::thread::sleep(Duration::from_millis(WATCH_POLL_MS));
                continue;
            }
            let Ok(directories) = configured_saved_games_directories(&state) else {
                std::thread::sleep(Duration::from_millis(WATCH_POLL_MS));
                continue;
            };
            let Ok(files) = collect_key_files(&directories) else {
                std::thread::sleep(Duration::from_millis(WATCH_POLL_MS));
                continue;
            };
            let live_paths: HashSet<PathBuf> = files.iter().cloned().collect();
            observed.retain(|path, _| live_paths.contains(path));
            for path in files {
                let Some(stamp) = file_stamp(&path) else {
                    continue;
                };
                if observed.get(&path) == Some(&stamp) {
                    continue;
                }
                observed.insert(path.clone(), stamp);
                match patch_new_eligible_file(&path) {
                    Ok(true) => {
                        if let Some(patched_stamp) = file_stamp(&path) {
                            observed.insert(path.clone(), patched_stamp);
                        }
                        crate::logger::log_msg(
                            "INFO",
                            "ChatF13",
                            &format!("已自动补齐新键位文件：{}", path.display()),
                        );
                    }
                    Ok(false) => {}
                    // Partial/incompatible files are left untouched and will
                    // be checked again when D2R changes their metadata.
                    Err(error) => crate::logger::log_msg(
                        "DEBUG",
                        "ChatF13",
                        &format!("暂未处理键位文件 {}：{error}", path.display()),
                    ),
                }
            }
            std::thread::sleep(Duration::from_millis(WATCH_POLL_MS));
        }
    });
}

fn ensure_d2r_closed() -> Result<(), String> {
    if d2r_running() {
        Err("请先关闭全部 D2R 窗口；游戏运行时会缓存并覆盖 .key/.keyo 键位文件".to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub fn get_room_rotation_chat_binding_status(
    state: tauri::State<'_, SharedState>,
) -> Result<ChatF13BindingStatus, String> {
    build_status(&state)
}

#[tauri::command]
pub fn install_room_rotation_chat_binding(
    state: tauri::State<'_, SharedState>,
) -> Result<ChatF13BindingStatus, String> {
    ensure_d2r_closed()?;
    let directories = configured_saved_games_directories(&state)?;
    let files = inspect_files(&directories)?;
    if files.is_empty() {
        return Err("存档目录中没有找到 .key/.keyo 键位文件".to_string());
    }
    let conflicts: Vec<String> = files
        .iter()
        .filter_map(|file| {
            file.state
                .as_ref()
                .err()
                .map(|error| format!("{}：{error}", file.path.display()))
        })
        .collect();
    if !conflicts.is_empty() {
        return Err(format!(
            "F13 安装预检失败，未修改任何文件：{}",
            conflicts.join("；")
        ));
    }

    let mut changed: Vec<(&Path, Vec<u8>)> = Vec::new();
    for file in &files {
        if file.state == Ok(BindingState::Installed) {
            continue;
        }
        create_backup(&file.path, &file.bytes)?;
        let patched = patch_f13(&file.bytes)?;
        if let Err(error) = write_file(&file.path, &patched) {
            let _ = write_file(&file.path, &file.bytes);
            for (path, original) in changed.iter().rev() {
                let _ = write_file(path, original);
            }
            return Err(format!("{error}；已尝试回滚本轮已修改文件"));
        }
        changed.push((&file.path, file.bytes.clone()));
    }
    set_auto_patch_enabled(&state, true)?;
    build_status(&state)
}

#[tauri::command]
pub fn restore_room_rotation_chat_binding(
    state: tauri::State<'_, SharedState>,
) -> Result<ChatF13BindingStatus, String> {
    ensure_d2r_closed()?;
    let directories = configured_saved_games_directories(&state)?;
    let files = inspect_files(&directories)?;
    let mut restore_plan = Vec::new();
    for file in &files {
        let backup = backup_path(&file.path)?;
        if !backup.is_file() {
            continue;
        }
        let backup_bytes = std::fs::read(&backup)
            .map_err(|error| format!("无法读取键位备份 {}：{error}", backup.display()))?;
        let restored = restore_chat_secondary_slot(&file.bytes, &backup_bytes)
            .map_err(|error| format!("无法恢复 {}：{error}", file.path.display()))?;
        restore_plan.push((file.path.clone(), file.bytes.clone(), restored, backup));
    }
    if restore_plan.is_empty() {
        return Err("没有找到由 D2RHub 创建的 F13 键位备份".to_string());
    }

    // Stop the watcher only after the restore has a valid plan. A failed
    // no-backup request must not silently disable future automatic patching.
    set_auto_patch_enabled(&state, false)?;

    let mut changed: Vec<(&Path, Vec<u8>)> = Vec::new();
    for (path, current, restored, _) in &restore_plan {
        if let Err(error) = write_file(path, restored) {
            let _ = write_file(path, current);
            for (changed_path, original) in changed.iter().rev() {
                let _ = write_file(changed_path, original);
            }
            let _ = set_auto_patch_enabled(&state, true);
            return Err(format!("{error}；已尝试回滚本轮已恢复文件"));
        }
        changed.push((path, current.clone()));
    }
    for (_, _, _, backup) in restore_plan {
        std::fs::remove_file(&backup)
            .map_err(|error| format!("键位已恢复，但无法删除备份 {}：{error}", backup.display()))?;
    }
    build_status(&state)
}

pub(crate) fn ensure_room_rotation_chat_binding_ready(state: &SharedState) -> Result<(), String> {
    let status = build_status(state)?;
    if status.ready {
        Ok(())
    } else {
        Err(format!(
            "F13 原生 Chat 备用键尚未就绪：{}。请在自动换房设置中关闭 D2R 后安装",
            status.message
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_key_file() -> Vec<u8> {
        let mut bytes = vec![0u8; KEY_FILE_HEADER_SIZE + 61 * KEY_RECORD_SIZE];
        write_u16(&mut bytes, 0, KEY_FILE_VERSION);
        for record in 0..61usize {
            let offset = KEY_FILE_HEADER_SIZE + record * KEY_RECORD_SIZE;
            write_u32(&mut bytes, offset, record as u32);
            write_u16(&mut bytes, offset + 4, UNBOUND_KEY);
            write_u32(&mut bytes, offset + 6, UNBOUND_KEY_TYPE);
            write_u32(&mut bytes, offset + 10, record as u32);
            write_u16(&mut bytes, offset + 14, UNBOUND_KEY);
            write_u32(&mut bytes, offset + 16, UNBOUND_KEY_TYPE);
        }
        write_u16(&mut bytes, CHAT_RECORD_OFFSET + 4, VK_RETURN);
        write_u32(&mut bytes, CHAT_RECORD_OFFSET + 6, BOUND_KEY_TYPE);
        bytes
    }

    #[test]
    fn patches_only_native_chat_secondary_slot() {
        let original = sample_key_file();
        assert_eq!(
            inspect_key_bytes(&original).unwrap(),
            BindingState::Eligible
        );
        let patched = patch_f13(&original).unwrap();
        assert_eq!(
            inspect_key_bytes(&patched).unwrap(),
            BindingState::Installed
        );
        let changed: Vec<usize> = original
            .iter()
            .zip(&patched)
            .enumerate()
            .filter_map(|(index, (before, after))| (before != after).then_some(index))
            .collect();
        assert_eq!(
            changed,
            vec![
                SECONDARY_SLOT_OFFSET + 4,
                SECONDARY_SLOT_OFFSET + 5,
                SECONDARY_SLOT_OFFSET + 6,
            ]
        );
    }

    #[test]
    fn rejects_an_existing_f13_collision() {
        let mut bytes = sample_key_file();
        let other_slot = KEY_FILE_HEADER_SIZE + 9 * KEY_RECORD_SIZE;
        write_u16(&mut bytes, other_slot + 4, VK_F13);
        write_u32(&mut bytes, other_slot + 6, BOUND_KEY_TYPE);
        assert!(inspect_key_bytes(&bytes).unwrap_err().contains("动作 9"));
    }

    #[test]
    fn accepts_a_stale_raw_key_value_when_the_slot_type_is_unbound() {
        let mut bytes = sample_key_file();
        write_u16(&mut bytes, SECONDARY_SLOT_OFFSET + 4, 0x1070);
        assert_eq!(inspect_key_bytes(&bytes).unwrap(), BindingState::Eligible);

        let patched = patch_f13(&bytes).unwrap();
        let restored = restore_chat_secondary_slot(&patched, &bytes).unwrap();
        assert_eq!(
            read_u16(&restored, SECONDARY_SLOT_OFFSET + 4).unwrap(),
            0x1070
        );
        assert_eq!(
            read_u32(&restored, SECONDARY_SLOT_OFFSET + 6).unwrap(),
            UNBOUND_KEY_TYPE
        );
    }

    #[test]
    fn restore_preserves_unrelated_changes() {
        let backup = sample_key_file();
        let mut current = patch_f13(&backup).unwrap();
        let unrelated = KEY_FILE_HEADER_SIZE + 12 * KEY_RECORD_SIZE;
        write_u16(&mut current, unrelated + 4, 0x41);
        write_u32(&mut current, unrelated + 6, BOUND_KEY_TYPE);
        let restored = restore_chat_secondary_slot(&current, &backup).unwrap();
        assert_eq!(
            inspect_key_bytes(&restored).unwrap(),
            BindingState::Eligible
        );
        assert_eq!(read_u16(&restored, unrelated + 4).unwrap(), 0x41);
    }

    #[test]
    fn newly_created_key_file_is_backed_up_patched_and_verified() {
        let directory = std::env::temp_dir().join(format!(
            "d2rhub_chat_f13_watch_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("new-character.keyo");
        let original = sample_key_file();
        std::fs::write(&path, &original).unwrap();

        assert!(patch_new_eligible_file(&path).unwrap());
        assert_eq!(
            inspect_key_bytes(&std::fs::read(&path).unwrap()).unwrap(),
            BindingState::Installed
        );
        assert_eq!(
            std::fs::read(backup_path(&path).unwrap()).unwrap(),
            original
        );
        assert!(!patch_new_eligible_file(&path).unwrap());

        std::fs::remove_dir_all(directory).unwrap();
    }
}
