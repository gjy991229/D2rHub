//! Cheap red-text projection gate; OCR always receives original RGB pixels.
use super::{processing, Candidate};
use image::{imageops, RgbImage};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

pub fn red([r, g, b]: [u8; 3]) -> bool {
    let (r, g, b) = (r as u32, g as u32, b as u32);
    // The supplied entry screenshots have a vivid red core around (202, 24, 0).
    // Require bright, saturated red instead of accepting dim red scenery.
    r >= 120 && g <= 60 && b <= 60 && r >= g * 4 && r >= b * 4
}
#[derive(Debug, serde::Serialize)]
pub struct Feature {
    pub present: bool,
    pub pixels: usize,
    pub strokes: usize,
    pub bounds: Option<(u32, u32, u32, u32)>,
}
pub fn feature(image: &RgbImage) -> Feature {
    // A strict color mask selects glyph cores; keep thin strokes eligible.
    feature_color(image, red, 0.045)
}
fn feature_color(image: &RgbImage, color: impl Fn([u8; 3]) -> bool, min_density: f32) -> Feature {
    let (w, h) = image.dimensions();
    let mut columns = vec![0usize; w as usize];
    let mut rows = vec![0usize; h as usize];
    let mut pixels = 0;
    for (x, y, p) in image.enumerate_pixels() {
        if color(p.0) {
            columns[x as usize] += 1;
            rows[y as usize] += 1;
            pixels += 1;
        }
    }
    let min_column = (h as usize / 30).clamp(2, 8);
    let xs: Vec<_> = columns
        .iter()
        .enumerate()
        .filter(|(_, n)| **n >= min_column)
        .map(|(x, _)| x as u32)
        .collect();
    let min_row = (w as usize / 150).max(3);
    let ys: Vec<_> = rows
        .iter()
        .enumerate()
        .filter(|(_, n)| **n >= min_row)
        .map(|(y, _)| y as u32)
        .collect();
    let bounds = xs
        .first()
        .zip(xs.last())
        .zip(ys.first().zip(ys.last()))
        .map(|((&x0, &x1), (&y0, &y1))| (x0, y0, x1 + 1, y1 + 1));
    let strokes = columns
        .iter()
        .fold((0, false), |(n, on), value| {
            let next = *value >= min_column;
            (n + usize::from(next && !on), next)
        })
        .0;
    let present = bounds.is_some_and(|(x0, y0, x1, y1)| {
        let bw = x1 - x0;
        let bh = y1 - y0;
        let density = pixels as f32 / (bw * bh).max(1) as f32;
        bh >= 9
            && bh * 12 >= h
            && bw >= bh * 3
            && bw <= bh * 45
            && strokes >= 3
            && (min_density..=0.72).contains(&density)
    });
    Feature {
        present,
        pixels,
        strokes,
        bounds,
    }
}
pub fn tz_crop(image: &RgbImage, entry: &Feature) -> Option<RgbImage> {
    let bottom = entry
        .bounds
        .filter(|_| entry.present)
        .map(|v| v.3)
        .unwrap_or(image.height() / 2);
    if bottom >= image.height() {
        return None;
    }
    let below =
        imageops::crop_imm(image, 0, bottom, image.width(), image.height() - bottom).to_image();
    processing::chat_lines(&below).into_iter().next()
}
pub fn terror_text(text: &str) -> bool {
    text.chars().any(|c| "邪恶区域恐怖地带".contains(c))
        || text.to_ascii_lowercase().contains("terrorized")
        || text.to_ascii_lowercase().contains("terror zone")
}

pub fn crop(image: &RgbImage, feature: &Feature) -> Option<RgbImage> {
    let (x0, y0, x1, y1) = feature.bounds.filter(|_| feature.present)?;
    let pad = ((y1 - y0) / 10).clamp(2, 8);
    let x = x0.saturating_sub(pad);
    let y = y0.saturating_sub(pad);
    Some(
        imageops::crop_imm(
            image,
            x,
            y,
            (x1 + pad).min(image.width()) - x,
            (y1 + pad).min(image.height()) - y,
        )
        .to_image(),
    )
}
#[cfg(test)]
pub fn resolve(text: &str, candidates: &[Candidate]) -> Option<Candidate> {
    resolve_name(text, candidates).ok()
}

