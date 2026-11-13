#[cfg(test)]
use super::config::Roi;
use image::{imageops, RgbImage};

#[derive(Clone, Copy)]
pub enum Mask {
    Text,
}
#[cfg(test)]
pub fn crop(image: &RgbImage, roi: Roi) -> RgbImage {
    let x = (roi.x * image.width() as f32) as u32;
    let y = (roi.y * image.height() as f32) as u32;
    let w = ((roi.width * image.width() as f32) as u32)
        .max(1)
        .min(image.width() - x);
    let h = ((roi.height * image.height() as f32) as u32)
        .max(1)
        .min(image.height() - y);
    imageops::crop_imm(image, x, y, w, h).to_image()
}
fn selected([r, g, b]: [u8; 3], kind: Mask) -> bool {
    let (r, g, b) = (r as i32, g as i32, b as i32);
    match kind {
        Mask::Text => {
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            (min > 150 && max - min < 65) // white/grey
                || (max > 100 && max - min > 45) // colored labels
        }
    }
}
pub fn binary(image: &RgbImage, kind: Mask) -> RgbImage {
    RgbImage::from_fn(image.width(), image.height(), |x, y| {
        image::Rgb(if selected(image.get_pixel(x, y).0, kind) {
            [0, 0, 0]
        } else {
            [255, 255, 255]
        })
    })
}
// The repeated marker is a cheap layout gate; OCR reads only the numeric center.
pub fn chat_signal(sequence: u32) -> String {
    format!("*********** {sequence:03} ***********")
}

