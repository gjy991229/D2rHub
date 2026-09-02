use std::cmp::Reverse;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const MAX_LOG_FILES: usize = 3;
const MAX_LOG_BYTES_PER_FILE: u64 = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticReport {
    pub schema_version: u32,
    pub generated_at: String,
    pub app_version: String,
    pub platform: String,
    pub architecture: String,
    pub configuration: Value,
    pub capabilities: Value,
    pub tasks: Value,
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticRedaction {
    replacements: Vec<(String, String)>,
}

impl DiagnosticRedaction {
    pub fn add(&mut self, sensitive: impl Into<String>, replacement: impl Into<String>) {
        let sensitive = sensitive.into();
        if sensitive.trim().len() >= 3 {
            self.replacements.push((sensitive, replacement.into()));
            self.replacements
                .sort_by_key(|replacement| Reverse(replacement.0.len()));
        }
    }

    pub fn redact_text(&self, text: &str) -> String {
        text.lines()
            .map(|line| {
                if contains_sensitive_marker(line) {
                    "[sensitive credential line omitted]".to_string()
                } else {
                    self.replacements
                        .iter()
                        .fold(line.to_string(), |current, pair| {
                            replace_ascii_case_insensitive(&current, &pair.0, &pair.1)
                        })
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn redact_json(&self, value: &mut Value) {
        match value {
            Value::String(text) => *text = self.redact_text(text),
            Value::Array(values) => {
                for value in values {
                    self.redact_json(value);
                }
            }
            Value::Object(values) => {
                for value in values.values_mut() {
                    self.redact_json(value);
                }
            }
            _ => {}
        }
    }
}

pub fn create_diagnostic_bundle(
    output_directory: &Path,
    logs_directory: Option<&Path>,
    mut report: DiagnosticReport,
    redaction: &DiagnosticRedaction,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(output_directory).map_err(|error| {
        format!(
            "创建诊断包目录 {} 失败: {error}",
            output_directory.display()
        )
    })?;
    redaction.redact_json(&mut report.configuration);
    redaction.redact_json(&mut report.capabilities);
    redaction.redact_json(&mut report.tasks);

    let file_name = format!(
        "D2RHub-diagnostics-{}-{}.zip",
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        uuid::Uuid::new_v4().simple()
    );
    let output_path = output_directory.join(file_name);
    let file = File::create(&output_path)
        .map_err(|error| format!("创建诊断包 {} 失败: {error}", output_path.display()))?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    archive
        .start_file("report.json", options)
        .map_err(|error| format!("创建诊断报告条目失败: {error}"))?;
    let report_bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("序列化诊断报告失败: {error}"))?;
    archive
        .write_all(&report_bytes)
        .map_err(|error| format!("写入诊断报告失败: {error}"))?;

    if let Some(logs_directory) = logs_directory {
        let redacted_logs = collect_recent_logs(logs_directory, redaction)?;
        if !redacted_logs.is_empty() {
            archive
                .start_file("logs/recent.log", options)
                .map_err(|error| format!("创建脱敏日志条目失败: {error}"))?;
            archive
                .write_all(redacted_logs.as_bytes())
                .map_err(|error| format!("写入脱敏日志失败: {error}"))?;
        }
    }

    let mut file = archive
        .finish()
        .map_err(|error| format!("完成诊断包失败: {error}"))?;
    file.flush()
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("同步诊断包 {} 失败: {error}", output_path.display()))?;
    Ok(output_path)
}

fn collect_recent_logs(
    logs_directory: &Path,
    redaction: &DiagnosticRedaction,
) -> Result<String, String> {
    let mut logs = std::fs::read_dir(logs_directory)
        .map_err(|error| format!("读取日志目录 {} 失败: {error}", logs_directory.display()))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path).ok()?;
            (metadata.file_type().is_file()
                && path.extension().is_some_and(|extension| extension == "log"))
            .then_some((path, metadata.modified().ok()))
        })
        .collect::<Vec<_>>();
    logs.sort_by_key(|(_, modified)| *modified);
    logs.reverse();

    let mut output = String::new();
    for (path, _) in logs.into_iter().take(MAX_LOG_FILES) {
        let mut bytes = Vec::new();
        File::open(&path)
            .and_then(|mut file| {
                let length = file.metadata()?.len();
                let tail_length = length.min(MAX_LOG_BYTES_PER_FILE);
                file.seek(SeekFrom::Start(length.saturating_sub(tail_length)))?;
                file.take(tail_length).read_to_end(&mut bytes)
            })
            .map_err(|error| format!("读取日志 {} 失败: {error}", path.display()))?;
        let content = String::from_utf8_lossy(&bytes);
        output.push_str(&format!(
            "\n===== {} =====\n",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("d2rhub.log")
        ));
        output.push_str(&redaction.redact_text(&content));
        output.push('\n');
    }
    Ok(output)
}

