#![allow(dead_code)]

/// 判断像素是否为「红色」——用于通道A地点提示检测
#[inline]
pub fn is_red_pixel(r: u8, g: u8, b: u8) -> bool {
    // 红色：R > 180 且 R 比 G/B 显著高
    r > 180 && r as i32 - g as i32 > 120 && r as i32 - b as i32 > 120
}

/// 判断像素是否为「黑色」——用于通道A主界面检测
#[inline]
pub fn is_black_pixel(r: u8, g: u8, b: u8) -> bool {
    r < 10 && g < 10 && b < 10
}

/// 自适应二值化通道A：先提取 V = max(0, R - G) 作为红色信号灰度，然后用 Otsu 算法计算阈值，大于等于阈值的部分为文本。
/// 输出到可复用的 `out` 缓冲区：目标(文本)→0(黑)，背景→255(白)
pub fn binarize_adaptive_ch_a_raw(raw: &[u8], w: u32, h: u32, out: &mut Vec<u8>) {
    let pixels = (w * h) as usize;
    out.resize(pixels, 0);

    // 1. 提取 R-G 灰度并统计直方图
    let mut histogram = [0u32; 256];
    let mut gray_vals = Vec::with_capacity(pixels);
    let mut total_val = 0u64;

    for i in 0..pixels {
        let idx = i * 4;
        let r = raw[idx] as i32;
        let g = raw[idx + 1] as i32;
        let diff = (r - g).clamp(0, 255) as u8;
        gray_vals.push(diff);
        histogram[diff as usize] += 1;
        total_val += diff as u64;
    }

    // 2. Otsu's method 寻找最佳阈值
    let mut sum_b = 0f64;
    let mut w_b = 0u32;
    let mut var_max = 0f64;
    let mut threshold = 0u8;

    let total_pixels = pixels as u32;
    let sum1 = total_val as f64;

    for (t, count) in histogram.iter().copied().enumerate() {
        w_b += count;
        if w_b == 0 {
            continue;
        }
        let w_f = total_pixels - w_b;
        if w_f == 0 {
            break;
        }

        sum_b += (t as u32 * count) as f64;
        let m_b = sum_b / w_b as f64;
        let m_f = (sum1 - sum_b) / w_f as f64;

        let var_between = w_b as f64 * w_f as f64 * (m_b - m_f) * (m_b - m_f);
        if var_between > var_max {
            var_max = var_between;
            threshold = t as u8;
        }
    }

    // 3. 应用阈值（Otsu 往往偏低，增加一个保底阈值防止暗背景噪点）
    let final_threshold = threshold.max(15);

    for (i, out_pixel) in out.iter_mut().enumerate().take(pixels) {
        // 大于等于阈值认为是红色文字，输出0(黑)，否则255(白)
        *out_pixel = if gray_vals[i] >= final_threshold {
            0
        } else {
            255
        };
    }
}

pub fn crop_mask_by_vertical_projection(mask: &[u8], w: u32, h: u32, gap_limit: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (0, 0);
    }

    let mut col_sums = vec![0u32; w as usize];
    for y in 0..h {
        let row_start = (y * w) as usize;
        for x in 0..w {
            if mask[row_start + x as usize] == 255 {
                col_sums[x as usize] += 1;
            }
        }
    }

    let center_x = w / 2;

    // 向左扫描
    let mut left_bound = center_x;
    let mut gap_count = 0;
    for x in (0..=center_x).rev() {
        if col_sums[x as usize] > 0 {
            gap_count = 0;
            left_bound = x;
        } else {
            gap_count += 1;
            if gap_count > gap_limit {
                left_bound = x;
                break;
            }
        }
    }

    // 向右扫描
    let mut right_bound = center_x;
    gap_count = 0;
    for x in center_x..w {
        if col_sums[x as usize] > 0 {
            gap_count = 0;
            right_bound = x;
        } else {
            gap_count += 1;
            if gap_count > gap_limit {
                right_bound = x;
                break;
            }
        }
    }

    // 防御性修正
    if left_bound > right_bound {
        right_bound = left_bound;
    }

    (left_bound, right_bound)
}

