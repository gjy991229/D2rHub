//! 游戏 OCR 识别数据
//! 由提取脚本自动生成

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

// ============================================================
// 1. 符文数据
// ============================================================

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RuneInfo {
    pub level: u32,
    pub english_name: &'static str,
    pub chinese_name: &'static str,
}

pub static RUNE_NAME_MAP: LazyLock<HashMap<&'static str, RuneInfo>> = LazyLock::new(|| {
    HashMap::from([
        (
            "艾尔",
            RuneInfo {
                level: 1,
                english_name: "El",
                chinese_name: "艾尔",
            },
        ),
        (
            "艾德",
            RuneInfo {
                level: 2,
                english_name: "Eld",
                chinese_name: "艾德",
            },
        ),
        (
            "特尔",
            RuneInfo {
                level: 3,
                english_name: "Tir",
                chinese_name: "特尔",
            },
        ),
        (
            "那夫",
            RuneInfo {
                level: 4,
                english_name: "Nef",
                chinese_name: "那夫",
            },
        ),
        (
            "爱斯",
            RuneInfo {
                level: 5,
                english_name: "Eth",
                chinese_name: "爱斯",
            },
        ),
        (
            "伊司",
            RuneInfo {
                level: 6,
                english_name: "Ith",
                chinese_name: "伊司",
            },
        ),
        (
            "塔尔",
            RuneInfo {
                level: 7,
                english_name: "Tal",
                chinese_name: "塔尔",
            },
        ),
        (
            "拉尔",
            RuneInfo {
                level: 8,
                english_name: "Ral",
                chinese_name: "拉尔",
            },
        ),
        (
            "欧特",
            RuneInfo {
                level: 9,
                english_name: "Ort",
                chinese_name: "欧特",
            },
        ),
        (
            "书尔",
            RuneInfo {
                level: 10,
                english_name: "Thul",
                chinese_name: "书尔",
            },
        ),
        (
            "安姆",
            RuneInfo {
                level: 11,
                english_name: "Amn",
                chinese_name: "安姆",
            },
        ),
        (
            "索尔",
            RuneInfo {
                level: 12,
                english_name: "Sol",
                chinese_name: "索尔",
            },
        ),
        (
            "夏",
            RuneInfo {
                level: 13,
                english_name: "Shael",
                chinese_name: "夏",
            },
        ),
        (
            "多尔",
            RuneInfo {
                level: 14,
                english_name: "Dol",
                chinese_name: "多尔",
            },
        ),
        (
            "海尔",
            RuneInfo {
                level: 15,
                english_name: "Hel",
                chinese_name: "海尔",
            },
        ),
        (
            "艾欧",
            RuneInfo {
                level: 16,
                english_name: "Io",
                chinese_name: "艾欧",
            },
        ),
        (
            "卢姆",
            RuneInfo {
                level: 17,
                english_name: "Lum",
                chinese_name: "卢姆",
            },
        ),
        (
            "科",
            RuneInfo {
                level: 18,
                english_name: "Ko",
                chinese_name: "科",
            },
        ),
        (
            "法尔",
            RuneInfo {
                level: 19,
                english_name: "Fal",
                chinese_name: "法尔",
            },
        ),
        (
            "蓝姆",
            RuneInfo {
                level: 20,
                english_name: "Lem",
                chinese_name: "蓝姆",
            },
        ),
        (
            "普尔",
            RuneInfo {
                level: 21,
                english_name: "Pul",
                chinese_name: "普尔",
            },
        ),
        (
            "乌姆",
            RuneInfo {
                level: 22,
                english_name: "Um",
                chinese_name: "乌姆",
            },
        ),
        (
            "马尔",
            RuneInfo {
                level: 23,
                english_name: "Mal",
                chinese_name: "马尔",
            },
        ),
        (
            "伊斯特",
            RuneInfo {
                level: 24,
                english_name: "Ist",
                chinese_name: "伊斯特",
            },
        ),
        (
            "古尔",
            RuneInfo {
                level: 25,
                english_name: "Gul",
                chinese_name: "古尔",
            },
        ),
        (
            "伐克斯",
            RuneInfo {
                level: 26,
                english_name: "Vex",
                chinese_name: "伐克斯",
            },
        ),
        (
            "欧姆",
            RuneInfo {
                level: 27,
                english_name: "Ohm",
                chinese_name: "欧姆",
            },
        ),
        (
            "罗",
            RuneInfo {
                level: 28,
                english_name: "Lo",
                chinese_name: "罗",
            },
        ),
        (
            "瑟",
            RuneInfo {
                level: 29,
                english_name: "Sur",
                chinese_name: "瑟",
            },
        ),
        (
            "贝",
            RuneInfo {
                level: 30,
                english_name: "Ber",
                chinese_name: "贝",
            },
        ),
        (
            "乔",
            RuneInfo {
                level: 31,
                english_name: "Jah",
                chinese_name: "乔",
            },
        ),
        (
            "查姆",
            RuneInfo {
                level: 32,
                english_name: "Cham",
                chinese_name: "查姆",
            },
        ),
        (
            "萨德",
            RuneInfo {
                level: 33,
                english_name: "Zod",
                chinese_name: "萨德",
            },
        ),
    ])
});

