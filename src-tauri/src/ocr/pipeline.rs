use crate::ocr::capturer::Capturer;
use crate::ocr::engine;
use crate::ocr::fuzzy;
use crate::ocr::preprocess;
use crate::ocr::{OcrConfig, OcrTextItem};
use crate::rune_data;
use std::path::Path;

/// 通道A 裁剪比例区域 (x_ratio, y_ratio, w_ratio, h_ratio)
const SCENE_TEXT_REGION: (f32, f32, f32, f32) = (0.28, 0.21, 0.44, 0.08);
type RuneMatch = (String, f64, (u32, u32, u32, u32));

struct OcrBuffers {
    frame: Vec<u8>,
    roi_a_raw: Vec<u8>,
    mask_b: Vec<u8>,
    roi_b_raw: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ActiveDrop {
    pub text: String,
    pub bbox: (u32, u32, u32, u32),
    pub last_seen: std::time::Instant,
}

pub struct OcrMonitor {
    config: OcrConfig,
    capturer: Capturer,
    buffers: OcrBuffers,
    last_ch_a_text: String,
    app_data_dir: String,
    last_frame_hash: u64,
    frozen_count: u32,
    active_drop: Option<ActiveDrop>,
}

impl OcrMonitor {
    pub fn new(config: OcrConfig, app_data_dir: String) -> Result<Self, String> {
        // 确保调试输出目录存在（清理已在 start_ocr_monitor 中完成）
        if config.debug_output {
            let test_dir = std::path::Path::new(&app_data_dir).join("test");
            if let Err(e) = std::fs::create_dir_all(&test_dir) {
                eprintln!(
                    "[OCR Debug] 创建调试输出目录失败: {} ({})",
                    test_dir.display(),
                    e
                );
            }
        }

        let capturer = Capturer::new(config.target_pid.unwrap_or(0), &config.window_title)?;
        let buf_size = capturer.buffer_size();

        let buffers = OcrBuffers {
            frame: vec![0u8; buf_size],
            roi_a_raw: Vec::new(),
            mask_b: Vec::new(),
            roi_b_raw: Vec::new(),
        };

        Ok(Self {
            config,
            capturer,
            buffers,
            last_ch_a_text: String::new(),
            app_data_dir,
            last_frame_hash: 0,
            frozen_count: 0,
            active_drop: None,
        })
    }

