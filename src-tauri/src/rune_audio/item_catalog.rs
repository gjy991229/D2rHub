use super::catalog::MAX_ITEM_ID;
use super::protocol::PROTOCOL_VERSION;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub const ITEM_CATALOG_FILE_NAME: &str = "audio-telemetry-item-catalog.json";
pub const LEGACY_ITEM_CATALOG_FILE_NAME: &str = "d2rhub-audio-item-catalog.json";
pub const CATEGORY_RUNES: &str = "runes";
pub const CATEGORY_GEMS: &str = "gems";
pub const CATEGORY_CHARMS: &str = "charms";
pub const CATEGORY_JEWELS: &str = "jewels";
pub const CATEGORY_KEYS: &str = "keys";
pub const CATEGORY_ORGANS: &str = "organs";
pub const CATEGORY_ESSENCES: &str = "essences";

pub const SUPPORTED_TRACKING_CATEGORIES: [&str; 7] = [
    CATEGORY_RUNES,
    CATEGORY_GEMS,
    CATEGORY_CHARMS,
    CATEGORY_JEWELS,
    CATEGORY_KEYS,
    CATEGORY_ORGANS,
    CATEGORY_ESSENCES,
];

pub const SUPPORTED_CHARM_CODES: [&str; 3] = ["cm1", "cm2", "cm3"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedItemDefinition {
    pub item_id: u32,
    pub code: &'static str,
    pub category: &'static str,
    pub fallback_name: &'static str,
    pub fallback_name_en: &'static str,
}

macro_rules! item {
    ($id:literal, $code:literal, $category:ident, $zh:literal, $en:literal) => {
        SupportedItemDefinition {
            item_id: $id,
            code: $code,
            category: $category,
            fallback_name: $zh,
            fallback_name_en: $en,
        }
    };
}

/// Stable IDs are part of protocol v7. Never reorder or reuse an ID.
pub const SUPPORTED_ITEMS: [SupportedItemDefinition; 50] = [
    item!(1, "gcv", CATEGORY_GEMS, "碎裂的紫宝石", "Chipped Amethyst"),
    item!(2, "gfv", CATEGORY_GEMS, "裂开的紫宝石", "Flawed Amethyst"),
    item!(3, "gsv", CATEGORY_GEMS, "紫宝石", "Amethyst"),
    item!(
        4,
        "gzv",
        CATEGORY_GEMS,
        "无瑕疵的紫宝石",
        "Flawless Amethyst"
    ),
    item!(5, "gpv", CATEGORY_GEMS, "完美的紫宝石", "Perfect Amethyst"),
    item!(6, "gcy", CATEGORY_GEMS, "碎裂的黄宝石", "Chipped Topaz"),
    item!(7, "gfy", CATEGORY_GEMS, "裂开的黄宝石", "Flawed Topaz"),
    item!(8, "gsy", CATEGORY_GEMS, "黄宝石", "Topaz"),
    item!(9, "gly", CATEGORY_GEMS, "无瑕疵的黄宝石", "Flawless Topaz"),
    item!(10, "gpy", CATEGORY_GEMS, "完美的黄宝石", "Perfect Topaz"),
    item!(11, "gcb", CATEGORY_GEMS, "碎裂的蓝宝石", "Chipped Sapphire"),
    item!(12, "gfb", CATEGORY_GEMS, "裂开的蓝宝石", "Flawed Sapphire"),
    item!(13, "gsb", CATEGORY_GEMS, "蓝宝石", "Sapphire"),
    item!(
        14,
        "glb",
        CATEGORY_GEMS,
        "无瑕疵的蓝宝石",
        "Flawless Sapphire"
    ),
    item!(15, "gpb", CATEGORY_GEMS, "完美的蓝宝石", "Perfect Sapphire"),
    item!(16, "gcg", CATEGORY_GEMS, "碎裂的绿宝石", "Chipped Emerald"),
    item!(17, "gfg", CATEGORY_GEMS, "裂开的绿宝石", "Flawed Emerald"),
    item!(18, "gsg", CATEGORY_GEMS, "绿宝石", "Emerald"),
    item!(
        19,
        "glg",
        CATEGORY_GEMS,
        "无瑕疵的绿宝石",
        "Flawless Emerald"
    ),
    item!(20, "gpg", CATEGORY_GEMS, "完美的绿宝石", "Perfect Emerald"),
    item!(21, "gcr", CATEGORY_GEMS, "碎裂的红宝石", "Chipped Ruby"),
    item!(22, "gfr", CATEGORY_GEMS, "裂开的红宝石", "Flawed Ruby"),
    item!(23, "gsr", CATEGORY_GEMS, "红宝石", "Ruby"),
    item!(24, "glr", CATEGORY_GEMS, "无瑕疵的红宝石", "Flawless Ruby"),
    item!(25, "gpr", CATEGORY_GEMS, "完美的红宝石", "Perfect Ruby"),
    item!(26, "gcw", CATEGORY_GEMS, "碎裂的钻石", "Chipped Diamond"),
    item!(27, "gfw", CATEGORY_GEMS, "裂开的钻石", "Flawed Diamond"),
    item!(28, "gsw", CATEGORY_GEMS, "钻石", "Diamond"),
    item!(29, "glw", CATEGORY_GEMS, "无瑕疵的钻石", "Flawless Diamond"),
    item!(30, "gpw", CATEGORY_GEMS, "完美的钻石", "Perfect Diamond"),
    item!(31, "skc", CATEGORY_GEMS, "碎裂的骷髅", "Chipped Skull"),
    item!(32, "skf", CATEGORY_GEMS, "裂开的骷髅", "Flawed Skull"),
    item!(33, "sku", CATEGORY_GEMS, "骷髅", "Skull"),
    item!(34, "skl", CATEGORY_GEMS, "无瑕疵的骷髅", "Flawless Skull"),
    item!(35, "skz", CATEGORY_GEMS, "完美的骷髅", "Perfect Skull"),
    item!(36, "cm1", CATEGORY_CHARMS, "小型护身符", "Small Charm"),
    item!(37, "cm2", CATEGORY_CHARMS, "大型护身符", "Large Charm"),
    item!(38, "cm3", CATEGORY_CHARMS, "超大型护身符", "Grand Charm"),
    item!(39, "jew", CATEGORY_JEWELS, "珠宝", "Jewel"),
    item!(40, "pk1", CATEGORY_KEYS, "恐惧之钥", "Key of Terror"),
    item!(41, "pk2", CATEGORY_KEYS, "憎恨之钥", "Key of Hate"),
    item!(42, "pk3", CATEGORY_KEYS, "毁灭之钥", "Key of Destruction"),
    item!(43, "dhn", CATEGORY_ORGANS, "迪亚波罗的角", "Diablo's Horn"),
    item!(44, "bey", CATEGORY_ORGANS, "巴尔之眼", "Baal's Eye"),
    item!(
        45,
        "mbr",
        CATEGORY_ORGANS,
        "墨菲斯托之脑",
        "Mephisto's Brain"
    ),
    item!(
        46,
        "toa",
        CATEGORY_ESSENCES,
        "赦免徽章",
        "Token of Absolution"
    ),
    item!(
        47,
        "tes",
        CATEGORY_ESSENCES,
        "扭曲的痛苦精华",
        "Twisted Essence of Suffering"
    ),
    item!(
        48,
        "ceh",
        CATEGORY_ESSENCES,
        "充盈的憎恨精华",
        "Charged Essence of Hatred"
    ),
    item!(
        49,
        "bet",
        CATEGORY_ESSENCES,
        "燃烧的恐惧精华",
        "Burning Essence of Terror"
    ),
    item!(
        50,
        "fed",
        CATEGORY_ESSENCES,
        "溃烂的毁灭精华",
        "Festering Essence of Destruction"
    ),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemCatalogEntry {
    pub item_id: u32,
    pub code: String,
    pub category: String,
    pub name: String,
    pub name_en: String,
    pub asset: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemCatalogFile {
    pub protocol_version: u8,
    pub source_items: String,
    pub items: Vec<ItemCatalogEntry>,
}

#[derive(Debug, Clone)]
pub struct ItemCatalog {
    by_id: HashMap<u32, ItemCatalogEntry>,
}

impl Default for ItemCatalog {
    fn default() -> Self {
        Self::builtin()
    }
}

impl ItemCatalog {
    pub fn builtin() -> Self {
        let by_id = SUPPORTED_ITEMS
            .into_iter()
            .map(|item| {
                (
                    item.item_id,
                    ItemCatalogEntry {
                        item_id: item.item_id,
                        code: item.code.to_string(),
                        category: item.category.to_string(),
                        name: item.fallback_name.to_string(),
                        name_en: item.fallback_name_en.to_string(),
                        asset: String::new(),
                    },
                )
            })
            .collect();
        Self { by_id }
    }

    pub fn load_from_directory(directory: &Path) -> Result<Self, String> {
        let path = [ITEM_CATALOG_FILE_NAME, LEGACY_ITEM_CATALOG_FILE_NAME]
            .into_iter()
            .map(|name| directory.join(name))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| format!("目录中没有协议物品清单: {}", directory.display()))?;
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("读取物品协议清单失败 {}: {error}", path.display()))?;
        let file = serde_json::from_slice::<ItemCatalogFile>(&bytes)
            .map_err(|error| format!("解析物品协议清单失败 {}: {error}", path.display()))?;
        if file.protocol_version != PROTOCOL_VERSION {
            return Err(format!(
                "物品清单协议版本 {} 与接收端 v{} 不兼容: {}",
                file.protocol_version,
                PROTOCOL_VERSION,
                path.display()
            ));
        }
        let mut catalog = Self::builtin();
        for entry in file.items.into_iter().filter(|entry| {
            (1..=MAX_ITEM_ID).contains(&entry.item_id)
                && !entry.code.trim().is_empty()
                && is_supported_item_category(&entry.category)
        }) {
            catalog.by_id.insert(entry.item_id, entry);
        }
        Ok(catalog)
    }

    pub fn resolve(&self, item_id: u32) -> Option<&ItemCatalogEntry> {
        self.by_id.get(&item_id)
    }
}

pub fn default_tracked_categories() -> Vec<String> {
    SUPPORTED_TRACKING_CATEGORIES
        .into_iter()
        .map(str::to_string)
        .collect()
}

pub fn default_tracked_charm_codes() -> Vec<String> {
    SUPPORTED_CHARM_CODES
        .into_iter()
        .map(str::to_string)
        .collect()
}

pub fn normalize_tracked_categories(categories: &[String]) -> Vec<String> {
    let requested = categories
        .iter()
        .map(|category| category.trim().to_ascii_lowercase())
        .collect::<HashSet<_>>();
    SUPPORTED_TRACKING_CATEGORIES
        .into_iter()
        .filter(|category| requested.contains(*category))
        .map(str::to_string)
        .collect()
}

pub fn normalize_tracked_charm_codes(codes: &[String]) -> Vec<String> {
    let requested = codes
        .iter()
        .map(|code| code.trim().to_ascii_lowercase())
        .collect::<HashSet<_>>();
    SUPPORTED_CHARM_CODES
        .into_iter()
        .filter(|code| requested.contains(*code))
        .map(str::to_string)
        .collect()
}

/// Returns the protocol gem quality (1=chipped, 5=perfect).
pub fn gem_quality_level(code: &str) -> Option<u32> {
    match code.trim().to_ascii_lowercase().as_str() {
        "gcv" | "gcy" | "gcb" | "gcg" | "gcr" | "gcw" | "skc" => Some(1),
        "gfv" | "gfy" | "gfb" | "gfg" | "gfr" | "gfw" | "skf" => Some(2),
        "gsv" | "gsy" | "gsb" | "gsg" | "gsr" | "gsw" | "sku" => Some(3),
        "gzv" | "gly" | "glb" | "glg" | "glr" | "glw" | "skl" => Some(4),
        "gpv" | "gpy" | "gpb" | "gpg" | "gpr" | "gpw" | "skz" => Some(5),
        _ => None,
    }
}

pub fn is_supported_item_category(category: &str) -> bool {
    matches!(
        category,
        CATEGORY_GEMS
            | CATEGORY_CHARMS
            | CATEGORY_JEWELS
            | CATEGORY_KEYS
            | CATEGORY_ORGANS
            | CATEGORY_ESSENCES
    )
}

pub fn selected_item_definitions(categories: &[String]) -> Vec<SupportedItemDefinition> {
    let selected = normalize_tracked_categories(categories)
        .into_iter()
        .collect::<HashSet<_>>();
    SUPPORTED_ITEMS
        .into_iter()
        .filter(|item| selected.contains(item.category))
        .collect()
}

pub fn catalog_file(source_items: String, entries: Vec<ItemCatalogEntry>) -> ItemCatalogFile {
    ItemCatalogFile {
        protocol_version: PROTOCOL_VERSION,
        source_items,
        items: entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_item_ids_are_unique_and_fit_the_protocol() {
        let ids = SUPPORTED_ITEMS
            .iter()
            .map(|item| item.item_id)
            .collect::<HashSet<_>>();
        let codes = SUPPORTED_ITEMS
            .iter()
            .map(|item| item.code)
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), SUPPORTED_ITEMS.len());
        assert_eq!(codes.len(), SUPPORTED_ITEMS.len());
        assert!(SUPPORTED_ITEMS
            .iter()
            .all(|item| (1..=MAX_ITEM_ID).contains(&item.item_id)));
    }

    #[test]
    fn category_selection_keeps_protocol_ids_stable() {
        let selected = selected_item_definitions(&[CATEGORY_KEYS.to_string()]);
        assert_eq!(
            selected.iter().map(|item| item.item_id).collect::<Vec<_>>(),
            vec![40, 41, 42]
        );
    }

    #[test]
    fn normalization_rejects_unknown_and_uses_ui_order() {
        let normalized = normalize_tracked_categories(&[
            CATEGORY_KEYS.to_string(),
            "unknown".to_string(),
            CATEGORY_RUNES.to_string(),
            CATEGORY_KEYS.to_string(),
        ]);
        assert_eq!(normalized, [CATEGORY_RUNES, CATEGORY_KEYS]);
    }

    #[test]
    fn charm_selection_is_normalized_in_catalog_order() {
        let normalized = normalize_tracked_charm_codes(&[
            " CM3 ".to_string(),
            "unknown".to_string(),
            "cm1".to_string(),
            "CM3".to_string(),
        ]);
        assert_eq!(normalized, ["cm1", "cm3"]);
    }

    #[test]
    fn every_supported_gem_has_the_expected_quality_level() {
        let levels = SUPPORTED_ITEMS
            .iter()
            .filter(|item| item.category == CATEGORY_GEMS)
            .map(|item| gem_quality_level(item.code).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(levels.len(), 35);
        for level in 1..=5 {
            assert_eq!(levels.iter().filter(|value| **value == level).count(), 7);
        }
        assert_eq!(gem_quality_level("cm1"), None);
    }
}