#[allow(dead_code)]
pub static HIGH_RUNE_NAME_SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    RUNE_NAME_MAP
        .iter()
        .filter(|(_, info)| info.level >= 20)
        .map(|(name, _)| *name)
        .collect()
});

// ============================================================
// 2. 场景名称数据
// ============================================================

pub static SCENE_NAME_SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "下水道一层",
        "下水道三层",
        "下水道二层",
        "下水道第一层",
        "下水道第三层",
        "下水道第二层",
        "世界之石大殿",
        "世界之石要塞一层",
        "世界之石要塞三层",
        "世界之石要塞二层",
        "世界之石要塞第一层",
        "世界之石要塞第三层",
        "世界之石要塞第二层",
        "主母巢穴",
        "主母的巢穴",
        "乱石旷野",
        "亚巴顿",
        "亚瑞特之巅",
        "亚瑞特山巅",
        "亚瑞特山脉巅峰",
        "亚瑞特高原",
        "亡者大殿一层",
        "亡者大殿三层",
        "亡者大殿二层",
        "侠盗营地",
        "修道院大门",
        "偏远绿洲",
        "僧院大门",
        "先祖之路",
        "内侧回廊",
        "军营",
        "冥河之洞",
        "冥河地穴",
        "冰冷之原",
        "冰冻之河",
        "冰冻苔原",
        "冰冻高地",
        "冰川小径",
        "冰河",
        "冰河小径",
        "冰河路径",
        "冰窖",
        "利爪腹蛇神殿第一层",
        "利爪腹蛇神殿第二层",
        "利爪蝮蛇神殿一层",
        "利爪蝮蛇神殿二层",
        "剥皮丛林",
        "剥皮地窖第一层",
        "剥皮地窖第三层",
        "剥皮地窖第二层",
        "古代水道",
        "古代通道",
        "古老石墓第一层",
        "古老石墓第二层",
        "后宫第一层",
        "后宫第二层",
        "哈洛加斯",
        "哞哞农场",
        "哞哞农庄",
        "嗜战丘陵",
        "地下墓穴一层",
        "地下墓穴三层",
        "地下墓穴二层",
        "地下墓穴四层",
        "地下墓穴第一层",
        "地下墓穴第三层",
        "地下墓穴第二层",
        "地下墓穴第四层",
        "地下通道一层",
        "地下通道二层",
        "地底通道第一层",
        "地底通道第二层",
        "地洞第一层",
        "地洞第二层",
        "地狱",
        "地狱魔窟",
        "地穴第一层",
        "地穴第二层",
        "埋骨之地",
        "塔拉夏之墓",
        "塔拉夏的古墓",
        "塔拉夏的墓室",
        "塔拉夏的密室",
        "墓地",
        "墓穴",
        "外侧回廊",
        "外围荒原",
        "外域荒原",
        "大教堂",
        "大沼泽",
        "大陵墓",
        "失落之城",
        "失落古城",
        "女眷住处一层",
        "女眷住处二层",
        "女眷住处第一层",
        "女眷住处第二层",
        "宏伟山巅",
        "寒冰地窖",
        "寝陵",
        "尼拉塞克的神殿",
        "崔凡克",
        "崔斯特姆",
        "崔斯特瑞姆",
        "巨像之巅",
        "巨蛛巢穴",
        "干土高地",
        "干燥的高地",
        "干燥高地",
        "库拉斯特上层",
        "库拉斯特上层区",
        "库拉斯特下层",
        "库拉斯特下层区",
        "库拉斯特商场",
        "库拉斯特堤道",
        "库拉斯特市集",
        "库拉斯特海港",
        "库拉斯特港口",
        "库拉斯特集市",
        "庞大湿地",
        "废弃的圣殿",
        "废弃的圣物",
        "废弃的寺院",
        "废弃的礼拜堂",
        "废弃的藏骨室",
        "悲痛之厅",
        "憎恨囚牢一层",
        "憎恨囚牢三层",
        "憎恨囚牢二层",
        "憎恨囚牢第一层",
        "憎恨囚牢第三层",
        "憎恨囚牢第二层",
        "憎恨的囚牢第一层",
        "憎恨的囚牢第三层",
        "憎恨的囚牢第二层",
        "术士的峡谷",
        "死亡神殿第一层",
        "死亡神殿第三层",
        "死亡神殿第二层",
        "残破的寺院",
        "残破神殿",
        "毁灭王座",
        "毁灭的王座",
        "毁灭的神庙",
        "水晶通道",
        "沃特大厅",
        "沼泽地洞第一层",
        "沼泽地洞第三层",
        "沼泽地洞第二层",
        "沼泽地穴第一层",
        "沼泽地穴第三层",
        "沼泽地穴第二层",
        "沼泽陷坑一层",
        "沼泽陷坑三层",
        "沼泽陷坑二层",
        "法师峡谷",
        "泰摩高地",
        "洞坑一层",
        "洞坑二层",
        "洞穴一层",
        "洞穴二层",
        "洞穴第一层",
        "洞穴第二层",
        "洞窟第一层",
        "洞窟第二层",
        "流亡者营地",
        "深渊一层",
        "深渊二层",
        "混沌庇难所",
        "混沌界要塞",
        "混沌要塞",
        "混沌避难所",
        "混沌魔殿",
        "漂泊者洞穴",
        "漂泊者洞窟",
        "漂流洞窟",
        "火焰之河",
        "炼狱地穴",
        "炼狱深渊",
        "王宫监牢一层",
        "王宫监牢三层",
        "王宫监牢二层",
        "瓦特之厅",
        "痛楚大厅",
        "痛苦之厅",
        "痛苦熔炉",
        "皇宫地窖第一层",
        "皇宫地窖第三层",
        "皇宫地窖第二层",
        "皇宫监牢第一层",
        "皇宫监牢第三层",
        "皇宫监牢第二层",
        "监牢一层",
        "监牢三层",
        "监牢二层",
        "监牢第一层",
        "监牢第三层",
        "监牢第二层",
        "石制古墓第一层",
        "石制古墓第二层",
        "石块旷野",
        "碎石古墓一层",
        "碎石古墓二层",
        "碎石荒地",
        "碎石荒野",
        "神秘庇难所",
        "神秘避难所",
        "神罚之城",
        "秘密母牛关卡",
        "秘法圣殿",
        "精华荒地",
        "绝望平原",
        "罪罚之城",
        "群蛇峡谷",
        "群魔堡垒",
        "翠绿丛林",
        "翠绿监牢一层",
        "翠绿监牢三层",
        "翠绿监牢二层",
        "艾巴当",
        "苦痛大厅",
        "苦痛的熔炉",
        "荒废的寺院",
        "萝格营地",
        "营房",
        "蛆虫巢穴一层",
        "蛆虫巢穴三层",
        "蛆虫巢穴二层",
        "蛆虫巢穴第一层",
        "蛆虫巢穴第三层",
        "蛆虫巢穴第二层",
        "蜘蛛巢穴",
        "蜘蛛森林",
        "蜘蛛洞穴",
        "蜘蛛洞窟",
        "血腥丘陵",
        "被毁的礼拜堂",
        "被遗忘的神殿",
        "被遗忘的藏骨室",
        "被遗忘的高塔",
        "贤者之谷",
        "远古之路",
        "遗失的城市",
        "遗忘之塔",
        "遗忘沙漠",
        "遗忘的圣殿",
        "遗忘的圣物",
        "遗忘的沙丘",
        "遗忘神殿",
        "遥远的绿洲",
        "邪恶洞穴",
        "邪恶洞窟",
        "邪恶洞窟第一层",
        "郊外大草原",
        "都瑞尔的房间",
        "阿克隆深渊",
        "高塔地牢第一层",
        "高塔地牢第三层",
        "高塔地牢第二层",
        "高塔地牢第五层",
        "高塔地牢第四层",
        "高塔地窖一层",
        "高塔地窖三层",
        "高塔地窖二层",
        "高塔地窖五层",
        "高塔地窖四层",
        "鲁高因",
        "鲜血荒地",
        "黑暗森林",
        "黑色沼泽",
        "黑色荒地",
    ])
});