/// 垂直投影裁切：计算二值化后图像每列的非白色像素(黑色像素)数。
/// 返回 (新起始X, 裁切宽度)，外扩 10 像素。
pub fn crop_by_vertical_projection(gray_buf: &[u8], w: u32, h: u32, scale_x: f32) -> (u32, u32) {
    let mut col_sums = vec![0u32; w as usize];
    for y in 0..h {
        let row_start = (y * w) as usize;
        for x in 0..w {
            if gray_buf[row_start + (x as usize)] == 0 {
                col_sums[x as usize] += 1;
            }
        }
    }

    // 找到所有非空列（至少有2个像素，过滤掉单像素噪点）
    let mut valid_cols = Vec::new();
    for x in 0..w {
        if col_sums[x as usize] >= 2 {
            valid_cols.push(x);
        }
    }

    if valid_cols.is_empty() {
        return (0, w);
    }

    // 从中心向两侧扩展聚类，超过空隙直接截断
    let center_x = w / 2;
    let mut anchor_x = valid_cols[0];
    let mut min_dist = (valid_cols[0] as i32 - center_x as i32).abs();

    for &x in valid_cols.iter().skip(1) {
        let dist = (x as i32 - center_x as i32).abs();
        if dist < min_dist {
            min_dist = dist;
            anchor_x = x;
        }
    }

    let max_gap = (8.0 * scale_x).max(5.0) as u32; // 允许8个像素的间隙
    let mut final_start = anchor_x;
    let mut final_end = anchor_x;

    // 向右扩展
    for &x in valid_cols.iter() {
        if x > final_end {
            if x - final_end <= max_gap {
                final_end = x;
            } else {
                break; // 超过间隙，直接截断
            }
        }
    }

    // 向左扩展
    for &x in valid_cols.iter().rev() {
        if x < final_start {
            if final_start - x <= max_gap {
                final_start = x;
            } else {
                break; // 超过间隙，直接截断
            }
        }
    }

    // 保留探测到的这个允许空白范围，只丢弃 max_gap 靠外之后的部分（替换掉以前写死的10像素）
    let padding = max_gap;
    let new_start = final_start.saturating_sub(padding);
    let new_end = (final_end + padding).min(w.saturating_sub(1));
    let new_w = new_end - new_start + 1;

    (new_start, new_w)
}

/// 灰度图裁切
pub fn crop_gray_buf(
    gray_buf: &[u8],
    old_w: u32,
    h: u32,
    new_start_x: u32,
    new_w: u32,
    out: &mut Vec<u8>,
) {
    let pixels = (new_w * h) as usize;
    out.resize(pixels, 0);
    for y in 0..h {
        let src_start = (y * old_w + new_start_x) as usize;
        let dst_start = (y * new_w) as usize;
        out[dst_start..dst_start + (new_w as usize)]
            .copy_from_slice(&gray_buf[src_start..src_start + (new_w as usize)]);
    }
}

/// 计算图像中所有独立连通域（像素碎片）的 Y 轴中心点标准差
pub fn get_y_centers_std_dev(gray_buf: &[u8], w: u32, h: u32) -> f32 {
    let mut labels = vec![0u32; (w * h) as usize];
    let mut current_label = 1u32;
    let mut centers = Vec::new();

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            if gray_buf[idx] == 0 && labels[idx] == 0 {
                let mut stack = vec![(x, y)];
                labels[idx] = current_label;
                let mut c_min_y = y;
                let mut c_max_y = y;

                while let Some((cx, cy)) = stack.pop() {
                    c_min_y = c_min_y.min(cy);
                    c_max_y = c_max_y.max(cy);

                    // 8-way connectivity
                    for dy in -1i32..=1 {
                        for dx in -1i32..=1 {
                            if dx == 0 && dy == 0 {
                                continue;
                            }
                            let nx = cx as i32 + dx;
                            let ny = cy as i32 + dy;
                            if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                                let n_idx = (ny * w as i32 + nx) as usize;
                                if gray_buf[n_idx] == 0 && labels[n_idx] == 0 {
                                    labels[n_idx] = current_label;
                                    stack.push((nx as u32, ny as u32));
                                }
                            }
                        }
                    }
                }

                centers.push((c_min_y + c_max_y) as f32 / 2.0);
                current_label += 1;
            }
        }
    }

    let n = centers.len() as f32;
    if n <= 1.0 {
        return 0.0;
    }

    let mean = centers.iter().sum::<f32>() / n;
    let variance = centers.iter().map(|c| (c - mean) * (c - mean)).sum::<f32>() / n;
    variance.sqrt()
}