pub fn chat_lines(image: &RgbImage) -> Vec<RgbImage> {
    lines_with_gap(image, Mask::Text, 20, 5)
}
fn lines_with_gap(image: &RgbImage, kind: Mask, limit: usize, gap: usize) -> Vec<RgbImage> {
    let original = image;
    let reduced;
    let image = if image.width() > 1920 {
        reduced = imageops::resize(
            image,
            1920,
            (image.height() as f64 * 1920.0 / image.width() as f64)
                .round()
                .max(1.0) as u32,
            imageops::FilterType::Triangle,
        );
        &reduced
    } else {
        image
    };
    let sx = original.width() as f64 / image.width() as f64;
    let sy = original.height() as f64 / image.height() as f64;
    let (w, h) = (image.width() as usize, image.height() as usize);
    let mut mask: Vec<bool> = image.pixels().map(|p| selected(p.0, kind)).collect();
    let mut boxes = Vec::<(usize, usize, usize, usize)>::new();
    let mut stack = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if !mask[y * w + x] {
                continue;
            }
            mask[y * w + x] = false;
            stack.push((x, y));
            let (mut x0, mut y0, mut x1, mut y1, mut n) = (x, y, x, y, 0);
            while let Some((px, py)) = stack.pop() {
                n += 1;
                x0 = x0.min(px);
                x1 = x1.max(px);
                y0 = y0.min(py);
                y1 = y1.max(py);
                for (nx, ny) in [
                    (px.wrapping_sub(1), py),
                    (px + 1, py),
                    (px, py.wrapping_sub(1)),
                    (px, py + 1),
                ] {
                    if nx < w && ny < h && mask[ny * w + nx] {
                        mask[ny * w + nx] = false;
                        stack.push((nx, ny));
                    }
                }
            }
            // At 720p a star can be just a 2x2 component. Keep it so both
            // marker trains remain attached to the taller chat text.
            if n >= 3 && (y1 > y0 || x1 > x0) && y1 - y0 <= h.min(100) && x1 - x0 <= 200 {
                boxes.push((x0, y0, x1 + 1, y1 + 1));
            }
            if boxes.len() > 8000 {
                return Vec::new();
            }
        }
    }
    boxes.sort_by_key(|b| b.0);
    let mut rows = Vec::<(usize, usize, usize, usize)>::new();
    for b in boxes {
        if let Some(row) = rows.iter_mut().find(|r| {
            let height = (r.3 - r.1).max(b.3 - b.1);
            let overlap = r.3.min(b.3).saturating_sub(r.1.max(b.1));
            overlap * 2 >= (r.3 - r.1).min(b.3 - b.1) && b.0 <= r.2 + height * gap
        }) {
            row.0 = row.0.min(b.0);
            row.1 = row.1.min(b.1);
            row.2 = row.2.max(b.2);
            row.3 = row.3.max(b.3);
        } else {
            rows.push(b);
        }
    }
    rows.retain(|r| r.2 - r.0 >= 18 && r.2 - r.0 >= r.3 - r.1 && r.3 - r.1 >= 6);
    rows.sort_by_key(|r| (r.1, r.0));
    let skip = rows.len().saturating_sub(limit);
    rows.into_iter()
        .skip(skip)
        .take(limit)
        .map(|r| {
            let x = r.0.saturating_sub(3);
            let y = r.1.saturating_sub(3);
            let x0 = (x as f64 * sx) as u32;
            let y0 = (y as f64 * sy) as u32;
            let x1 = (((r.2 + 3).min(w) as f64 * sx).ceil() as u32).min(original.width());
            let y1 = (((r.3 + 3).min(h) as f64 * sy).ceil() as u32).min(original.height());
            imageops::crop_imm(original, x0, y0, x1 - x0, y1 - y0).to_image()
        })
        .collect()
}
#[cfg(test)]
pub fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
/// Loose repeated-symbol layout gate, independent of each star's stroke shape.
#[derive(Debug, serde::Serialize)]
pub struct ChatAnalysis {
    pub matched: bool,
    pub glyph_runs: usize,
    pub marker_runs: usize,
    pub geometry_windows: usize,
    pub number_bounds: Option<(u32, u32)>,
    pub reason: &'static str,
}
#[cfg(test)]
pub fn chat_hint(image: &RgbImage) -> bool {
    chat_analysis(image).matched
}
pub fn chat_analysis(image: &RgbImage) -> ChatAnalysis {
    // White/grey signal ink only: colored player names must not distort the
    // projection or be mistaken for the numeric center.
    let white = |x, y| {
        let [r, g, b] = image.get_pixel(x, y).0;
        r.min(g).min(b) > 150 && r.max(g).max(b) - r.min(g).min(b) < 65
    };
    let (w, h) = image.dimensions();
    let mut runs = Vec::new();
    let mut x = 0;
    while x < w {
        if !(0..h).any(|y| white(x, y)) {
            x += 1;
            continue;
        }
        let start = x;
        let (mut top, mut bottom) = (h, 0);
        while x < w && (0..h).any(|y| white(x, y)) {
            for y in 0..h {
                if white(x, y) {
                    top = top.min(y);
                    bottom = bottom.max(y + 1);
                }
            }
            x += 1;
        }
        runs.push((start, x, top, bottom));
    }
    // Group adjacent small glyphs by approximate height and alignment. No
    // cross/diagonal template, exact star count, or fixed pixel spacing.
    let mut trains = Vec::new();
    let max_height = runs.iter().map(|g| g.3 - g.2).max().unwrap_or(0);
    let mut i = 0;
    while i < runs.len() {
        let start = i;
        let anchor = runs[i];
        let height = anchor.3 - anchor.2;
        i += 1;
        while i < runs.len() {
            let glyph = runs[i];
            let gh = glyph.3 - glyph.2;
            if height < 2
                || gh.abs_diff(height) > (height / 2).max(1)
                || glyph.1 - glyph.0 > height * 3
                || glyph.2.abs_diff(anchor.2) > (height / 2).max(1)
                || glyph.0 - runs[i - 1].1 > height * 4
            {
                break;
            }
            i += 1;
        }
        if i - start >= 4
            && height >= 2
            && height * 3 <= max_height * 2
            && anchor.1 - anchor.0 <= height * 3
        {
            trains.push((start, i, height));
        }
    }
    let marker_runs = trains.iter().map(|(start, end, _)| end - start).sum();
    let mut geometry_windows = 0;
    let mut number_bounds = None;
    for pair in trains.windows(2) {
        let (_, left_end, lh) = pair[0];
        let (right_start, _, rh) = pair[1];
        let center = &runs[left_end..right_start];
        if center.is_empty() || center.len() > 10 {
            continue;
        }
        geometry_windows += 1;
        let marker_height = lh.max(rh);
        let center_top = center.iter().map(|g| g.2).min().unwrap();
        let center_bottom = center.iter().map(|g| g.3).max().unwrap();
        let center_height = center_bottom - center_top;
        let left = runs[left_end - 1];
        let right = runs[right_start];
        if lh.abs_diff(rh) <= (marker_height / 2).max(1)
            && left.2.abs_diff(right.2) <= marker_height.max(2)
            && center_height * 2 >= marker_height * 3
            && center_height <= marker_height * 12
            && center_top <= left.3 + marker_height
            && center_bottom >= left.2
            && center[0].0 - left.1 <= center_height * 3
            && right.0 - center.last().unwrap().1 <= center_height * 3
            && center.last().unwrap().1 - center[0].0 <= center_height * 20
        {
            // Keep a little original background, bounded by the marker gaps.
            let x0 = center[0].0.saturating_sub(2).max(left.1);
            let x1 = (center.last().unwrap().1 + 2).min(right.0);
            number_bounds = Some((x0, x1));
            break;
        }
    }
    ChatAnalysis {
        matched: number_bounds.is_some(),
        glyph_runs: runs.len(),
        marker_runs,
        geometry_windows,
        number_bounds,
        reason: if number_bounds.is_some() {
            "marker"
        } else if trains.len() < 2 {
            "too_few_marker_trains"
        } else {
            "no_numeric_center"
        },
    }
}
/// Conservative exact alias matching; do not coerce uncertain text to a rare item.
#[cfg(test)]
pub fn matches(text: &str, aliases: &[String]) -> bool {
    let text = normalize(text);
    !text.is_empty() && aliases.iter().any(|alias| text == normalize(alias))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signal_has_short_readable_dedup_suffix() {
        assert_eq!(chat_signal(77), "*********** 077 ***********");
        assert_eq!(chat_signal(501), "*********** 501 ***********");
        assert_eq!(chat_signal(u32::MAX), "*********** 4294967295 ***********");
    }
    fn frame(text: &str, scale: u32, stagger: bool) -> RgbImage {
        let mut image = RgbImage::new((text.len() as u32 * 10 + 8) * scale, 24 * scale);
        for (i, ch) in text.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            for y in 0..7 * scale {
                for x in 0..7 * scale {
                    let pixel = if ch == '*' {
                        x < 3 * scale && y < 3 * scale
                    } else {
                        true
                    };
                    if pixel {
                        image.put_pixel(
                            (4 + i as u32 * 10) * scale + x,
                            (4 + if stagger && i > text.len() / 2 { 7 } else { 0 }) * scale + y,
                            image::Rgb([255, 255, 255]),
                        );
                    }
                }
            }
        }
        image
    }
    #[test]
    fn loose_marker_trains_gate_only_a_taller_center() {
        for scale in [1, 2, 3] {
            for text in [
                chat_signal(501),
                "**** 077 ******".into(),
                "******** 4294967295 ********".into(),
            ] {
                let image = frame(&text, scale, false);
                assert!(chat_hint(&image), "{text} at {scale}x");
                assert!(
                    chat_lines(&image).iter().any(chat_hint),
                    "segmentation: {text}"
                );
            }
            for text in [
                "*** 501 ***",
                "**** 501",
                "501 ****",
                "***********",
                "hello 501 world",
                "501",
                "+ + 501 + +",
            ] {
                assert!(!chat_hint(&frame(text, scale, false)), "{text}");
            }
            assert!(!chat_hint(&frame("**** 501 ****", scale, true)));
        }
    }
    #[test]
    fn real_1080p_and_720p_signals_keep_all_three_rows() {
        for png in [
            include_bytes!("../../tests/fixtures/chat-signal-1080p.png").as_slice(),
            include_bytes!("../../tests/fixtures/chat-signal-720p.png").as_slice(),
        ] {
            let image = image::load_from_memory(png).unwrap().to_rgb8();
            let lines = chat_lines(&image);
            assert_eq!(lines.len(), 3, "{}x{}", image.width(), image.height());
            for line in lines {
                let report = chat_analysis(&line);
                assert!(report.matched, "{report:?}");
                let (x0, x1) = report.number_bounds.unwrap();
                assert!(x1 > x0 && x1 <= line.width());
                assert!(report.marker_runs >= 8, "{report:?}");
            }
        }
    }
    #[test]
    fn no_fuzzy_rare_item_promotion() {
        assert!(!matches("Ber", &["Belt".into()]));
        assert!(matches("Ber Rune", &["ber rune".into()]));
    }
}