#[allow(dead_code)]
pub static MAIN_CITY_NAME_SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "侠盗营地",
        "哈洛加斯",
        "库拉斯特海港",
        "库拉斯特港口",
        "流亡者营地",
        "混沌界要塞",
        "混沌要塞",
        "群魔堡垒",
        "萝格营地",
        "鲁高因",
    ])
});

// ============================================================
// 3. 可接受的符文名全集
// ============================================================

pub static ACCEPTABLE_RUNE_NAME_SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    for key in RUNE_NAME_MAP.keys() {
        set.insert(*key);
    }
    for key in RUNE_NAME_TRANS_MAPPING.keys() {
        set.insert(*key);
    }
    set
});

// ============================================================
// 4. 纠错映射
// ============================================================

pub static RUNE_NAME_TRANS_MAPPING: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| {
        HashMap::from([
            ("提尔", "特尔"),
            ("奈夫", "那夫"),
            ("沙伊", "夏"),
            ("兰姆", "蓝姆"),
            ("玛尔", "马尔"),
            ("伊司特", "伊斯特"),
            ("扎哈", "乔"),
            ("佐德", "萨德"),
            ("图尔", "书尔"),
            ("艾斯", "爱斯"),
            ("伊斯", "伊司"),
            ("埃欧", "艾欧"),
        ])
    });