/// RGB to OpenCV-style HSV (H: 0-179, S: 0-255, V: 0-255)
pub fn rgb_to_hsv_cv(r: u8, g: u8, b: u8) -> (i32, i32, i32) {
    let mut h = 0;
    let mut s = 0;
    let v = r.max(g).max(b);
    let v_min = r.min(g).min(b);
    let diff = v as i32 - v_min as i32;

    if v > 0 {
        s = (255 * diff) / v as i32;
    }

    if diff > 0 {
        if v == r {
            h = 60 * (g as i32 - b as i32) / diff;
        } else if v == g {
            h = 120 + 60 * (b as i32 - r as i32) / diff;
        } else if v == b {
            h = 240 + 60 * (r as i32 - g as i32) / diff;
        }
        if h < 0 {
            h += 360;
        }
    }

    (h / 2, s, v as i32)
}

/// Extract mask based on base RGB color and a range.
/// Returns a Vec<u8> where 255 is foreground, 0 is background.
pub fn extract_mask_by_hsv(
    raw: &[u8],
    w: u32,
    h_img: u32,
    base_rgb: [u8; 3],
    hsv_range: [u8; 3],
    out: &mut Vec<u8>,
) {
    let pixels = (w * h_img) as usize;
    out.resize(pixels, 0);

    let (base_h, base_s, base_v) = rgb_to_hsv_cv(base_rgb[0], base_rgb[1], base_rgb[2]);
    let range_h = hsv_range[0] as i32;
    let range_s = hsv_range[1] as i32;
    let range_v = hsv_range[2] as i32;

    let h_min = base_h - range_h;
    let h_max = base_h + range_h;
    let s_min = (base_s - range_s).max(0);
    let s_max = (base_s + range_s).min(255);
    let v_min = (base_v - range_v).max(0);
    let v_max = (base_v + range_v).min(255);

    for (i, out_pixel) in out.iter_mut().enumerate().take(pixels) {
        let idx = i * 4;
        let r = raw[idx];
        let g = raw[idx + 1];
        let b = raw[idx + 2];
        let (px_h, px_s, px_v) = rgb_to_hsv_cv(r, g, b);

        let h_match = if h_min < 0 {
            (px_h >= (h_min + 180)) || (px_h <= h_max)
        } else if h_max > 179 {
            (px_h >= h_min) || (px_h <= (h_max - 180))
        } else {
            px_h >= h_min && px_h <= h_max
        };

        if h_match && px_s >= s_min && px_s <= s_max && px_v >= v_min && px_v <= v_max {
            *out_pixel = 255;
        } else {
            *out_pixel = 0;
        }
    }
}

/// Morphological Close (Dilate then Erode) using a 2x2 kernel.
pub fn morphology_close(mask: &mut [u8], w: u32, h: u32) {
    let mut temp = vec![0u8; mask.len()];

    // Dilate 2x2
    for y in 0..h.saturating_sub(1) {
        for x in 0..w.saturating_sub(1) {
            let idx = (y * w + x) as usize;
            if mask[idx] == 255
                || mask[idx + 1] == 255
                || mask[idx + w as usize] == 255
                || mask[idx + w as usize + 1] == 255
            {
                temp[idx] = 255;
            }
        }
    }

    // Erode 2x2
    let mut temp2 = vec![0u8; mask.len()];
    for y in 0..h.saturating_sub(1) {
        for x in 0..w.saturating_sub(1) {
            let idx = (y * w + x) as usize;
            if temp[idx] == 255
                && temp[idx + 1] == 255
                && temp[idx + w as usize] == 255
                && temp[idx + w as usize + 1] == 255
            {
                temp2[idx] = 255;
            }
        }
    }
    mask.copy_from_slice(&temp2);
}

