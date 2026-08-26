use super::catalog::{
    marker_from_signal_number, marker_signal_number, TelemetryMarker, SIGNAL_COUNT,
};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

pub const PROTOCOL_VERSION: u8 = 3;
pub const CODE_CHIPS: usize = 63;
pub const CODE_REPETITIONS: usize = 3;
/// 兼容 32kHz 源资源；距离 16kHz 奈奎斯特上限保留 2kHz 保护带。
pub const CARRIER_HZ: f32 = 14_000.0;
/// v3 使用 63-chip Gold 码，并缩短码片以保持约 150ms 的总标签长度。
pub const CHIP_SECONDS: f32 = 0.00075;
pub const MARKER_OFFSET_SECONDS: f32 = 0.008;
pub const MIN_SAMPLE_RATE: u32 = 32_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MarkerConfig {
    pub gain_db: f32,
    pub detection_threshold: f32,
}

impl Default for MarkerConfig {
    fn default() -> Self {
        Self {
            gain_db: -26.0,
            detection_threshold: 0.58,
        }
    }
}

impl MarkerConfig {
    pub fn validate(self) -> Result<Self, String> {
        if !(-42.0..=-12.0).contains(&self.gain_db) {
            return Err("声纹增益必须位于 -42dBFS 到 -12dBFS 之间".to_string());
        }
        if !(0.45..=0.95).contains(&self.detection_threshold) {
            return Err("识别阈值必须位于 0.45 到 0.95 之间".to_string());
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingReport {
    pub marker: TelemetryMarker,
    pub marker_frames: usize,
    pub frames_added: usize,
    pub output_gain: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    pub marker: TelemetryMarker,
    pub confidence: f32,
    pub start_frame: u64,
}

fn m_sequence(taps: &[usize]) -> [u8; CODE_CHIPS] {
    const DEGREE: usize = 6;
    let mut state = [1u8; DEGREE + CODE_CHIPS];
    for index in 0..(CODE_CHIPS - DEGREE) {
        state[index + DEGREE] = taps
            .iter()
            .fold(0u8, |value, tap| value ^ state[index + tap]);
    }
    let mut output = [0u8; CODE_CHIPS];
    output.copy_from_slice(&state[..CODE_CHIPS]);
    output
}

/// 生成长度 63 的 Gold 码族。该优选序列对可提供 65 个码，当前使用前 41 个。
pub fn telemetry_code(signal_number: u32) -> Result<[f32; CODE_CHIPS], String> {
    if !(1..=SIGNAL_COUNT).contains(&signal_number) {
        return Err(format!("声纹信号编号必须位于 1-{SIGNAL_COUNT}"));
    }
    let first = m_sequence(&[0, 1]); // x^6 + x + 1
    let second = m_sequence(&[0, 5]); // x^6 + x^5 + 1
    let mut bits = [0u8; CODE_CHIPS];
    match signal_number {
        1 => bits = first,
        2 => bits = second,
        number => {
            let shift = (number - 3) as usize;
            for index in 0..CODE_CHIPS {
                bits[index] = first[index] ^ second[(index + shift) % CODE_CHIPS];
            }
        }
    }
    Ok(bits.map(|bit| if bit == 0 { -1.0 } else { 1.0 }))
}

fn chip_frames(sample_rate: u32) -> usize {
    ((sample_rate as f32 * CHIP_SECONDS).round() as usize).max(8)
}

pub fn marker_frames(sample_rate: u32) -> usize {
    chip_frames(sample_rate) * CODE_CHIPS * CODE_REPETITIONS
}

fn marker_offset_frames(sample_rate: u32) -> usize {
    (sample_rate as f32 * MARKER_OFFSET_SECONDS).round() as usize
}

pub fn embed_marker(
    samples: &mut Vec<i32>,
    channels: usize,
    bits_per_sample: u32,
    sample_rate: u32,
    marker: TelemetryMarker,
    config: MarkerConfig,
) -> Result<EmbeddingReport, String> {
    let config = config.validate()?;
    if sample_rate < MIN_SAMPLE_RATE {
        return Err(format!(
            "FLAC 采样率 {}Hz 过低；声纹协议最低要求 {}Hz",
            sample_rate, MIN_SAMPLE_RATE
        ));
    }
    if channels == 0 || channels > 8 {
        return Err(format!("不支持的声道数: {channels}"));
    }
    if !(8..=32).contains(&bits_per_sample) {
        return Err(format!("不支持的采样位深: {bits_per_sample}"));
    }
    if !samples.len().is_multiple_of(channels) {
        return Err("PCM 样本不是完整的交错声道帧".to_string());
    }

    let code = telemetry_code(marker_signal_number(marker)?)?;
    let chip_len = chip_frames(sample_rate);
    let marker_len = marker_frames(sample_rate);
    let marker_offset = marker_offset_frames(sample_rate);
    let original_frames = samples.len() / channels;
    let required_frames = marker_offset + marker_len;
    let frames_added = required_frames.saturating_sub(original_frames);
    samples.resize(original_frames.max(required_frames) * channels, 0);

    let max_sample = if bits_per_sample == 32 {
        i32::MAX as f64
    } else {
        ((1i64 << (bits_per_sample - 1)) - 1) as f64
    };
    let marker_amplitude = 10.0f64.powf(config.gain_db as f64 / 20.0) * max_sample;
    let mut mixed = samples
        .iter()
        .map(|sample| *sample as f64)
        .collect::<Vec<_>>();

    for marker_frame in 0..marker_len {
        let chip_index = marker_frame / chip_len;
        let frame_in_chip = marker_frame % chip_len;
        let chip_phase = frame_in_chip as f64 / chip_len as f64;
        let envelope = (std::f64::consts::PI * chip_phase).sin().powi(2);
        let sign = code[chip_index % CODE_CHIPS] as f64;
        let time = marker_frame as f64 / sample_rate as f64;
        let carrier = (std::f64::consts::TAU * CARRIER_HZ as f64 * time).sin();
        let tagged_sample = marker_amplitude * envelope * sign * carrier;
        let output_frame = marker_offset + marker_frame;
        for channel in 0..channels {
            mixed[output_frame * channels + channel] += tagged_sample;
        }
    }

    let peak = mixed
        .iter()
        .fold(0.0f64, |current, sample| current.max(sample.abs()));
    let output_gain = if peak > max_sample {
        (max_sample / peak) as f32
    } else {
        1.0
    };
    let min_sample = -max_sample - 1.0;
    for (target, sample) in samples.iter_mut().zip(mixed) {
        *target = (sample * output_gain as f64)
            .round()
            .clamp(min_sample, max_sample) as i32;
    }

    Ok(EmbeddingReport {
        marker,
        marker_frames: marker_len,
        frames_added,
        output_gain,
    })
}

struct DetectorPlan {
    sample_rate: u32,
    chip_len: usize,
    marker_len: usize,
    step: usize,
    codes: Vec<[f32; CODE_CHIPS]>,
    chip_weights: Vec<f32>,
    weight_sum: f32,
}

impl DetectorPlan {
    fn new(sample_rate: u32) -> Self {
        let chip_len = chip_frames(sample_rate);
        let chip_weights = (0..chip_len)
            .map(|offset| {
                let phase = offset as f32 / chip_len as f32;
                (PI * phase).sin().powi(2)
            })
            .collect::<Vec<_>>();
        Self {
            sample_rate,
            chip_len,
            marker_len: marker_frames(sample_rate),
            step: (chip_len / 4).max(1),
            codes: (1..=SIGNAL_COUNT)
                .map(|number| telemetry_code(number).expect("fixed signal range is valid"))
                .collect(),
            weight_sum: chip_weights.iter().sum(),
            chip_weights,
        }
    }
}

/// 连续振荡器替代逐样本 sin/cos；重置相位只产生公共相位旋转，不影响 I/Q 合成置信度。
fn demodulate(samples: &[f32], sample_rate: u32) -> (Vec<f32>, Vec<f32>) {
    let phase_step = std::f32::consts::TAU * CARRIER_HZ / sample_rate as f32;
    let (step_sin, step_cos) = phase_step.sin_cos();
    let mut oscillator_sin = 0.0f32;
    let mut oscillator_cos = 1.0f32;
    let mut in_phase = Vec::with_capacity(samples.len());
    let mut quadrature = Vec::with_capacity(samples.len());
    for (frame, sample) in samples.iter().enumerate() {
        in_phase.push(*sample * oscillator_sin);
        quadrature.push(*sample * oscillator_cos);
        let next_sin = oscillator_sin * step_cos + oscillator_cos * step_sin;
        let next_cos = oscillator_cos * step_cos - oscillator_sin * step_sin;
        oscillator_sin = next_sin;
        oscillator_cos = next_cos;
        if frame & 2047 == 2047 {
            let norm = (oscillator_sin * oscillator_sin + oscillator_cos * oscillator_cos).sqrt();
            if norm > 0.0 {
                oscillator_sin /= norm;
                oscillator_cos /= norm;
            }
        }
    }
    (in_phase, quadrature)
}

fn detect_ready_samples(samples: &[f32], plan: &DetectorPlan, threshold: f32) -> Vec<Detection> {
    if samples.len() < plan.marker_len {
        return Vec::new();
    }
    let (demod_in_phase, demod_quadrature) = demodulate(samples, plan.sample_rate);
    let total_chips = CODE_CHIPS * CODE_REPETITIONS;
    let mut repetition_folded = [(0.0f32, 0.0f32); CODE_CHIPS];
    let mut raw = Vec::new();

    for start in (0..=samples.len() - plan.marker_len).step_by(plan.step) {
        repetition_folded.fill((0.0, 0.0));
        let mut carrier_power = 0.0f32;
        for marker_chip in 0..total_chips {
            let chip_start = start + marker_chip * plan.chip_len;
            let mut in_phase = 0.0f32;
            let mut quadrature = 0.0f32;
            for offset in 0..plan.chip_len {
                let absolute_frame = chip_start + offset;
                let weight = plan.chip_weights[offset];
                in_phase += demod_in_phase[absolute_frame] * weight;
                quadrature += demod_quadrature[absolute_frame] * weight;
            }
            if plan.weight_sum > 0.0 {
                in_phase /= plan.weight_sum;
                quadrature /= plan.weight_sum;
            }
            carrier_power += in_phase * in_phase + quadrature * quadrature;
            let folded = &mut repetition_folded[marker_chip % CODE_CHIPS];
            folded.0 += in_phase;
            folded.1 += quadrature;
        }

        let count = total_chips as f32;
        let carrier_level = (carrier_power / count).sqrt();
        if carrier_level < 0.000_01 {
            continue;
        }

        for (index, code) in plan.codes.iter().enumerate() {
            let mut in_phase = 0.0f32;
            let mut quadrature = 0.0f32;
            for (chip_index, (chip_i, chip_q)) in repetition_folded.iter().enumerate() {
                let sign = code[chip_index];
                in_phase += sign * chip_i;
                quadrature += sign * chip_q;
            }
            let confidence =
                (in_phase * in_phase + quadrature * quadrature).sqrt() / (count * carrier_level);
            if confidence >= threshold {
                if let Some(marker) = marker_from_signal_number(index as u32 + 1) {
                    raw.push(Detection {
                        marker,
                        confidence: confidence.min(1.0),
                        start_frame: start as u64,
                    });
                }
            }
        }
    }

    raw.sort_by(|left, right| {
        right
            .confidence
            .partial_cmp(&left.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let suppression_radius = plan.marker_len as u64 / 2;
    let mut selected: Vec<Detection> = Vec::new();
    for detection in raw {
        let duplicate = selected.iter().any(|existing| {
            existing.marker == detection.marker
                && existing.start_frame.abs_diff(detection.start_frame) < suppression_radius
        });
        if !duplicate {
            selected.push(detection);
        }
    }
    selected.sort_by_key(|detection| detection.start_frame);
    selected
}

/// 增量检测器只消费已经完整到达的候选起点；旧音频不会在下一轮重复计算。
pub struct StreamingDetector {
    plan: DetectorPlan,
    threshold: f32,
    buffer: Vec<f32>,
    buffer_start_frame: u64,
}

impl StreamingDetector {
    pub fn new(sample_rate: u32, threshold: f32) -> Result<Self, String> {
        if sample_rate < MIN_SAMPLE_RATE {
            return Err(format!("检测采样率最低要求 {MIN_SAMPLE_RATE}Hz"));
        }
        if !(0.0..=1.0).contains(&threshold) {
            return Err("识别阈值必须位于 0 到 1 之间".to_string());
        }
        Ok(Self {
            plan: DetectorPlan::new(sample_rate),
            threshold,
            buffer: Vec::new(),
            buffer_start_frame: 0,
        })
    }

    pub fn push(&mut self, samples: &[f32]) -> Vec<Detection> {
        self.buffer.extend_from_slice(samples);
        if self.buffer.len() < self.plan.marker_len {
            return Vec::new();
        }
        let max_start = self.buffer.len() - self.plan.marker_len;
        let candidate_count = max_start / self.plan.step + 1;
        let consumed_frames = candidate_count * self.plan.step;
        let mut detections = detect_ready_samples(&self.buffer, &self.plan, self.threshold);
        for detection in &mut detections {
            detection.start_frame += self.buffer_start_frame;
        }
        self.buffer.drain(..consumed_frames);
        self.buffer_start_frame += consumed_frames as u64;
        detections
    }

    #[cfg(test)]
    fn buffered_frames(&self) -> usize {
        self.buffer.len()
    }
}

/// 从单声道、归一化到 -1..1 的完整 PCM 中识别所有声纹事件。
pub fn detect_markers(samples: &[f32], sample_rate: u32, threshold: f32) -> Vec<Detection> {
    let Ok(mut detector) = StreamingDetector::new(sample_rate, threshold) else {
        return Vec::new();
    };
    detector.push(samples)
}

pub fn interleaved_i32_to_mono(
    samples: &[i32],
    channels: usize,
    bits_per_sample: u32,
) -> Result<Vec<f32>, String> {
    if channels == 0 || !samples.len().is_multiple_of(channels) {
        return Err("PCM 声道布局无效".to_string());
    }
    let scale = if bits_per_sample == 32 {
        i32::MAX as f32
    } else if (1..32).contains(&bits_per_sample) {
        ((1i64 << (bits_per_sample - 1)) - 1) as f32
    } else {
        return Err(format!("不支持的采样位深: {bits_per_sample}"));
    };
    Ok(samples
        .chunks_exact(channels)
        .map(|frame| {
            frame
                .iter()
                .map(|sample| *sample as f32 / scale)
                .sum::<f32>()
                / channels as f32
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rune_audio::catalog::{TelemetryMarker, RUNE_COUNT, TRACKED_LOCATIONS};

    fn rune(rune_number: u32) -> TelemetryMarker {
        TelemetryMarker::Rune { rune_number }
    }

    fn area(area_id: u32) -> TelemetryMarker {
        TelemetryMarker::Area { area_id }
    }

    #[test]
    fn gold_family_covers_runes_and_areas_with_low_cross_correlation() {
        let codes = (1..=SIGNAL_COUNT)
            .map(|number| telemetry_code(number).unwrap())
            .collect::<Vec<_>>();
        for left in 0..codes.len() {
            for right in left + 1..codes.len() {
                let max_correlation = (0..CODE_CHIPS)
                    .map(|shift| {
                        (0..CODE_CHIPS)
                            .map(|index| {
                                codes[left][index] * codes[right][(index + shift) % CODE_CHIPS]
                            })
                            .sum::<f32>()
                            .abs()
                    })
                    .fold(0.0f32, f32::max);
                assert!(
                    max_correlation <= 15.0,
                    "codes {left} and {right}: {max_correlation}"
                );
            }
        }
    }

    #[test]
    fn embedded_rune_and_area_markers_round_trip() {
        for marker in [rune(1), rune(17), rune(RUNE_COUNT), area(1), area(25)] {
            let mut samples = vec![0i32; 48_000 / 4 * 2];
            embed_marker(&mut samples, 2, 16, 48_000, marker, MarkerConfig::default()).unwrap();
            let mono = interleaved_i32_to_mono(&samples, 2, 16).unwrap();
            let detections = detect_markers(&mono, 48_000, 0.58);
            assert!(
                detections.iter().any(|item| item.marker == marker),
                "missing {marker:?}: {detections:?}"
            );
        }
    }

    #[test]
    fn marker_embedded_at_32khz_survives_resampling_to_48khz_capture() {
        let marker = area(24);
        let mut samples = vec![0i32; 32_000 / 4];
        embed_marker(&mut samples, 1, 16, 32_000, marker, MarkerConfig::default()).unwrap();
        let mono_32khz = interleaved_i32_to_mono(&samples, 1, 16).unwrap();
        let output_frames = mono_32khz.len() * 3 / 2;
        let mono_48khz = (0..output_frames)
            .map(|output_frame| {
                let source_position = output_frame as f32 * 2.0 / 3.0;
                let left = source_position.floor() as usize;
                let right = (left + 1).min(mono_32khz.len() - 1);
                let fraction = source_position - left as f32;
                mono_32khz[left] * (1.0 - fraction) + mono_32khz[right] * fraction
            })
            .collect::<Vec<_>>();
        assert!(detect_markers(&mono_48khz, 48_000, 0.58)
            .iter()
            .any(|detection| detection.marker == marker));
    }

    #[test]
    fn rune_and_area_can_be_detected_in_the_same_mix() {
        let first_marker = rune(5);
        let second_marker = area(23);
        let mut first = vec![0i32; 48_000 / 4 * 2];
        let mut second = first.clone();
        let config = MarkerConfig {
            gain_db: -22.0,
            ..MarkerConfig::default()
        };
        embed_marker(&mut first, 2, 16, 48_000, first_marker, config).unwrap();
        embed_marker(&mut second, 2, 16, 48_000, second_marker, config).unwrap();
        let mixed = first
            .iter()
            .zip(second.iter())
            .map(|(left, right)| left.saturating_add(*right))
            .collect::<Vec<_>>();
        let mono = interleaved_i32_to_mono(&mixed, 2, 16).unwrap();
        let detections = detect_markers(&mono, 48_000, 0.52);
        assert!(detections.iter().any(|item| item.marker == first_marker));
        assert!(detections.iter().any(|item| item.marker == second_marker));
    }

    #[test]
    fn same_rune_can_be_detected_twice_within_one_second() {
        let marker = rune(24);
        let mut tagged = vec![0i32; 48_000 / 5];
        embed_marker(&mut tagged, 1, 16, 48_000, marker, MarkerConfig::default()).unwrap();
        let tagged = interleaved_i32_to_mono(&tagged, 1, 16).unwrap();
        let second_start = 12_000; // 250ms；明确小于旧的 1 秒业务 CD。
        let mut sequence = vec![0.0f32; second_start + tagged.len()];
        for (index, sample) in tagged.iter().enumerate() {
            sequence[index] += sample;
            sequence[second_start + index] += sample;
        }
        let detections = detect_markers(&sequence, 48_000, 0.58)
            .into_iter()
            .filter(|detection| detection.marker == marker)
            .collect::<Vec<_>>();
        assert_eq!(detections.len(), 2, "{detections:?}");
        assert!(
            detections[1]
                .start_frame
                .abs_diff(detections[0].start_frame)
                < 48_000
        );
    }

    #[test]
    fn streaming_detector_processes_each_candidate_once_and_bounds_memory() {
        let marker = area(21);
        let mut samples = vec![0i32; 48_000 / 2];
        embed_marker(&mut samples, 1, 16, 48_000, marker, MarkerConfig::default()).unwrap();
        let mono = interleaved_i32_to_mono(&samples, 1, 16).unwrap();
        let mut detector = StreamingDetector::new(48_000, 0.58).unwrap();
        let mut detections = Vec::new();
        for chunk in mono.chunks(4_800) {
            detections.extend(detector.push(chunk));
            assert!(detector.buffered_frames() < marker_frames(48_000) + 4_800);
        }
        assert!(detections.iter().any(|item| item.marker == marker));
    }

    #[test]
    fn all_catalog_locations_fit_the_protocol() {
        assert_eq!(TRACKED_LOCATIONS.len(), 8);
        assert_eq!(SIGNAL_COUNT, 41);
        let mut samples = vec![0i32; 48_000 / 4];
        embed_marker(
            &mut samples,
            1,
            16,
            48_000,
            TelemetryMarker::Frontend,
            MarkerConfig::default(),
        )
        .unwrap();
        let mono = interleaved_i32_to_mono(&samples, 1, 16).unwrap();
        assert!(detect_markers(&mono, 48_000, 0.58)
            .iter()
            .any(|detection| detection.marker == TelemetryMarker::Frontend));
    }

    #[test]
    fn silence_does_not_produce_a_detection() {
        let samples = vec![0.0f32; marker_frames(48_000) * 2];
        assert!(detect_markers(&samples, 48_000, 0.45).is_empty());
    }
}
