/// 符文数据 — 全部从 crate::ocr::game_data 派生，保证全项目统一。
use std::sync::LazyLock;

/// 符文标准名称（简体中文，按编号 1-33 排列）
/// 与前端 src/store/stats.ts 保持同步
static RUNE_NAMES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut entries: Vec<(u32, &'static str)> = crate::ocr::game_data::RUNE_NAME_MAP
        .iter()
        .map(|(name, info)| (info.level, *name))
        .collect();
    entries.sort_by_key(|k| k.0);
    entries.into_iter().map(|(_, name)| name).collect()
});

/// 高级符文阈值（#24 伊斯特 及以上）
pub const HIGH_RUNE_THRESHOLD: u32 = 24;

/// 从 OCR 文本中匹配符文名称或编号，返回 1-based 编号（1-33）
///
/// 匹配策略（按优先级）：
/// 1. 遍历 ALL_RUNE_ALIASES（按长度降序，标准名→方言→英文→繁体→短称），
///    文本包含别名即返回对应编号。长度降序保证"伊斯特"优先于"伊司"。
/// 2. 若仍未找到，遍历文本中的数字 1-33，返回最后一个有效数字。
pub fn get_rune_number(text: &str) -> Option<u32> {
    let sanitized = text.trim();
    if sanitized.is_empty() {
        return None;
    }

    let lower = sanitized.to_lowercase();

    // 1. 别名表查找（已按长度降序，最长优先）
    for &(alias, level) in crate::ocr::game_data::ALL_RUNE_ALIASES.iter() {
        if lower.contains(alias) {
            return Some(level);
        }
    }

    // 2. 数字回退 1-33
    let mut valid_numbers = Vec::new();
    for part in sanitized.split(|c: char| !c.is_ascii_digit()) {
        if let Ok(n) = part.parse::<u32>() {
            if (1..=33).contains(&n) {
                valid_numbers.push(n);
            }
        }
    }
    valid_numbers.pop()
}

/// 判断是否为高级符文（#24 及以上）
#[inline]
pub fn is_high_rune(number: u32) -> bool {
    number >= HIGH_RUNE_THRESHOLD
}

/// 根据编号获取符文标准名称（1-based）
pub fn get_rune_name(number: u32) -> Option<&'static str> {
    if (1..=33).contains(&number) {
        Some(RUNE_NAMES[(number - 1) as usize])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rune_names_count() {
        assert_eq!(RUNE_NAMES.len(), 33);
    }

    #[test]
    fn test_get_rune_number_standard() {
        assert_eq!(get_rune_number("艾尔"), Some(1));
        assert_eq!(get_rune_number("伊斯特"), Some(24));
        assert_eq!(get_rune_number("萨德"), Some(33));
        assert_eq!(get_rune_number("贝"), Some(30));
        assert_eq!(get_rune_number("乔"), Some(31));
        assert_eq!(get_rune_number("那夫"), Some(4));
        assert_eq!(get_rune_number("特尔"), Some(3));
    }

    #[test]
    fn test_get_rune_number_aliases() {
        // 方言别名（旧 rune_data::RUNE_NAMES）
        assert_eq!(get_rune_number("提尔"), Some(3));
        assert_eq!(get_rune_number("奈夫"), Some(4));
        assert_eq!(get_rune_number("沙伊"), Some(13));
        assert_eq!(get_rune_number("扎哈"), Some(31));
        assert_eq!(get_rune_number("佐德"), Some(33));
        assert_eq!(get_rune_number("图尔"), Some(10));
        assert_eq!(get_rune_number("兰姆"), Some(20));
        assert_eq!(get_rune_number("玛尔"), Some(23));

        // 短称
        assert_eq!(get_rune_number("扎"), Some(31));

        // 英文
        assert_eq!(get_rune_number("Nef"), Some(4));
        assert_eq!(get_rune_number("ist"), Some(24));
        assert_eq!(get_rune_number("Jah"), Some(31));
        assert_eq!(get_rune_number("zod"), Some(33));

        // 繁体
        assert_eq!(get_rune_number("羅"), Some(28));
        assert_eq!(get_rune_number("貝"), Some(30));
    }

    #[test]
    fn test_get_rune_number_longest_match_first() {
        // "伊司特" 不应被 "伊司" 抢先匹配
        assert_eq!(get_rune_number("伊司特"), Some(24));
        assert_eq!(get_rune_number("伊斯特"), Some(24));
    }

    #[test]
    fn test_get_rune_number_edge() {
        assert_eq!(get_rune_number("  伊斯特  "), Some(24));
        assert_eq!(get_rune_number("随机文字"), None);
        assert_eq!(get_rune_number(""), None);
    }

    #[test]
    fn test_get_rune_number_contains_text() {
        // "那夫#4" 应该匹配到 "那夫"
        assert_eq!(get_rune_number("那夫#4"), Some(4));
        // 含英文混杂
        assert_eq!(get_rune_number("頂級：L⊕#28"), Some(28));
    }

    #[test]
    fn test_is_high_rune() {
        assert!(!is_high_rune(1));
        assert!(!is_high_rune(23));
        assert!(is_high_rune(24));
        assert!(is_high_rune(30));
        assert!(is_high_rune(33));
    }

    #[test]
    fn test_get_rune_name() {
        assert_eq!(get_rune_name(1), Some("艾尔"));
        assert_eq!(get_rune_name(4), Some("那夫"));
        assert_eq!(get_rune_name(6), Some("伊司"));
        assert_eq!(get_rune_name(24), Some("伊斯特"));
        assert_eq!(get_rune_name(31), Some("乔"));
        assert_eq!(get_rune_name(33), Some("萨德"));
        assert_eq!(get_rune_name(0), None);
        assert_eq!(get_rune_name(34), None);
    }
}
