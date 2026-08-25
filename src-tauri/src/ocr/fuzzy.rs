use crate::ocr::game_data;
use std::path::Path;
use strsim::jaro_winkler;

/// 写入 rune_match debug 日志到 ocr_debug.txt
fn debug_rune_log(dir: &Path, msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("ocr_debug.txt"))
    {
        if let Err(e) = writeln!(f, "{}", msg) {
            eprintln!("[debug_rune_log] 写入失败: {}", e);
        }
    }
}

pub fn scene_clean_text(text: &str) -> String {
    let mut clean_text = game_data::keep_only_chinese(text);
    if clean_text.chars().count() < 3 {
        return "".to_string();
    }

    let mut chars: Vec<char> = clean_text.chars().collect();
    let replace_first = &*game_data::SCENE_REPLACE_FIRST;
    let replace_second = &*game_data::SCENE_REPLACE_SECOND;

    // Convert char to string to check in hashset
    if let Some(first_char) = chars.first() {
        let first_str = first_char.to_string();
        if replace_first.contains(first_str.as_str()) {
            chars[0] = '进';
        }
    }

    if chars.len() > 1 {
        let second_str = chars[1].to_string();
        if replace_second.contains(second_str.as_str()) {
            chars[1] = '入';
        }
    }

    clean_text = chars.into_iter().collect();
    clean_text
        .strip_prefix("进入")
        .unwrap_or(&clean_text)
        .to_string()
}

pub fn scene_match(raw_text: &str, threshold: u8) -> Option<(String, f64)> {
    let raw_text = raw_text.trim().replace(['\r', '\n', '\t'], "");
    let cleaned = scene_clean_text(&raw_text);
    if cleaned.is_empty() {
        return None;
    }

    let scenes = &*game_data::SCENE_NAME_SET;

    if scenes.contains(cleaned.as_str()) {
        return Some((cleaned, 1.0));
    }

    let mut best_match = None;
    let mut best_score = 0.0;

    for &scene in scenes.iter() {
        let score = jaro_winkler(&cleaned, scene);
        if score > best_score {
            best_score = score;
            best_match = Some(scene.to_string());
        }
    }

    let threshold_f64 = (threshold as f64) / 100.0;
    if best_score >= threshold_f64 {
        return best_match.map(|m| (m, best_score));
    }
    None
}

pub fn rune_clean_text(text: &str) -> String {
    let mut clean_text = game_data::keep_only_chinese(text);
    if clean_text.is_empty() {
        return "".to_string();
    }

    for &w in game_data::RUNE_REPLACE_FIRST.iter() {
        clean_text = clean_text.replace(w, "符");
    }

    for &w in game_data::RUNE_REPLACE_SECOND.iter() {
        clean_text = clean_text.replace(w, "文");
    }

    for &w in game_data::RUNE_NEED_CLEAN_WORD.iter() {
        clean_text = clean_text.replace(w, "");
    }

    clean_text.replace("符文", "")
}

pub fn fix_rune_name(text: &str) -> String {
    game_data::fix_rune_name(text).to_string()
}