/// Find solid rectangle bounding boxes from the mask (specifically for Rune drop backgrounds).
pub fn find_rect_contours(
    mask: &[u8],
    w: u32,
    h: u32,
    _max_height: u32,
) -> Vec<(u32, u32, u32, u32)> {
    let mut visited = vec![false; mask.len()];
    let mut rois = Vec::new();

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            if mask[idx] == 255 && !visited[idx] {
                // BFS to find connected component
                let mut min_x = x;
                let mut max_x = x;
                let mut min_y = y;
                let mut max_y = y;
                let mut queue = vec![(x, y)];
                visited[idx] = true;

                while let Some((cx, cy)) = queue.pop() {
                    min_x = min_x.min(cx);
                    max_x = max_x.max(cx);
                    min_y = min_y.min(cy);
                    max_y = max_y.max(cy);

                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            if dx == 0 && dy == 0 {
                                continue;
                            }
                            let nx = cx as i32 + dx;
                            let ny = cy as i32 + dy;
                            if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                                let n_idx = (ny * w as i32 + nx) as usize;
                                if mask[n_idx] == 255 && !visited[n_idx] {
                                    visited[n_idx] = true;
                                    queue.push((nx as u32, ny as u32));
                                }
                            }
                        }
                    }
                }

                let box_w = max_x - min_x + 1;
                let box_h = max_y - min_y + 1;

                // 面积须大于游戏窗口面积的万分之八，过滤噪点
                let box_area = box_w as u64 * box_h as u64;
                let frame_area = w as u64 * h as u64;
                if box_area * 10000 <= frame_area * 8 {
                    continue;
                }

                let aspect_ratio = box_w as f32 / box_h as f32;
                if !(0.5..=12.0).contains(&aspect_ratio) {
                    continue;
                }

                rois.push((min_x, min_y, box_w, box_h));
            }
        }
    }
    rois
}

/// Find text bounding boxes from the mask.
pub fn find_text_contours(
    mask: &[u8],
    w: u32,
    h: u32,
    _max_height: u32,
    debug_ctx: Option<(&std::path::Path, &str)>,
) -> Vec<(u32, u32, u32, u32)> {
    let mut visited = vec![false; mask.len()];
    let mut rois = Vec::new();

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            if mask[idx] == 255 && !visited[idx] {
                // BFS to find connected component
                let mut min_x = x;
                let mut max_x = x;
                let mut min_y = y;
                let mut max_y = y;
                let mut queue = vec![(x, y)];
                visited[idx] = true;

                while let Some((cx, cy)) = queue.pop() {
                    min_x = min_x.min(cx);
                    max_x = max_x.max(cx);
                    min_y = min_y.min(cy);
                    max_y = max_y.max(cy);

                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            if dx == 0 && dy == 0 {
                                continue;
                            }
                            let nx = cx as i32 + dx;
                            let ny = cy as i32 + dy;
                            if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                                let n_idx = (ny * w as i32 + nx) as usize;
                                if mask[n_idx] == 255 && !visited[n_idx] {
                                    visited[n_idx] = true;
                                    queue.push((nx as u32, ny as u32));
                                }
                            }
                        }
                    }
                }

                let box_w = max_x - min_x + 1;
                let box_h = max_y - min_y + 1;

                let aspect_ratio = box_w as f32 / box_h as f32;

                // Debug output for rejections
                if let Some((dir, prefix)) = debug_ctx {
                    let fail_reason = if box_w < 10 || box_h < 10 {
                        Some("too_small")
                    } else if !(2.0..=20.0).contains(&aspect_ratio) {
                        Some("bad_aspect")
                    } else {
                        None
                    };

                    if let Some(reason) = fail_reason {
                        let mut fail_buf = vec![0u8; (box_w * box_h) as usize];
                        for fy in 0..box_h {
                            for fx in 0..box_w {
                                let src_idx = ((min_y + fy) * w + (min_x + fx)) as usize;
                                let dst_idx = (fy * box_w + fx) as usize;
                                fail_buf[dst_idx] = mask[src_idx];
                            }
                        }
                        let _ = image::save_buffer(
                            dir.join(format!(
                                "{}_fail_{}_{}_{}_{}_{}.png",
                                prefix, reason, box_w, box_h, min_x, min_y
                            )),
                            &fail_buf,
                            box_w,
                            box_h,
                            image::ColorType::L8,
                        );
                        continue;
                    }
                } else {
                    if box_w < 10 || box_h < 10 {
                        continue;
                    }
                    if !(2.0..=20.0).contains(&aspect_ratio) {
                        continue;
                    }
                }

                rois.push((min_x, min_y, box_w, box_h));
            }
        }
    }

    rois
}

