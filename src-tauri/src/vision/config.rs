use crate::infrastructure::module_config::ModuleConfigStore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Roi {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Roi {
    pub fn valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite())
            && self.x >= 0.0
            && self.y >= 0.0
            && self.width >= 0.01
            && self.height >= 0.01
            && self.x + self.width <= 1.001
            && self.y + self.height <= 1.001
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VisionConfig {
    pub diagnostic_enabled: bool,
    pub chat_send: bool,
    pub chat_follow: bool,
    /// Legacy serialized preference; listening now follows the room configuration dynamically.
    pub chat_account_id: String,
    pub chat_roi: Roi,
    pub poll_ms: u64,
    pub area_enabled: bool,
    pub area_roi: Roi,
}
impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            diagnostic_enabled: false,
            chat_send: false,
            chat_follow: false,
            chat_account_id: String::new(),
            chat_roi: Roi {
                x: 0.0,
                y: 0.4,
                width: 0.31,
                height: 0.4,
            },
            poll_ms: 2000,
            area_enabled: true,
            area_roi: Roi {
                x: 0.4,
                y: 0.2,
                width: 0.2,
                height: 0.09,
            },
        }
    }
}
impl VisionConfig {
    pub fn only_diagnostics_changed(&self, other: &Self) -> bool {
        let mut runtime = self.clone();
        runtime.diagnostic_enabled = other.diagnostic_enabled;
        self.diagnostic_enabled != other.diagnostic_enabled && runtime == *other
    }
    pub fn needs_ocr(&self) -> bool {
        self.chat_follow
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.chat_roi.valid() || !self.area_roi.valid() {
            return Err("截图区域必须位于游戏客户区内（0～1 比例）".into());
        }
        if !(1000..=10000).contains(&self.poll_ms) {
            return Err("视觉识别周期或阈值无效".into());
        }
        Ok(())
    }
}
#[derive(Clone, Serialize)]
pub struct ConfigSnapshot {
    pub generation: u64,
    pub config: VisionConfig,
}
pub fn store(root: &str) -> Result<ModuleConfigStore, String> {
    ModuleConfigStore::new(root, "visual-recognition", 1).map_err(|e| e.to_string())
}
pub fn load(store: &ModuleConfigStore) -> Result<ConfigSnapshot, String> {
    let value = store.load::<VisionConfig>().map_err(|e| e.to_string())?;
    Ok(value
        .map(|v| ConfigSnapshot {
            generation: v.generation,
            config: {
                let mut config = v.payload;
                if config.area_roi
                    == (Roi {
                        x: 0.4,
                        y: 0.2,
                        width: 0.2,
                        height: 0.04,
                    })
                {
                    config.area_roi.height = 0.09;
                }
                config
            },
        })
        .unwrap_or(ConfigSnapshot {
            generation: 0,
            config: VisionConfig::default(),
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_defaults_match_calibrated_regions_and_diagnostics_are_opt_in() {
        let c = VisionConfig::default();
        assert_eq!(c.poll_ms, 2000);
        assert_eq!(
            c.chat_roi,
            Roi {
                x: 0.0,
                y: 0.4,
                width: 0.31,
                height: 0.4
            }
        );
        assert_eq!(
            c.area_roi,
            Roi {
                x: 0.4,
                y: 0.2,
                width: 0.2,
                height: 0.09
            }
        );
        assert!(!c.diagnostic_enabled);
        assert!(c.area_enabled);
        c.validate().unwrap();
        let enabled = VisionConfig {
            chat_follow: true,
            ..c.clone()
        };
        enabled.validate().unwrap(); // Target is selected from room settings, not this legacy field.
        let mut logging = c.clone();
        logging.diagnostic_enabled = true;
        assert!(c.only_diagnostics_changed(&logging));
        logging.area_enabled = false;
        assert!(!c.only_diagnostics_changed(&logging));
        assert!(!c.only_diagnostics_changed(&c)); // Explicit retry still restarts resources.
        let customized: VisionConfig =
            serde_json::from_str(r#"{"chat_roi":{"x":0.1,"y":0.2,"width":0.3,"height":0.4}}"#)
                .unwrap();
        assert_eq!(customized.chat_roi.x, 0.1);
    }
    #[test]
    fn diagnostic_and_feature_switches_survive_store_reopen() {
        let root =
            std::env::temp_dir().join(format!("vision-config-test-{}", uuid::Uuid::new_v4()));
        let config = VisionConfig {
            diagnostic_enabled: true,
            chat_send: true,
            area_enabled: true,

            ..Default::default()
        };
        let first = store(root.to_str().unwrap()).unwrap();
        first.save_if_generation(0, config.clone()).unwrap();
        drop(first);
        let reopened = store(root.to_str().unwrap()).unwrap();
        let snapshot = load(&reopened).unwrap();
        assert_eq!(snapshot.generation, 1);
        assert_eq!(
            serde_json::to_value(snapshot.config).unwrap(),
            serde_json::to_value(config).unwrap()
        );
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn chat_listener_requires_ocr_but_sender_does_not() {
        let mut config = VisionConfig {
            chat_send: true,
            chat_follow: true,
            area_enabled: false,

            chat_account_id: "follower".into(),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
        assert!(config.needs_ocr());
        config.chat_follow = false;
        assert!(!config.needs_ocr());
        config.area_enabled = true;
        assert!(!config.needs_ocr());
    }
    #[test]
    fn missing_preferences_enable_statistics_but_keep_chat_and_diagnostics_off() {
        let c: VisionConfig = serde_json::from_str("{}").unwrap();
        assert!(!c.diagnostic_enabled);
        assert!(!c.needs_ocr());
        assert!(c.area_enabled);
        assert!(!c.chat_send);
        assert!(c.validate().is_ok());
    }
    #[test]
    fn explicitly_disabled_preferences_survive_defaults_and_store_reopen() {
        let config: VisionConfig = serde_json::from_str(
            r#"{"area_enabled":false,"drops_enabled":false,"drops_enforce":false,"labels_visible":false}"#,
        ).unwrap();
        assert!(!config.needs_ocr());
        assert!(config.validate().is_ok());
        let root = std::env::temp_dir().join(format!("vision-disabled-{}", uuid::Uuid::new_v4()));
        let first = store(root.to_str().unwrap()).unwrap();
        first.save_if_generation(0, config.clone()).unwrap();
        drop(first);
        let reopened = store(root.to_str().unwrap()).unwrap();
        assert_eq!(load(&reopened).unwrap().config, config);
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rejects_invalid_capture() {
        let mut c = VisionConfig::default();
        c.area_roi.x = f32::NAN;
        assert!(c.validate().is_err());
        c = VisionConfig::default();
        c.poll_ms = 1000;
        assert!(c.validate().is_ok());
    }
    #[test]
    fn obsolete_drop_options_cannot_enable_ocr_or_block_recording() {
        let config: VisionConfig = serde_json::from_str(r#"{"area_enabled":false,"drops_enabled":true,"drops_enforce":true,"labels_visible":false,"drop_threshold":99}"#).unwrap();
        assert!(!config.needs_ocr());
        config.validate().unwrap();
        let saved = serde_json::to_value(config).unwrap();
        assert!(saved.get("drops_enforce").is_none());
    }
}