// ============================================================
// 英文符文名 → 标准简体中文名（不区分大小写，存储为小写）
// ============================================================
pub static RUNE_NAME_EN_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("el", "艾尔"),
        ("eld", "艾德"),
        ("tir", "特尔"),
        ("nef", "那夫"),
        ("eth", "爱斯"),
        ("ith", "伊司"),
        ("tal", "塔尔"),
        ("ral", "拉尔"),
        ("ort", "欧特"),
        ("thul", "书尔"),
        ("amn", "安姆"),
        ("sol", "索尔"),
        ("shael", "夏"),
        ("dol", "多尔"),
        ("hel", "海尔"),
        ("io", "艾欧"),
        ("lum", "卢姆"),
        ("ko", "科"),
        ("fal", "法尔"),
        ("lem", "蓝姆"),
        ("pul", "普尔"),
        ("um", "乌姆"),
        ("mal", "马尔"),
        ("ist", "伊斯特"),
        ("gul", "古尔"),
        ("vex", "伐克斯"),
        ("ohm", "欧姆"),
        ("lo", "罗"),
        ("sur", "瑟"),
        ("ber", "贝"),
        ("jah", "乔"),
        ("cham", "查姆"),
        ("zod", "萨德"),
    ])
});

/// 英文符文名模糊匹配键集合（小写）
pub static EN_RUNE_SET: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| RUNE_NAME_EN_MAP.keys().copied().collect());

// ============================================================
// 繁体中文符文名 → 标准简体中文名（OCR 可能识出繁体字形）
// ============================================================
pub static RUNE_NAME_TC_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("艾爾", "艾尔"),
        ("特爾", "特尔"),
        ("書爾", "书尔"),
        ("愛斯", "爱斯"),
        ("索爾", "索尔"),
        ("夏爾", "夏"),
        ("多爾", "多尔"),
        ("海爾", "海尔"),
        ("艾歐", "艾欧"),
        ("盧姆", "卢姆"),
        ("法爾", "法尔"),
        ("藍姆", "蓝姆"),
        ("普爾", "普尔"),
        ("烏姆", "乌姆"),
        ("馬爾", "马尔"),
        ("維克斯", "伐克斯"),
        ("歐姆", "欧姆"),
        ("羅", "罗"),
        ("貝", "贝"),
        ("喬", "乔"),
        ("查姆", "查姆"),
        ("薩德", "萨德"),
    ])
});

/// 繁体符文名模糊匹配键集合
pub static TC_RUNE_SET: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| RUNE_NAME_TC_MAP.keys().copied().collect());

/// 数字 → 符文名（1-33）
pub fn rune_name_from_number(n: u32) -> Option<&'static str> {
    RUNE_NUMBER_MAP.get(&n).copied()
}

static RUNE_NUMBER_MAP: LazyLock<HashMap<u32, &'static str>> = LazyLock::new(|| {
    RUNE_NAME_MAP
        .iter()
        .map(|(name, info)| (info.level, *name))
        .collect()
});

/// 场景名称首字 OCR 易错字 → 修正为"进"
pub static SCENE_REPLACE_FIRST: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["辽", "迎", "近", "道"]));