/// Convert mask to RGBA so we can feed it into Windows OCR Engine
// Keeping explicit ROI coordinates avoids allocating a transient crop descriptor per frame.
#[allow(clippy::too_many_arguments)]
pub fn mask_to_rgba(
    mask: &[u8],
    w: u32,
    _h: u32,
    px: u32,
    py: u32,
    pw: u32,
    ph: u32,
    out: &mut Vec<u8>,
) {
    let req_size = (pw * ph * 4) as usize;
    out.resize(req_size, 0);
    for dy in 0..ph {
        for dx in 0..pw {
            let src_idx = ((py + dy) * w + (px + dx)) as usize;
            let val = mask[src_idx];
            let dst_idx = ((dy * pw + dx) * 4) as usize;
            // Text is black on white for OCR
            if val == 255 {
                out[dst_idx] = 0;
                out[dst_idx + 1] = 0;
                out[dst_idx + 2] = 0;
                out[dst_idx + 3] = 255;
            } else {
                out[dst_idx] = 255;
                out[dst_idx + 1] = 255;
                out[dst_idx + 2] = 255;
                out[dst_idx + 3] = 255;
            }
        }
    }
}

/// Adaptive morphology close: square kernel, resolution-tiered, 2 iterations
/// Matches competitor's cv2.morphologyEx(MORPH_CLOSE, kernel, iterations=2)
pub fn morphology_close_adaptive(mask: &mut [u8], w: u32, h: u32, fw: u32) {
    let k: i32 = if fw >= 2500 {
        12
    } else if fw >= 1900 {
        8
    } else {
        6
    };
    let w_i = w as i32;
    let h_i = h as i32;

    for _ in 0..2 {
        // Dilate
        let src = mask.to_vec();
        for y in 0..h_i {
            for x in 0..w_i {
                let mut hit = false;
                'outer: for dy in -k..=k {
                    for dx in -k..=k {
                        let nx = x + dx;
                        let ny = y + dy;
                        if nx >= 0
                            && nx < w_i
                            && ny >= 0
                            && ny < h_i
                            && src[(ny * w_i + nx) as usize] == 255
                        {
                            hit = true;
                            break 'outer;
                        }
                    }
                }
                mask[(y * w_i + x) as usize] = if hit { 255 } else { 0 };
            }
        }
        // Erode
        let src = mask.to_vec();
        for y in 0..h_i {
            for x in 0..w_i {
                let mut all = true;
                'outer2: for dy in -k..=k {
                    for dx in -k..=k {
                        let nx = x + dx;
                        let ny = y + dy;
                        if nx >= 0
                            && nx < w_i
                            && ny >= 0
                            && ny < h_i
                            && src[(ny * w_i + nx) as usize] != 255
                        {
                            all = false;
                            break 'outer2;
                        }
                    }
                }
                mask[(y * w_i + x) as usize] = if all { 255 } else { 0 };
            }
        }
    }
}

/// Find tight bounding box of white (255) pixels using vertical + horizontal projection
pub fn find_text_bbox(mask: &[u8], w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let wu = w as usize;
    let hu = h as usize;

    let mut col_sums = vec![0u32; wu];
    let mut row_sums = vec![0u32; hu];
    for y in 0..hu {
        for x in 0..wu {
            if mask[y * wu + x] == 255 {
                col_sums[x] += 1;
                row_sums[y] += 1;
            }
        }
    }

    let min_x = col_sums.iter().position(|&c| c > 0)? as u32;
    let max_x = col_sums.iter().rposition(|&c| c > 0)? as u32;
    let min_y = row_sums.iter().position(|&r| r > 0)? as u32;
    let max_y = row_sums.iter().rposition(|&r| r > 0)? as u32;

    if max_x < min_x || max_y < min_y {
        return None;
    }
    Some((min_x, min_y, max_x - min_x + 1, max_y - min_y + 1))
}
