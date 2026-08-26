/// 符文标准名称（简体中文，按编号 1-33 排列）
/// 与前端 src/store/stats.ts 保持同步
pub const RUNE_NAMES: [&str; 33] = [
    "艾尔",
    "艾德",
    "特尔",
    "那夫",
    "爱斯",
    "伊司",
    "塔尔",
    "拉尔",
    "欧特",
    "书尔",
    "安姆",
    "索尔",
    "夏",
    "多尔",
    "海尔",
    "艾欧",
    "卢姆",
    "科",
    "法尔",
    "蓝姆",
    "普尔",
    "乌姆",
    "马尔",
    "伊斯特",
    "古尔",
    "伐克斯",
    "欧姆",
    "罗",
    "瑟",
    "贝",
    "乔",
    "查姆",
    "萨德",
];

pub const RUNE_NAMES_EN: [&str; 33] = [
    "El", "Eld", "Tir", "Nef", "Eth", "Ith", "Tal", "Ral", "Ort", "Thul", "Amn", "Sol", "Shael",
    "Dol", "Hel", "Io", "Lum", "Ko", "Fal", "Lem", "Pul", "Um", "Mal", "Ist", "Gul", "Vex", "Ohm",
    "Lo", "Sur", "Ber", "Jah", "Cham", "Zod",
];

const RUNE_ALIASES: [(&str, u32); 26] = [
    ("伊司特", 24),
    ("伊斯特", 24),
    ("Shael", 13),
    ("提尔", 3),
    ("奈夫", 4),
    ("图尔", 10),
    ("沙伊", 13),
    ("兰姆", 20),
    ("玛尔", 23),
    ("扎哈", 31),
    ("佐德", 33),
    ("伊斯", 6),
    ("艾斯", 5),
    ("埃欧", 16),
    ("羅", 28),
    ("貝", 30),
    ("Jah", 31),
    ("Zod", 33),
    ("Nef", 4),
    ("Ist", 24),
    ("L⊕", 28),
    ("扎", 31),
    ("伊司", 6),
    ("那夫", 4),
    ("特尔", 3),
    ("萨德", 33),
];

/// 从旧记录文字或编号中匹配符文，返回 1-based 编号（1-33）
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

    // 1. 标准名优先按长度匹配，避免短名称截获长名称。
    let mut candidates: Vec<(&str, u32)> = RUNE_NAMES
        .iter()
        .enumerate()
        .map(|(index, name)| (*name, index as u32 + 1))
        .chain(
            RUNE_NAMES_EN
                .iter()
                .enumerate()
                .map(|(index, name)| (*name, index as u32 + 1)),
        )
        .chain(RUNE_ALIASES)
        .collect();
    candidates.sort_by_key(|(alias, _)| std::cmp::Reverse(alias.chars().count()));
    for (alias, level) in candidates {
        if lower.contains(&alias.to_lowercase()) {
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
