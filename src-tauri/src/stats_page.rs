use std::path::{Path, PathBuf};

fn normalize_stats_theme(theme: &str) -> &'static str {
    if theme == "light" {
        "light"
    } else {
        "onyx"
    }
}

pub(crate) fn stats_template_candidates(
    resource_dir: &Path,
    dev_path: &Path,
    prefer_dev: bool,
) -> Vec<PathBuf> {
    let resource_path = resource_dir.join("docs").join("stats.html");
    let nsis_path = resource_dir.join("_up_").join("docs").join("stats.html");

    if prefer_dev {
        vec![dev_path.to_path_buf(), resource_path, nsis_path]
    } else {
        vec![resource_path, nsis_path, dev_path.to_path_buf()]
    }
}

pub(crate) fn render_stats_template(
    template: &str,
    stats_json: &str,
    api_port: u16,
    api_token: &str,
    theme: &str,
) -> String {
    template
        .replace("{{STATS_DATA}}", stats_json)
        .replace("{{STATS_API_PORT}}", &api_port.to_string())
        .replace("{{STATS_API_TOKEN}}", api_token)
        .replace("{{STATS_THEME}}", normalize_stats_theme(theme))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_app_theme_is_injected_without_a_stale_local_override() {
        let template = include_str!("../../docs/stats.html");
        let rendered = render_stats_template(template, "[]", 43123, "test-token", "light");

        assert!(rendered.contains("<html lang=\"zh-CN\" data-theme=\"light\">"));
        assert!(rendered.contains("const API_PORT=43123"));
        assert!(rendered.contains("const API_TOKEN=\"test-token\""));
        assert!(!rendered.contains("{{STATS_API_PORT}}"));
        assert!(!rendered.contains("{{STATS_API_TOKEN}}"));
        assert!(!rendered.contains("{{STATS_THEME}}"));
        assert!(!rendered.contains("d2rhub-stats-theme"));
        assert!(rendered.contains("X-D2RHub-Stats-Token"));
        assert!(rendered.contains("apiFetch(\"/api/records\")"));
        assert!(rendered.contains("id=\"batch-toggle\""));
        assert!(rendered.contains("/api/records/batch"));
        assert!(rendered.contains("id=\"filter-outliers\""));
        assert!(rendered.contains("function optimizeOutlierRecords"));
        assert!(rendered.contains("durations.length<10"));
        assert!(rendered.contains("seconds>average*10||seconds<average*.1"));
        assert!(rendered.contains("_stats_timer_seconds"));
    }

    #[test]
    fn unknown_app_theme_falls_back_to_onyx() {
        assert_eq!(normalize_stats_theme("onyx"), "onyx");
        assert_eq!(normalize_stats_theme("unexpected"), "onyx");
    }

    #[test]
    fn debug_template_candidates_prefer_the_workspace_source() {
        let resource_dir = Path::new("target/debug");
        let dev_path = Path::new("docs/stats.html");
        let candidates = stats_template_candidates(resource_dir, dev_path, true);

        assert_eq!(candidates[0], dev_path);
        assert_eq!(candidates[1], resource_dir.join("docs").join("stats.html"));
        assert_eq!(
            candidates[2],
            resource_dir.join("_up_").join("docs").join("stats.html")
        );
    }
}