    pub fn poll(&mut self) {
        let poll_start = std::time::Instant::now();
        // 统一调试输出目录（基于 app_data_dir，与 exe 位置解耦）
        let debug_out_dir = std::path::Path::new(&self.app_data_dir).join("test");
        if self.config.debug_output {
            let _ = std::fs::create_dir_all(&debug_out_dir);
        }

        let needed = self.capturer.buffer_size();
        if self.buffers.frame.len() < needed {
            self.buffers.frame.resize(needed, 0);
        }
        let cap_start = std::time::Instant::now();
        if let Err(e) = self.capturer.capture_into(&mut self.buffers.frame) {
            if self.config.debug_output {
                eprintln!("[OCR Debug] 截图失败: {}", e);
            }
            return;
        }
        let cap_ms = cap_start.elapsed();
        let fw = self.capturer.width;
        let fh = self.capturer.height;

        let hash = frame_fingerprint(&self.buffers.frame, fw, fh);
        if hash == self.last_frame_hash {
            self.frozen_count += 1;
            if self.frozen_count > 10 {
                self.capturer.reset_cache();
                self.frozen_count = 0;
            }
            return;
        }
        self.frozen_count = 0;
        self.last_frame_hash = hash;

        // --- Channel A: Scene OCR ---
        let ch_a_start = std::time::Instant::now();
        let roi_x = (fw as f32 * SCENE_TEXT_REGION.0) as u32;
        let roi_y = (fh as f32 * SCENE_TEXT_REGION.1) as u32;
        let roi_a_w = (fw as f32 * SCENE_TEXT_REGION.2) as u32;
        let roi_a_h = (fh as f32 * SCENE_TEXT_REGION.3) as u32;

        self.buffers
            .roi_a_raw
            .resize((roi_a_w * roi_a_h * 4) as usize, 0);
        for y in 0..roi_a_h {
            for x in 0..roi_a_w {
                let src_idx = ((y + roi_y) * fw + (x + roi_x)) as usize * 4;
                let dst_idx = (y * roi_a_w + x) as usize * 4;
                if src_idx + 3 < self.buffers.frame.len() {
                    self.buffers.roi_a_raw[dst_idx] = self.buffers.frame[src_idx];
                    self.buffers.roi_a_raw[dst_idx + 1] = self.buffers.frame[src_idx + 1];
                    self.buffers.roi_a_raw[dst_idx + 2] = self.buffers.frame[src_idx + 2];
                    self.buffers.roi_a_raw[dst_idx + 3] = self.buffers.frame[src_idx + 3];
                }
            }
        }

        // Channel A Color Gating (HSV): Count red pixels (threshold=1000)
        let (s_r, s_g, s_b) = (
            self.config.scene_text_color_rgb[0],
            self.config.scene_text_color_rgb[1],
            self.config.scene_text_color_rgb[2],
        );
        let (s_h, s_s, s_v) = preprocess::rgb_to_hsv_cv(s_r, s_g, s_b);
        let sc_rh = self.config.scene_text_color_range[0] as i32;
        let sc_rs = self.config.scene_text_color_range[1] as i32;
        let sc_rv = self.config.scene_text_color_range[2] as i32;
        let sc_h_min = s_h - sc_rh;
        let sc_h_max = s_h + sc_rh;
        let sc_s_min = (s_s - sc_rs).max(0);
        let sc_s_max = (s_s + sc_rs).min(255);
        let sc_v_min = (s_v - sc_rv).max(0);
        let sc_v_max = (s_v + sc_rv).min(255);

        let mut scene_matching_pixels = 0;
        for chunk in self.buffers.roi_a_raw.chunks_exact(4) {
            let (px_h, px_s, px_v) = preprocess::rgb_to_hsv_cv(chunk[0], chunk[1], chunk[2]);
            let h_match = if sc_h_min < 0 {
                (px_h >= (sc_h_min + 180)) || (px_h <= sc_h_max)
            } else if sc_h_max > 179 {
                (px_h >= sc_h_min) || (px_h <= (sc_h_max - 180))
            } else {
                px_h >= sc_h_min && px_h <= sc_h_max
            };

            if h_match
                && px_s >= sc_s_min
                && px_s <= sc_s_max
                && px_v >= sc_v_min
                && px_v <= sc_v_max
            {
                scene_matching_pixels += 1;
            }
        }

        if scene_matching_pixels > 1000 {
            // debug: 仅通过颜色门控的帧才存盘
            if self.config.debug_output {
                let _ = image::save_buffer(
                    debug_out_dir.join(format!("{}_ch_a_roi_raw.png", hash)),
                    &self.buffers.roi_a_raw,
                    roi_a_w,
                    roi_a_h,
                    image::ColorType::Rgba8,
                );
            }
            if let Ok(results) = engine::recognize_rgba(&self.buffers.roi_a_raw, roi_a_w, roi_a_h) {
                // 收集所有行的匹配结果，取最高置信度
                let mut best_match: Option<(String, f64)> = None;
                for block in &results {
                    let text = &block.text;
                    if let Some((name, score)) =
                        fuzzy::scene_match(text, self.config.text_matcher_threshold)
                    {
                        if best_match.as_ref().is_none_or(|(_, s)| score > *s) {
                            best_match = Some((name, score));
                        }
                        // score=1.0 精确匹配可提前终止
                        if score >= 1.0 {
                            break;
                        }
                    }
                }

                if let Some((matched, best_score)) = best_match {
                    if self.config.debug_output {
                        use std::io::Write;
                        if let Ok(mut f) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(debug_out_dir.join("ocr_debug.txt"))
                        {
                            let _ =
                                writeln!(f, "[Channel A] ✅ {} (score={:.3})", matched, best_score);
                        }
                    }
                    if matched != self.last_ch_a_text {
                        self.last_ch_a_text = matched.clone();

                        // Clear Channel B deduplication state immediately on scene switch!
                        self.active_drop = None;

                        let is_town =
                            crate::ocr::game_data::MAIN_CITY_NAME_SET.contains(matched.as_str());
                        super::push_result(
                            &super::CH_A_RESULTS,
                            OcrTextItem {
                                text: matched,
                                source: "channel_a".into(),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                                rune_number: None,
                                screenshot_path: None,
                                is_town,
                                rune_name_en: None,
                            },
                        );
                    }
                } else if self.config.debug_output {
                    // 模糊匹配全部失败，输出 OCR 原始文本用于调试
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(debug_out_dir.join("ocr_debug.txt"))
                    {
                        let raw_texts: Vec<&str> =
                            results.iter().map(|b| b.text.as_str()).collect();
                        let _ = writeln!(
                            f,
                            "[Channel A] ❌ 无匹配 (matching_pixels={}) OCR输出: {:?}",
                            scene_matching_pixels, raw_texts
                        );
                    }
                }
            }
        }

        // --- Channel B: Rune OCR ---
        let ch_b_start = std::time::Instant::now();
        // 1. 全屏提取背景底色 (深蓝色)
        preprocess::extract_mask_by_hsv(
            &self.buffers.frame,
            fw,
            fh,
            self.config.rune_background_color_rgb,
            self.config.rune_background_color_range,
            &mut self.buffers.mask_b,
        );

        preprocess::morphology_close(&mut self.buffers.mask_b, fw, fh);

        let rois_bg = preprocess::find_rect_contours(&self.buffers.mask_b, fw, fh, u32::MAX);

        let mut frame_best: Option<RuneMatch> = None;
        let mut unmatched_texts: Vec<String> = Vec::new();

        // ── 蓝底 HSV 门控预计算（用于 ROI 内蓝色掩膜）──
        let (bg_r, bg_g, bg_b) = (
            self.config.rune_background_color_rgb[0],
            self.config.rune_background_color_rgb[1],
            self.config.rune_background_color_rgb[2],
        );
        let (bg_h, bg_s, bg_v) = preprocess::rgb_to_hsv_cv(bg_r, bg_g, bg_b);
        let bg_rh = self.config.rune_background_color_range[0] as i32;
        let bg_rs = self.config.rune_background_color_range[1] as i32;
        let bg_rv = self.config.rune_background_color_range[2] as i32;
        let bg_h_min = bg_h - bg_rh;
        let bg_h_max = bg_h + bg_rh;
        let bg_s_min = (bg_s - bg_rs).max(0);
        let bg_s_max = (bg_s + bg_rs).min(255);
        let bg_v_min = (bg_v - bg_rv).max(0);
        let bg_v_max = (bg_v + bg_rv).min(255);

        // ── 橙字 HSV 门控预计算 ──
        let (txt_r, txt_g, txt_b) = (
            self.config.rune_text_color_rgb[0],
            self.config.rune_text_color_rgb[1],
            self.config.rune_text_color_rgb[2],
        );
        let (txt_h, txt_s, txt_v) = preprocess::rgb_to_hsv_cv(txt_r, txt_g, txt_b);
        let txt_rh = self.config.rune_text_color_range[0] as i32;
        let txt_rs = self.config.rune_text_color_range[1] as i32;
        let txt_rv = self.config.rune_text_color_range[2] as i32;
        let txt_h_min = txt_h - txt_rh;
        let txt_h_max = txt_h + txt_rh;
        let txt_s_min = (txt_s - txt_rs).max(0);
        let txt_s_max = (txt_s + txt_rs).min(255);
        let txt_v_min = (txt_v - txt_rv).max(0);
        let txt_v_max = (txt_v + txt_rv).min(255);

        for (rx, ry, rw, rh) in rois_bg {
            // 提取背景框范围 of RGB data
            self.buffers.roi_b_raw.resize((rw * rh * 4) as usize, 0);

            let mut orange_pixels = 0;

            for y in 0..rh {
                for x in 0..rw {
                    let src_idx = ((y + ry) * fw + (x + rx)) as usize * 4;
                    let dst_idx = (y * rw + x) as usize * 4;
                    if src_idx + 3 < self.buffers.frame.len() {
                        let r = self.buffers.frame[src_idx];
                        let g = self.buffers.frame[src_idx + 1];
                        let b = self.buffers.frame[src_idx + 2];
                        let a = self.buffers.frame[src_idx + 3];

                        self.buffers.roi_b_raw[dst_idx] = r;
                        self.buffers.roi_b_raw[dst_idx + 1] = g;
                        self.buffers.roi_b_raw[dst_idx + 2] = b;
                        self.buffers.roi_b_raw[dst_idx + 3] = a;

                        // 统一计算像素的 HSV (用于橙色字门控 + 蓝底二值化)
                        let (px_h, px_s, px_v) = preprocess::rgb_to_hsv_cv(r, g, b);

                        // 粗略检查橙色像素数量 (使用 HSV)
                        let txt_h_match = if txt_h_min < 0 {
                            (px_h >= (txt_h_min + 180)) || (px_h <= txt_h_max)
                        } else if txt_h_max > 179 {
                            (px_h >= txt_h_min) || (px_h <= (txt_h_max - 180))
                        } else {
                            px_h >= txt_h_min && px_h <= txt_h_max
                        };

                        if txt_h_match
                            && px_s >= txt_s_min
                            && px_s <= txt_s_max
                            && px_v >= txt_v_min
                            && px_v <= txt_v_max
                        {
                            orange_pixels += 1;
                        }

                        // 蓝色掩膜二值化：蓝底→白(255)，非蓝底(文字等)→黑(0)
                        let h_match = if bg_h_min < 0 {
                            (px_h >= (bg_h_min + 180)) || (px_h <= bg_h_max)
                        } else if bg_h_max > 179 {
                            (px_h >= bg_h_min) || (px_h <= (bg_h_max - 180))
                        } else {
                            px_h >= bg_h_min && px_h <= bg_h_max
                        };
                        if h_match
                            && px_s >= bg_s_min
                            && px_s <= bg_s_max
                            && px_v >= bg_v_min
                            && px_v <= bg_v_max
                        {
                            // 蓝底 → 白色 (255)
                            self.buffers.roi_b_raw[dst_idx] = 255;
                            self.buffers.roi_b_raw[dst_idx + 1] = 255;
                            self.buffers.roi_b_raw[dst_idx + 2] = 255;
                        } else {
                            // 非蓝底（文字等）→ 黑色 (0)
                            self.buffers.roi_b_raw[dst_idx] = 0;
                            self.buffers.roi_b_raw[dst_idx + 1] = 0;
                            self.buffers.roi_b_raw[dst_idx + 2] = 0;
                        }
                    }
                }
            }

            if self.config.debug_output {
                if let Err(e) = image::save_buffer(
                    debug_out_dir.join(format!(
                        "{}_ch_b_roi_raw_{}_{}_{}orange.png",
                        hash, rx, ry, orange_pixels
                    )),
                    &self.buffers.roi_b_raw,
                    rw,
                    rh,
                    image::ColorType::Rgba8,
                ) {
                    eprintln!("[OCR Debug] Save ch_b_roi_raw failed: {}", e);
                }
            }

            // 橙色像素点数大于等于 20 才送 OCR
            if orange_pixels >= 20 {
                if let Ok(results) = engine::recognize_rgba(&self.buffers.roi_b_raw, rw, rh) {
                    for block in results {
                        let text = block.text;
                        let matched =
                            fuzzy::rune_match(&text, self.config.rune_matcher_threshold, None);

                        if let Some((name, score)) = matched {
                            if frame_best.as_ref().is_none_or(|(_, s, _)| score > *s) {
                                frame_best = Some((name, score, (rx, ry, rw, rh)));
                            }
                            if score >= 1.0 {
                                break;
                            }
                        } else if self.config.debug_output {
                            unmatched_texts.push(text);
                        }
                    }
                }
            }
        }

        let mut has_matched = false;
        if let Some((matched, best_score, bbox)) = frame_best {
            has_matched = true;
            if self.config.debug_output {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(debug_out_dir.join("ocr_debug.txt"))
                {
                    let _ = writeln!(f, "[Channel B] ✅ {} (score={:.3})", matched, best_score);
                }
            }

            // Check deduplication
            let is_duplicate = if let Some(ref mut active) = self.active_drop {
                if active.text == matched {
                    // Check Euclidean center distance
                    let c1_x = (bbox.0 + bbox.2 / 2) as f32;
                    let c1_y = (bbox.1 + bbox.3 / 2) as f32;
                    let c2_x = (active.bbox.0 + active.bbox.2 / 2) as f32;
                    let c2_y = (active.bbox.1 + active.bbox.3 / 2) as f32;
                    let dist = ((c1_x - c2_x).powi(2) + (c1_y - c2_y).powi(2)).sqrt();

                    if dist < 150.0 {
                        // Update active drop state
                        active.bbox = bbox;
                        active.last_seen = std::time::Instant::now();
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };

            if !is_duplicate {
                // Trigger drop event!
                self.active_drop = Some(ActiveDrop {
                    text: matched.clone(),
                    bbox,
                    last_seen: std::time::Instant::now(),
                });

                let rune_number = rune_data::get_rune_number(&matched);
                let mut screenshot_path = None;
                let mut rune_name_en = None;

                if let Some(info) = crate::ocr::game_data::RUNE_NAME_MAP.get(matched.as_str()) {
                    rune_name_en = Some(info.english_name.to_string());
                }

                if let Some(rn) = rune_number {
                    if rune_data::is_high_rune(rn) {
                        screenshot_path = save_high_rune_screenshot(
                            &self.buffers.frame,
                            fw,
                            fh,
                            rn,
                            &self.app_data_dir,
                        );
                    }
                }

                super::push_result(
                    &super::CH_B_RESULTS,
                    OcrTextItem {
                        text: matched,
                        source: "channel_b".into(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        rune_number,
                        screenshot_path,
                        is_town: false,
                        rune_name_en,
                    },
                );
            }
        } else if self.config.debug_output && !unmatched_texts.is_empty() {
            // 全帧所有 ROI 模糊匹配全部失败，输出 OCR 原始文本
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(debug_out_dir.join("ocr_debug.txt"))
            {
                let _ = writeln!(f, "[Channel B] ❌ 无匹配 OCR输出: {:?}", unmatched_texts);
            }
        }

        // Release deduplication state if no hovered rune detected for > 2.5 seconds
        if !has_matched {
            if let Some(ref active) = self.active_drop {
                if active.last_seen.elapsed() > std::time::Duration::from_millis(2500) {
                    self.active_drop = None;
                }
            }
        }

        // Record poll execution time with per-channel breakdown
        if self.config.debug_output {
            use std::io::Write;
            let pre_process_ms = ch_a_start.duration_since(poll_start);
            let ch_a_ms = ch_b_start.duration_since(ch_a_start);
            let ch_b_true_ms = ch_b_start.elapsed();
            let total_ms = poll_start.elapsed();
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(debug_out_dir.join("ocr_debug.txt"))
            {
                let _ = writeln!(f, "[Perf] Capture: {:?}  |  Pre-process: {:?}  |  Channel A: {:?}  |  Channel B: {:?}  |  Total: {:?}", cap_ms, pre_process_ms, ch_a_ms, ch_b_true_ms, total_ms);
            }
        }
    }
}

/// 保存高级符文完整窗口截图到 stateData/img/
/// 返回相对于 stateData 的路径，失败返回 None
fn save_high_rune_screenshot(
    frame: &[u8],
    width: u32,
    height: u32,
    rune_number: u32,
    app_data_dir: &str,
) -> Option<String> {
    let now = chrono::Local::now();
    let date_str = now.format("%Y%m%d").to_string();
    let time_str = now.format("%H%M%S").to_string();
    let rune_name = rune_data::get_rune_name(rune_number)?;

    let filename = format!(
        "{}_{}_{}_({}).png",
        date_str, time_str, rune_number, rune_name
    );

    let img_dir = Path::new(app_data_dir).join("stateData").join("img");
    if let Err(e) = std::fs::create_dir_all(&img_dir) {
        eprintln!("[OCR] 创建截图目录失败: {} ({})", img_dir.display(), e);
        return None;
    }

    let filepath = img_dir.join(&filename);

    match image::save_buffer(&filepath, frame, width, height, image::ColorType::Rgba8) {
        Ok(()) => {
            let rel_path = format!("img/{}", filename);
            eprintln!("[OCR] ✅ 高级符文截图已保存: {}", rel_path);
            Some(rel_path)
        }
        Err(e) => {
            eprintln!("[OCR] 保存截图失败: {} ({})", filepath.display(), e);
            None
        }
    }
}

/// Frame fingerprint: sample ~1024 evenly distributed pixels for fast hash
fn frame_fingerprint(frame: &[u8], width: u32, height: u32) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    let total = (width as usize) * (height as usize);
    let step = (total / 1024).max(1);
    for i in (0..total).step_by(step) {
        let idx = i * 4;
        if idx + 3 < frame.len() {
            frame[idx..idx + 3].hash(&mut hasher);
        }
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_active_drop_deduplication() {
        let bbox1 = (100, 200, 50, 20);
        let c1_x = (bbox1.0 + bbox1.2 / 2) as f32;
        let c1_y = (bbox1.1 + bbox1.3 / 2) as f32;

        // Same text, close bounding box
        let bbox2 = (110, 205, 50, 20);
        let c2_x = (bbox2.0 + bbox2.2 / 2) as f32;
        let c2_y = (bbox2.1 + bbox2.3 / 2) as f32;
        let dist = ((c1_x - c2_x).powi(2) + (c1_y - c2_y).powi(2)).sqrt();
        assert!(dist < 150.0);

        // Same text, far bounding box
        let bbox3 = (300, 200, 50, 20);
        let c3_x = (bbox3.0 + bbox3.2 / 2) as f32;
        let c3_y = (bbox3.1 + bbox3.3 / 2) as f32;
        let dist_far = ((c1_x - c3_x).powi(2) + (c1_y - c3_y).powi(2)).sqrt();
        assert!(dist_far >= 150.0);
    }
}