fn resolve_name(text: &str, candidates: &[Candidate]) -> Result<Candidate, &'static str> {
    let text = text.trim();
    // A map-entry announcement must contain its explicit prefix. Never accept
    // arbitrary red chat, health text, or a best-effort nearest catalog name.
    let body = text
        .strip_prefix("进入")
        .or_else(|| text.strip_prefix("進入"))
        .or_else(|| text.strip_prefix("Entering "))
        .ok_or("missing_entry_prefix")?;
    let name = processing::normalize(body);
    let mut found = candidates.iter().filter(|candidate| {
        candidate
            .aliases
            .iter()
            .any(|alias| processing::normalize(alias) == name)
    });
    let candidate = found.next().ok_or("unknown_scene_name")?;
    if found.any(|other| {
        processing::normalize(&other.name) != processing::normalize(&candidate.name)
            || other.location.as_ref().map(|v| v.kind)
                != candidate.location.as_ref().map(|v| v.kind)
    }) {
        return Err("ambiguous_scene_name");
    }
    Ok(candidate.clone())
}
#[derive(Default)]
pub struct Gate {
    signature: Option<[u8; 32]>,
    result: Option<Candidate>,
    retry: Option<Instant>,
    pub last_skip: Option<&'static str>,
    pub last_rejection: Option<&'static str>,
    entry_name: Option<String>,
    entry_id: Option<u64>,
    tz_seen: bool,
}
impl Gate {
    pub fn absent(&mut self) {
        self.signature = None;
        self.result = None;
        self.entry_name = None;
        self.entry_id = None;
        self.tz_seen = false;
        self.last_rejection = None;
    }
    pub fn read(
        &mut self,
        image: &RgbImage,
        now: Instant,
        candidates: &[Candidate],
        tz: Option<&RgbImage>,
        mut recognize: impl FnMut(&RgbImage) -> Result<(String, f32), String>,
    ) -> Result<Option<Candidate>, String> {
        self.last_skip = None;
        self.last_rejection = None;
        let mut hash = Sha256::new();
        hash.update(image.width().to_le_bytes());
        hash.update(image.height().to_le_bytes());
        // Background animation must not invalidate an otherwise identical glyph mask.
        for p in image.pixels() {
            hash.update([u8::from(red(p.0))]);
        }
        hash.update([u8::from(tz.is_some())]);
        if let Some(tz) = tz {
            hash.update(tz.width().to_le_bytes());
            hash.update(tz.height().to_le_bytes());
            for p in tz.pixels() {
                hash.update(p.0);
            }
        }
        let signature = hash.finalize().into();
        if self.signature == Some(signature)
            && (self.result.as_ref().is_some_and(|v| v.tz || tz.is_none())
                || self.retry.is_some_and(|until| until > now))
        {
            self.last_skip = Some(if self.result.is_some() {
                "unchanged_red_mask"
            } else {
                "retry_cooldown"
            });
            return Ok(self.result.clone());
        }
        self.signature = Some(signature);
        self.result = None;
        self.retry = Some(now + Duration::from_secs(2));
        let (text, score) = recognize(image)?;
        if score >= 0.90 {
            match resolve_name(&text, candidates) {
                Ok(candidate) => self.result = Some(candidate),
                Err(reason) => self.last_rejection = Some(reason),
            }
            if let Some(result) = self.result.as_mut() {
                result.confidence = score;
            }
            if let Some(result) = self.result.as_mut() {
                if self.entry_name.as_deref() != Some(result.name.as_str()) {
                    static NEXT: std::sync::atomic::AtomicU64 =
                        std::sync::atomic::AtomicU64::new(1);
                    self.entry_name = Some(result.name.clone());
                    self.entry_id = Some(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
                    self.tz_seen = false;
                }
                result.announcement = self.entry_id;
                result.tz = self.tz_seen;
                if let Some(tz) = tz {
                    let (text, _) = recognize(tz)?;
                    if terror_text(&text) {
                        self.tz_seen = true;
                        result.tz = true;
                    }
                }
            }
        } else {
            self.last_rejection = Some("low_ocr_confidence");
        }
        Ok(self.result.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tz_matches_any_requested_character_but_not_color_or_unrelated_text() {
        for c in "邪恶区域恐怖地带".chars() {
            assert!(terror_text(&format!("x{c}x")));
        }
        for text in ["Terrorized", "TERROR ZONE"] {
            assert!(terror_text(text));
        }
        for text in ["", "进入洞穴", "普通场景"] {
            assert!(!terror_text(text));
        }
        let entry = Feature {
            present: false,
            pixels: 0,
            strokes: 0,
            bounds: None,
        };
        for rgb in [[220, 220, 220], [200, 140, 50], [140, 40, 200]] {
            let mut image = RgbImage::new(160, 80);
            for y in 48..65 {
                for x in 20..130 {
                    if x % 20 < 7 {
                        image.put_pixel(x, y, image::Rgb(rgb));
                    }
                }
            }
            assert!(tz_crop(&image, &entry).is_some());
        }
    }

    #[test]
    fn rejection_reasons_distinguish_unknown_names_from_conflicting_aliases() {
        let candidate = Candidate {
            name: "古墓".into(),
            aliases: vec!["古墓".into(), "共用译名".into()],
            ..Default::default()
        };
        let duplicate = Candidate {
            name: "古·墓".into(),
            ..candidate.clone()
        };
        assert!(resolve_name("进入古墓", &[candidate.clone(), duplicate]).is_ok());
        assert_eq!(
            resolve_name("古墓", std::slice::from_ref(&candidate)).err(),
            Some("missing_entry_prefix")
        );
        assert_eq!(
            resolve_name("进入未登记地点", std::slice::from_ref(&candidate)).err(),
            Some("unknown_scene_name")
        );
        let conflicting = Candidate {
            name: "另一处地点".into(),
            ..candidate.clone()
        };
        assert_eq!(
            resolve_name("进入共用译名", &[candidate, conflicting]).err(),
            Some("ambiguous_scene_name")
        );
    }

    #[test]
    fn muted_scenery_is_rejected_but_sparse_bright_strokes_and_purple_pass() {
        let mut noise = RgbImage::new(384, 97);
        for y in 4..68 {
            for x in 20..360 {
                if (x - 20) % 40 < 20 && (x + y) % 7 == 0 {
                    noise.put_pixel(x, y, image::Rgb([100, 45, 30]));
                }
            }
        }
        assert!(!feature(&noise).present);

        for pixel in noise.pixels_mut() {
            if pixel.0 != [0, 0, 0] {
                *pixel = image::Rgb([160, 0, 0]);
            }
        }
        assert!(feature(&noise).present);
        for pixel in noise.pixels_mut() {
            if red(pixel.0) {
                *pixel = image::Rgb([140, 40, 200]);
            }
        }
        // Sparse purple scenery is below the stricter TZ threshold.
        let entry = Feature {
            present: true,
            pixels: 0,
            strokes: 0,
            bounds: Some((0, 0, 1, 1)),
        };
        assert!(tz_crop(&noise, &entry).is_none());
        for y in 4..32 {
            for x in 20..300 {
                if (x - 20) % 30 < 8 {
                    noise.put_pixel(x, y, image::Rgb([140, 40, 200]));
                }
            }
        }
        assert!(tz_crop(&noise, &entry).is_some());
    }

    #[test]
    fn red_requires_bright_saturated_pixels_and_accepts_screenshot_glyph_cores() {
        for pixel in [[202, 24, 0], [170, 21, 1], [120, 15, 3], [255, 60, 60]] {
            assert!(red(pixel), "glyph core {pixel:?}");
        }
        for pixel in [
            [65, 0, 0],      // dark red
            [119, 15, 2],    // below the brightness floor
            [180, 60, 0],    // orange
            [160, 45, 30],   // muted red
            [240, 61, 0],    // too much green
            [240, 0, 61],    // too much blue
            [140, 40, 200],  // purple TZ text
            [200, 200, 200], // neutral scenery
        ] {
            assert!(!red(pixel), "background {pixel:?}");
        }
    }

    #[test]
    #[ignore = "Read-only color gate probe; set D2RHUB_AREA_COLOR_SAMPLE to an entry ROI PNG"]
    fn screenshot_color_gate_probe() {
        let image = image::open(std::env::var("D2RHUB_AREA_COLOR_SAMPLE").unwrap())
            .unwrap()
            .to_rgb8();
        for image in [
            image.clone(),
            imageops::resize(&image, 384, 97, imageops::FilterType::Triangle),
        ] {
            let entry = feature(&image);
            println!("{}x{}: {entry:?}", image.width(), image.height());
            assert!(entry.present);
            let entry_crop = crop(&image, &entry).unwrap();
            let has_tz = std::env::var("D2RHUB_AREA_COLOR_TZ").is_ok_and(|v| v == "1");
            assert_eq!(tz_crop(&image, &entry).is_some(), has_tz);
            // Gating selects a rectangle and must preserve every RGB pixel.
            assert!(entry_crop.pixels().any(|pixel| !red(pixel.0)));
        }
    }

    #[test]
    fn compact_entry_glyphs_pass_at_multiple_capture_scales() {
        for scale in [1, 2, 3] {
            let mut image = RgbImage::new(384 * scale, 97 * scale);
            for glyph in 0..7 {
                for y in 0..28 {
                    for x in 0..20 {
                        if !(2..18).contains(&x) || y < 2 || (13..15).contains(&y) || y >= 26 {
                            for dy in 0..scale {
                                for dx in 0..scale {
                                    image.put_pixel(
                                        (60 + glyph * 30 + x) * scale + dx,
                                        (4 + y) * scale + dy,
                                        image::Rgb([160, 0, 0]),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            assert!(feature(&image).present, "scale {scale}");
        }
    }

    #[test]
    fn tz_is_bound_to_one_announcement_and_does_not_disappear_when_purple_fades() {
        let candidates = vec![Candidate {
            name: "洞穴".into(),
            aliases: vec!["洞穴".into()],
            ..Default::default()
        }];
        let red = RgbImage::from_pixel(100, 20, image::Rgb([160, 0, 0]));
        let purple = RgbImage::from_pixel(60, 20, image::Rgb([140, 40, 200]));
        let now = Instant::now();
        let mut gate = Gate::default();
        let normal = gate
            .read(&red, now, &candidates, None, |_| {
                Ok(("进入洞穴".into(), 0.99))
            })
            .unwrap()
            .unwrap();
        let mut calls = 0;
        let terror = gate
            .read(&red, now, &candidates, Some(&purple), |_| {
                calls += 1;
                Ok((
                    if calls == 1 {
                        "进入洞穴"
                    } else {
                        "恐惧区域"
                    }
                    .into(),
                    0.99,
                ))
            })
            .unwrap()
            .unwrap();
        assert!(terror.tz);
        assert_eq!(calls, 2, "TZ requires its own OCR result");
        assert_eq!(normal.announcement, terror.announcement);
        let fading = gate
            .read(&red, now, &candidates, None, |_| {
                Ok(("进入洞穴".into(), 0.99))
            })
            .unwrap()
            .unwrap();
        assert!(fading.tz);
        gate.absent();
        let next = gate
            .read(&red, now, &candidates, None, |_| {
                Ok(("进入洞穴".into(), 0.99))
            })
            .unwrap()
            .unwrap();
        assert!(!next.tz);
        assert_ne!(next.announcement, normal.announcement);
    }
    #[test]
    fn retries_failed_text_but_reuses_success_despite_background_animation() {
        let candidates = vec![Candidate {
            name: "洞穴".into(),
            name_en: "Cave".into(),
            aliases: vec!["洞穴".into()],
            ..Default::default()
        }];
        let mut gate = Gate::default();
        let now = Instant::now();
        let mut image = RgbImage::new(200, 25);
        image.put_pixel(10, 10, image::Rgb([160, 0, 0]));
        assert!(gate
            .read(&image, now, &candidates, None, |_| Ok((
                "进入洞穴".into(),
                0.89
            )))
            .unwrap()
            .is_none());
        assert_eq!(gate.last_rejection, Some("low_ocr_confidence"));
        assert!(gate
            .read(
                &image,
                now + Duration::from_secs(1),
                &candidates,
                None,
                |_| panic!("retry cooldown")
            )
            .unwrap()
            .is_none());
        assert!(gate
            .read(
                &image,
                now + Duration::from_secs(2),
                &candidates,
                None,
                |_| Ok(("进入洞穴".into(), 0.99))
            )
            .unwrap()
            .is_some());
        image.put_pixel(20, 10, image::Rgb([50, 50, 50]));
        assert!(gate
            .read(
                &image,
                now + Duration::from_secs(3),
                &candidates,
                None,
                |_| panic!("background changes are not glyph changes")
            )
            .unwrap()
            .is_some());
        image.put_pixel(30, 10, image::Rgb([160, 0, 0]));
        assert!(gate
            .read(
                &image,
                now + Duration::from_secs(3),
                &candidates,
                None,
                |_| Ok(("进入地穴".into(), 0.99))
            )
            .unwrap()
            .is_none());
    }
    #[test]
    fn rejects_solid_red_regions_and_requires_entry_prefix_and_unique_name() {
        assert!(!feature(&RgbImage::from_pixel(300, 35, image::Rgb([180, 0, 0]))).present);
        let candidates = vec![Candidate {
            name: "世界之石要塞二层".into(),
            name_en: "Worldstone Keep Level 2".into(),
            aliases: vec!["世界之石要塞二层".into()],
            ..Default::default()
        }];
        assert!(resolve("进入世界之石要塞二层", &candidates).is_some());
        assert!(resolve("世界之石要塞二层", &candidates).is_none());
        assert!(resolve("进入世界之石要塞三层", &candidates).is_none());
    }
}