pub fn rune_match(
    raw_text: &str,
    threshold: u8,
    debug_out_dir: Option<&Path>,
) -> Option<(String, f64)> {
    let raw_text = raw_text.trim().replace(['\r', '\n', '\t'], "");
    if let Some(dir) = debug_out_dir {
        debug_rune_log(
            dir,
            &format!("[RuneMatch] 原始文本(已去特殊字符): {:?}", raw_text),
        );
    }
    // 0. 过滤无用词
    for &w in game_data::RUNE_UNUSEFUL_WORD.iter() {
        if raw_text.contains(w) {
            return None;
        }
    }

    let threshold_f64 = (threshold as f64) / 100.0;

    // 预清理各语种文本
    let cleaned_cn = rune_clean_text(&raw_text);
    let cleaned_en = {
        let s = game_data::keep_only_ascii_letters(&raw_text);
        if s.len() >= 2 {
            s.to_lowercase()
        } else {
            String::new()
        }
    };
    // 繁中和简中共用同一清洗逻辑
    let cleaned_tc = if cleaned_cn.is_empty() {
        let mut c = game_data::keep_only_chinese(&raw_text);
        for &w in game_data::RUNE_NEED_CLEAN_WORD.iter() {
            c = c.replace(w, "");
        }
        c
    } else {
        cleaned_cn.clone()
    };

    // 当前最佳结果
    let mut best_score: f64 = 0.0;
    let mut best_name: Option<String> = None;

    if let Some(dir) = debug_out_dir {
        debug_rune_log(
            dir,
            &format!(
                "[RuneMatch] 预清理后 — cn: {:?} | en: {:?} | tc: {:?}",
                cleaned_cn, cleaned_en, cleaned_tc
            ),
        );
    }

    // ── 1. 简体中文 ──
    if !cleaned_cn.is_empty() {
        let runes = &*game_data::ACCEPTABLE_RUNE_NAME_SET;
        if runes.contains(cleaned_cn.as_str()) {
            best_score = 1.0;
            best_name = Some(fix_rune_name(&cleaned_cn));
            if let Some(dir) = debug_out_dir {
                debug_rune_log(
                    dir,
                    &format!(
                        "[RuneMatch] 层级1(简中) 精确匹配: {} -> {} (score=1.0)",
                        cleaned_cn,
                        best_name.as_ref().unwrap()
                    ),
                );
            }
        } else {
            for &rune in runes.iter() {
                let score = jaro_winkler(&cleaned_cn, rune);
                if score > best_score {
                    best_score = score;
                    best_name = Some(fix_rune_name(rune));
                }
            }
            if let Some(dir) = debug_out_dir {
                if let Some(ref name) = best_name {
                    debug_rune_log(
                        dir,
                        &format!(
                            "[RuneMatch] 层级1(简中) 模糊最佳: {} (score={:.3})",
                            name, best_score
                        ),
                    );
                }
            }
        }
    }

    // ── 2. 英文（不区分大小写） ──
    if !cleaned_en.is_empty() {
        let en_set = &*game_data::EN_RUNE_SET;
        let en_map = &*game_data::RUNE_NAME_EN_MAP;
        if en_set.contains(cleaned_en.as_str()) {
            if 1.0 > best_score {
                best_score = 1.0;
                best_name = en_map.get(cleaned_en.as_str()).map(|&s| s.to_string());
            }
            if let Some(dir) = debug_out_dir {
                debug_rune_log(
                    dir,
                    &format!(
                        "[RuneMatch] 层级2(英文) 精确匹配: {} -> {:?} (score=1.0)",
                        cleaned_en,
                        en_map.get(cleaned_en.as_str()).copied()
                    ),
                );
            }
        } else {
            for &en_key in en_set.iter() {
                let score = jaro_winkler(&cleaned_en, en_key);
                if score > best_score {
                    best_score = score;
                    best_name = en_map.get(en_key).map(|&s| s.to_string());
                }
            }
            if let Some(dir) = debug_out_dir {
                if let Some(ref name) = best_name {
                    debug_rune_log(
                        dir,
                        &format!(
                            "[RuneMatch] 层级2(英文) 模糊最佳: {} (score={:.3})",
                            name, best_score
                        ),
                    );
                } else {
                    debug_rune_log(dir, "[RuneMatch] 层级2(英文) 无有效匹配");
                }
            }
        }
    }

    // ── 3. 繁体中文 ──
    if !cleaned_tc.is_empty() {
        let tc_set = &*game_data::TC_RUNE_SET;
        let tc_map = &*game_data::RUNE_NAME_TC_MAP;
        if tc_set.contains(cleaned_tc.as_str()) {
            if 1.0 > best_score {
                best_score = 1.0;
                best_name = tc_map.get(cleaned_tc.as_str()).map(|&s| s.to_string());
            }
            if let Some(dir) = debug_out_dir {
                debug_rune_log(
                    dir,
                    &format!(
                        "[RuneMatch] 层级3(繁体) 精确匹配: {} -> {:?} (score=1.0)",
                        cleaned_tc,
                        tc_map.get(cleaned_tc.as_str()).copied()
                    ),
                );
            }
        } else {
            for &tc_key in tc_set.iter() {
                let score = jaro_winkler(&cleaned_tc, tc_key);
                if score > best_score {
                    best_score = score;
                    best_name = tc_map.get(tc_key).map(|&s| s.to_string());
                }
            }
            if let Some(dir) = debug_out_dir {
                if let Some(ref name) = best_name {
                    debug_rune_log(
                        dir,
                        &format!(
                            "[RuneMatch] 层级3(繁体) 模糊最佳: {} (score={:.3})",
                            name, best_score
                        ),
                    );
                } else {
                    debug_rune_log(dir, "[RuneMatch] 层级3(繁体) 无有效匹配");
                }
            }
        }
    }

    // ── 4. 数字回退 (1-33) ──
    // 仅在 cleaned_cn 非空（确有中文文本）且无任何文本匹配时才启用，
    // 避免纯噪点数字（如 OCR 碎片中的 "4"）误匹配
    if best_name.is_none() && !cleaned_cn.is_empty() {
        for part in raw_text.split(|c: char| !c.is_ascii_digit()) {
            if let Ok(n) = part.parse::<u32>() {
                if let Some(name) = game_data::rune_name_from_number(n) {
                    // 数字匹配视为较高置信度 (0.85)，但低于精确文本匹配
                    let score = 0.85;
                    if score > best_score {
                        best_score = score;
                        best_name = Some(name.to_string());
                    }
                    if let Some(dir) = debug_out_dir {
                        debug_rune_log(
                            dir,
                            &format!(
                                "[RuneMatch] 层级4(数字) 命中: {} -> {} (score=0.85)",
                                n, name
                            ),
                        );
                    }
                    break;
                }
            }
        }
    }

    // ── 最终结果 ──
    if let Some(dir) = debug_out_dir {
        let threshold_f64_display = (threshold as f64) / 100.0;
        match &best_name {
            Some(name) if best_score >= threshold_f64 => {
                debug_rune_log(
                    dir,
                    &format!(
                        "[RuneMatch] ✅ 最终: {} (best_score={:.3}, threshold={})",
                        name, best_score, threshold_f64_display
                    ),
                );
            }
            Some(name) => {
                debug_rune_log(
                    dir,
                    &format!(
                        "[RuneMatch] ❌ 未达阈值: {} (best_score={:.3}, threshold={})",
                        name, best_score, threshold_f64_display
                    ),
                );
            }
            None => {
                debug_rune_log(
                    dir,
                    &format!(
                        "[RuneMatch] ❌ 无任何匹配 (threshold={})",
                        threshold_f64_display
                    ),
                );
            }
        }
    }

    if best_score >= threshold_f64 {
        best_name.map(|n| (n, best_score))
    } else {
        None
    }
}