/// 场景名称第二字 OCR 易错字 → 修正为"入"
pub static SCENE_REPLACE_SECOND: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["人", "儿", "八"]));

/// 符文首字 OCR 易错字 → 修正为"符"
pub static RUNE_REPLACE_FIRST: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["付"]));

/// 符文第二字 OCR 易错字 → 修正为"文"
pub static RUNE_REPLACE_SECOND: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["义"]));

/// 符文识别中的无用词（过滤掉）
pub static RUNE_UNUSEFUL_WORD: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["出货", "恭喜出货"]));

/// 需要从符文名中剥离的分类词
pub static RUNE_NEED_CLEAN_WORD: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["顶级符文", "高等符文", "高级符文"]));

// ============================================================
// 4.5. 符文别名全集 — 任意别名→编号(1-33)，按长度降序
//     (供 rune_data 和全项目编号查询使用)
// ============================================================

/// 所有已知符文别名 → 编号，按字符串长度降序排列（最长优先匹配）
pub static ALL_RUNE_ALIASES: LazyLock<Vec<(&'static str, u32)>> = LazyLock::new(|| {
    let mut map: HashMap<&'static str, u32> = HashMap::new();

    // 1. 标准简体中文名（唯一权威来源）
    for (name, info) in RUNE_NAME_MAP.iter() {
        map.insert(name, info.level);
    }

    // 2. 纠错/方言变体（来自 RUNE_NAME_TRANS_MAPPING）
    for (variant, standard) in RUNE_NAME_TRANS_MAPPING.iter() {
        if let Some(info) = RUNE_NAME_MAP.get(standard) {
            map.entry(variant).or_insert(info.level);
        }
    }

    // 3. 英文名（小写）
    for (en, cn) in RUNE_NAME_EN_MAP.iter() {
        if let Some(info) = RUNE_NAME_MAP.get(cn) {
            map.entry(en).or_insert(info.level);
        }
    }

    // 4. 繁体中文名
    for (tc, sc) in RUNE_NAME_TC_MAP.iter() {
        if let Some(info) = RUNE_NAME_MAP.get(sc) {
            map.entry(tc).or_insert(info.level);
        }
    }

    // 5. 额外方言别名（仅存在于旧 rune_data::RUNE_NAMES，尚未被上述覆盖）
    map.entry("提尔").or_insert(3); // 特尔
    map.entry("奈夫").or_insert(4); // 那夫
    map.entry("沙伊").or_insert(13); // 夏
    map.entry("扎").or_insert(31); // 乔/扎哈 的短称（OCR 常只识别出"扎"）

    let mut aliases: Vec<(&'static str, u32)> = map.into_iter().collect();
    aliases.sort_by_key(|entry| std::cmp::Reverse(entry.0.chars().count()));
    aliases
});

// ============================================================
// 5. 工具函数
// ============================================================

pub fn is_chinese_char(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c)
}

pub fn keep_only_chinese(text: &str) -> String {
    text.chars().filter(|&c| is_chinese_char(c)).collect()
}

/// 只保留 ASCII 字母（A-Z a-z），用于英文模糊匹配
pub fn keep_only_ascii_letters(text: &str) -> String {
    text.chars().filter(|c| c.is_ascii_alphabetic()).collect()
}

pub fn fix_rune_name(text: &str) -> &str {
    RUNE_NAME_TRANS_MAPPING.get(text).copied().unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rune_count() {
        assert_eq!(RUNE_NAME_MAP.len(), 33);
    }

    #[test]
    fn test_high_rune_count() {
        assert!(HIGH_RUNE_NAME_SET.contains("蓝姆"));
        assert!(HIGH_RUNE_NAME_SET.contains("萨德"));
    }

    #[test]
    fn test_scene_count() {
        assert!(SCENE_NAME_SET.len() > 100);
    }

    #[test]
    fn test_is_chinese_char() {
        assert!(is_chinese_char('中'));
        assert!(!is_chinese_char('a'));
    }

    #[test]
    fn test_keep_only_chinese() {
        assert_eq!(keep_only_chinese("进入罗格营地123"), "进入罗格营地");
        assert_eq!(keep_only_chinese("Hello World!"), "");
    }

    #[test]
    fn test_fix_rune_name() {
        assert_eq!(fix_rune_name("兰姆"), "蓝姆");
        assert_eq!(fix_rune_name("蓝姆"), "蓝姆");
    }
}