fn contains_sensitive_marker(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "token",
        "credential",
        "authorization",
        "web_token",
        "st=",
        "password",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn replace_ascii_case_insensitive(source: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return source.to_string();
    }
    let source_lower = source.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    while let Some(relative) = source_lower[cursor..].find(&needle_lower) {
        let start = cursor + relative;
        output.push_str(&source[cursor..start]);
        output.push_str(replacement);
        cursor = start + needle.len();
    }
    output.push_str(&source[cursor..]);
    output
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::{create_diagnostic_bundle, DiagnosticRedaction, DiagnosticReport};

    #[test]
    fn redaction_removes_credentials_paths_and_account_identifiers() {
        let mut redaction = DiagnosticRedaction::default();
        redaction.add(r"C:\Users\Alice", "[user-profile]");
        redaction.add("account-secret", "[account]");
        let source = "opening C:\\Users\\Alice\\D2R\nAccount account-secret ready\nToken=secret";
        let redacted = redaction.redact_text(source);

        assert!(redacted.contains("[user-profile]"));
        assert!(redacted.contains("[account]"));
        assert!(redacted.contains("[sensitive credential line omitted]"));
        assert!(!redacted.contains("Alice"));
        assert!(!redacted.contains("account-secret"));
        assert!(!redacted.contains("Token=secret"));
    }

    #[test]
    fn bundle_contains_only_redacted_report_and_recent_log_text() {
        let root = std::env::temp_dir().join(format!(
            "d2rhub_diagnostics_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let logs = root.join("logs");
        let output = root.join("output");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(
            logs.join("current.log"),
            "account-secret opened C:\\Users\\Alice\\D2R\npassword=hunter2",
        )
        .unwrap();
        let mut redaction = DiagnosticRedaction::default();
        redaction.add(r"C:\Users\Alice", "[user-profile]");
        redaction.add("account-secret", "[account]");
        let report = DiagnosticReport {
            schema_version: 1,
            generated_at: "2026-09-01T00:00:00Z".to_string(),
            app_version: "0.9.8".to_string(),
            platform: "windows".to_string(),
            architecture: "x86_64".to_string(),
            configuration: serde_json::json!({"path": r"C:\Users\Alice\D2R"}),
            capabilities: serde_json::json!([]),
            tasks: serde_json::json!([{"subject": "account-secret"}]),
        };

        let bundle = create_diagnostic_bundle(&output, Some(&logs), report, &redaction).unwrap();
        let file = std::fs::File::open(bundle).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut combined = String::new();
        archive
            .by_name("report.json")
            .unwrap()
            .read_to_string(&mut combined)
            .unwrap();
        archive
            .by_name("logs/recent.log")
            .unwrap()
            .read_to_string(&mut combined)
            .unwrap();

        assert!(combined.contains("[user-profile]"));
        assert!(combined.contains("[account]"));
        assert!(!combined.contains("Alice"));
        assert!(!combined.contains("account-secret"));
        assert!(!combined.contains("hunter2"));
        let _ = std::fs::remove_dir_all(root);
    }
}
