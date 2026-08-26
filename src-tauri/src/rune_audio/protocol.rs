use super::catalog::{validate_marker, TelemetryMarker};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::f32::consts::PI;

pub const PROTOCOL_VERSION: u8 = 4;
pub const PREAMBLE_CHIPS: usize = 63;
pub const PAYLOAD_BITS: usize = 20;
pub const PACKET_REPETITIONS: usize = 2;
pub const PREAMBLE_CARRIER_HZ: f32 = 18_000.0;
pub const BIT_ZERO_HZ: f32 = 17_000.0;
pub const BIT_ONE_HZ: f32 = 19_000.0;
pub const CHIP_SECONDS: f32 = 0.00075;
pub const SYMBOL_SECONDS: f32 = 0.003;
pub const PREAMBLE_PAYLOAD_GAP_SECONDS: f32 = 0.002;
pub const PACKET_GAP_SECONDS: f32 = 0.004;
pub const MARKER_OFFSET_SECONDS: f32 = 0.008;
pub const MIN_SAMPLE_RATE: u32 = 44_100;

const PACKET_MAGIC: u32 = 0b10;
const TYPE_RUNE: u32 = 0b01;
const TYPE_AREA: u32 = 0b10;
const TYPE_FRONTEND: u32 = 0b11;
const PAYLOAD_HEADER_BITS: usize = 14;
const CRC_BITS: usize = 6;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MarkerConfig {
    pub gain_db: f32,
    pub detection_threshold: f32,
}

impl Default for MarkerConfig {
    fn default() -> Self {
        Self {
            gain_db: -30.0,
            detection_threshold: 0.56,
        }
    }
}

impl MarkerConfig {
    pub fn validate(self) -> Result<Self, String> {
        if !(-42.0..=-12.0).contains(&self.gain_db) {
            return Err("声纹增益必须位于 -42dBFS 到 -12dBFS 之间".to_string());
        }
        if !(0.40..=0.95).contains(&self.detection_threshold) {
            return Err("识别阈值必须位于 0.40 到 0.95 之间".to_string());
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

fn m_sequence() -> [f32; PREAMBLE_CHIPS] {
    const DEGREE: usize = 6;
    let mut state = [1u8; DEGREE + PREAMBLE_CHIPS];
    for index in 0..(PREAMBLE_CHIPS - DEGREE) {
        // Primitive polynomial x^6 + x + 1.
        state[index + DEGREE] = state[index] ^ state[index + 1];
    }
    let mut output = [0.0f32; PREAMBLE_CHIPS];
    for (target, bit) in output.iter_mut().zip(state) {
        *target = if bit == 0 { -1.0 } else { 1.0 };
    }
    output
}

fn crc6(data: u32) -> u32 {
    let mut crc = 0u32;
    for bit_index in (0..PAYLOAD_HEADER_BITS).rev() {
        let input = (data >> bit_index) & 1;
        let feedback = input ^ ((crc >> (CRC_BITS - 1)) & 1);
        crc = (crc << 1) & 0x3f;
        if feedback != 0 {
            // x^6 + x + 1, without the implicit x^6 term.
            crc ^= 0x03;
        }
    }
    crc
}

fn encode_payload(marker: TelemetryMarker) -> Result<u32, String> {
    validate_marker(marker)?;
    let (kind, id) = match marker {
        TelemetryMarker::Rune { rune_number } => (TYPE_RUNE, rune_number),
        TelemetryMarker::Area { area_id } => (TYPE_AREA, area_id),
        TelemetryMarker::Frontend => (TYPE_FRONTEND, 0),
    };
    let header = (PACKET_MAGIC << 12) | (kind << 10) | id;
    Ok((header << CRC_BITS) | crc6(header))
}

fn decode_payload(payload: u32) -> Option<TelemetryMarker> {
    let header = payload >> CRC_BITS;
    if payload & 0x3f != crc6(header) || header >> 12 != PACKET_MAGIC {
        return None;
    }
    let kind = (header >> 10) & 0b11;
    let id = header & 0x3ff;
    let marker = match kind {
        TYPE_RUNE => TelemetryMarker::Rune { rune_number: id },
        TYPE_AREA => TelemetryMarker::Area { area_id: id },
        TYPE_FRONTEND if id == 0 => TelemetryMarker::Frontend,
        _ => return None,
    };
    validate_marker(marker).ok().map(|_| marker)
}

fn chip_frames(sample_rate: u32) -> usize {
    ((sample_rate as f32 * CHIP_SECONDS).round() as usize).max(16)
}

fn symbol_frames(sample_rate: u32) -> usize {
    ((sample_rate as f32 * SYMBOL_SECONDS).round() as usize).max(64)
}

fn seconds_frames(sample_rate: u32, seconds: f32) -> usize {
    (sample_rate as f32 * seconds).round() as usize
}

fn preamble_frames(sample_rate: u32) -> usize {
    chip_frames(sample_rate) * PREAMBLE_CHIPS
}

fn payload_offset_frames(sample_rate: u32) -> usize {
    preamble_frames(sample_rate) + seconds_frames(sample_rate, PREAMBLE_PAYLOAD_GAP_SECONDS)
}

fn packet_frames(sample_rate: u32) -> usize {
    payload_offset_frames(sample_rate) + symbol_frames(sample_rate) * PAYLOAD_BITS
}

pub fn marker_frames(sample_rate: u32) -> usize {
    packet_frames(sample_rate) * PACKET_REPETITIONS
        + seconds_frames(sample_rate, PACKET_GAP_SECONDS) * (PACKET_REPETITIONS - 1)
}

fn marker_offset_frames(sample_rate: u32) -> usize {
    seconds_frames(sample_rate, MARKER_OFFSET_SECONDS)
}

fn mix_packet(
    mixed: &mut [f64],
    channels: usize,
    sample_rate: u32,
    packet_start: usize,
    payload: u32,
    amplitude: f64,
) {
    let code = m_sequence();
    let chip_len = chip_frames(sample_rate);
    for (chip_index, sign) in code.iter().enumerate() {
        for offset in 0..chip_len {
            let phase = offset as f64 / chip_len as f64;
            let envelope = (std::f64::consts::PI * phase).sin().powi(2);
            let frame = packet_start + chip_index * chip_len + offset;
            let time = frame as f64 / sample_rate as f64;
            let value = amplitude
                * envelope
                * *sign as f64
                * (std::f64::consts::TAU * PREAMBLE_CARRIER_HZ as f64 * time).sin();
            for channel in 0..channels {
                mixed[frame * channels + channel] += value;
            }
        }
    }

    let symbol_len = symbol_frames(sample_rate);
    let payload_start = packet_start + payload_offset_frames(sample_rate);
    for bit_index in 0..PAYLOAD_BITS {
        let shift = PAYLOAD_BITS - 1 - bit_index;
        let frequency = if (payload >> shift) & 1 == 0 {
            BIT_ZERO_HZ
        } else {
            BIT_ONE_HZ
        };
        for offset in 0..symbol_len {
            let phase = offset as f64 / symbol_len as f64;
            let envelope = (std::f64::consts::PI * phase).sin().powi(2);
            let frame = payload_start + bit_index * symbol_len + offset;
            let time = frame as f64 / sample_rate as f64;
            let value =
                amplitude * envelope * (std::f64::consts::TAU * frequency as f64 * time).sin();
            for channel in 0..channels {
                mixed[frame * channels + channel] += value;
            }
        }
    }
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
    let payload = encode_payload(marker)?;
    if sample_rate < MIN_SAMPLE_RATE {
        return Err(format!(
            "FLAC 采样率 {}Hz 过低；v4 超声数据包最低要求 {}Hz",
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
    let amplitude = 10.0f64.powf(config.gain_db as f64 / 20.0) * max_sample;
    let mut mixed = samples
        .iter()
        .map(|sample| *sample as f64)
        .collect::<Vec<_>>();
    let packet_len = packet_frames(sample_rate);
    let packet_gap = seconds_frames(sample_rate, PACKET_GAP_SECONDS);
    for repetition in 0..PACKET_REPETITIONS {
        mix_packet(
            &mut mixed,
            channels,
            sample_rate,
            marker_offset + repetition * (packet_len + packet_gap),
            payload,
            amplitude,
        );
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
    symbol_len: usize,
    payload_offset: usize,
    packet_len: usize,
    marker_len: usize,
    step: usize,
    code: [f32; PREAMBLE_CHIPS],
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
            symbol_len: symbol_frames(sample_rate),
            payload_offset: payload_offset_frames(sample_rate),
            packet_len: packet_frames(sample_rate),
            marker_len: marker_frames(sample_rate),
            step: (chip_len / 4).max(1),
            code: m_sequence(),
            weight_sum: chip_weights.iter().sum(),
            chip_weights,
        }
    }
}

fn demodulate(samples: &[f32], sample_rate: u32, frequency: f32) -> (Vec<f32>, Vec<f32>) {
    let phase_step = std::f32::consts::TAU * frequency / sample_rate as f32;
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

fn tone_energy(samples: &[f32], sample_rate: u32, frequency: f32) -> f32 {
    let phase_step = std::f32::consts::TAU * frequency / sample_rate as f32;
    let mut in_phase = 0.0f32;
    let mut quadrature = 0.0f32;
    for (index, sample) in samples.iter().enumerate() {
        let phase = index as f32 * phase_step;
        let weight = (PI * index as f32 / samples.len() as f32).sin().powi(2);
        let (sin, cos) = phase.sin_cos();
        in_phase += *sample * weight * sin;
        quadrature += *sample * weight * cos;
    }
    in_phase * in_phase + quadrature * quadrature
}

fn decode_candidate(
    samples: &[f32],
    plan: &DetectorPlan,
    start: usize,
) -> Option<(TelemetryMarker, f32)> {
    let mut payload = 0u32;
    let mut quality_sum = 0.0f32;
    let payload_start = start + plan.payload_offset;
    for bit_index in 0..PAYLOAD_BITS {
        let symbol_start = payload_start + bit_index * plan.symbol_len;
        let symbol = &samples[symbol_start..symbol_start + plan.symbol_len];
        let zero = tone_energy(symbol, plan.sample_rate, BIT_ZERO_HZ);
        let one = tone_energy(symbol, plan.sample_rate, BIT_ONE_HZ);
        let total = zero + one;
        if total < 1e-10 {
            return None;
        }
        let quality = (zero - one).abs() / total;
        quality_sum += quality;
        payload = (payload << 1) | u32::from(one > zero);
    }
    let bit_quality = quality_sum / PAYLOAD_BITS as f32;
    if bit_quality < 0.12 {
        return None;
    }
    decode_payload(payload).map(|marker| (marker, bit_quality))
}

fn detect_ready_samples(samples: &[f32], plan: &DetectorPlan, threshold: f32) -> Vec<Detection> {
    if samples.len() < plan.packet_len {
        return Vec::new();
    }
    let (demod_i, demod_q) = demodulate(samples, plan.sample_rate, PREAMBLE_CARRIER_HZ);
    let mut raw = Vec::new();
    for start in (0..=samples.len() - plan.packet_len).step_by(plan.step) {
        let mut correlation_i = 0.0f32;
        let mut correlation_q = 0.0f32;
        let mut carrier_power = 0.0f32;
        for chip_index in 0..PREAMBLE_CHIPS {
            let chip_start = start + chip_index * plan.chip_len;
            let mut chip_i = 0.0f32;
            let mut chip_q = 0.0f32;
            for offset in 0..plan.chip_len {
                let frame = chip_start + offset;
                let weight = plan.chip_weights[offset];
                chip_i += demod_i[frame] * weight;
                chip_q += demod_q[frame] * weight;
            }
            chip_i /= plan.weight_sum;
            chip_q /= plan.weight_sum;
            carrier_power += chip_i * chip_i + chip_q * chip_q;
            correlation_i += plan.code[chip_index] * chip_i;
            correlation_q += plan.code[chip_index] * chip_q;
        }
        let carrier_level = (carrier_power / PREAMBLE_CHIPS as f32).sqrt();
        if carrier_level < 0.000_005 {
            continue;
        }
        let preamble_confidence = (correlation_i * correlation_i + correlation_q * correlation_q)
            .sqrt()
            / (PREAMBLE_CHIPS as f32 * carrier_level);
        if preamble_confidence < threshold {
            continue;
        }
        if let Some((marker, bit_quality)) = decode_candidate(samples, plan, start) {
            raw.push(Detection {
                marker,
                confidence: (preamble_confidence * 0.75 + bit_quality * 0.25).min(1.0),
                start_frame: start as u64,
            });
        }
    }

    raw.sort_by(|left, right| {
        right
            .confidence
            .partial_cmp(&left.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let suppression_radius = plan.marker_len as u64;
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

/// Incremental detector. CRC validation and packet-level NMS prevent ordinary
/// game audio and the protocol's second copy from becoming logical events.
pub struct StreamingDetector {
    plan: DetectorPlan,
    threshold: f32,
    buffer: Vec<f32>,
    buffer_start_frame: u64,
    last_detections: HashMap<TelemetryMarker, u64>,
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
            last_detections: HashMap::new(),
        })
    }

    pub fn push(&mut self, samples: &[f32]) -> Vec<Detection> {
        self.buffer.extend_from_slice(samples);
        if self.buffer.len() < self.plan.packet_len {
            return Vec::new();
        }
        let max_start = self.buffer.len() - self.plan.packet_len;
        let candidate_count = max_start / self.plan.step + 1;
        let consumed_frames = candidate_count * self.plan.step;
        let mut detections = detect_ready_samples(&self.buffer, &self.plan, self.threshold);
        for detection in &mut detections {
            detection.start_frame += self.buffer_start_frame;
        }
        self.buffer.drain(..consumed_frames);
        self.buffer_start_frame += consumed_frames as u64;

        detections.retain(|detection| {
            let duplicate = self
                .last_detections
                .get(&detection.marker)
                .is_some_and(|last| {
                    detection.start_frame.abs_diff(*last) < self.plan.marker_len as u64
                });
            if !duplicate {
                self.last_detections
                    .insert(detection.marker, detection.start_frame);
            }
            !duplicate
        });
        detections
    }

    #[cfg(test)]
    fn buffered_frames(&self) -> usize {
        self.buffer.len()
    }
}

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

    fn rune(number: u32) -> TelemetryMarker {
        TelemetryMarker::Rune {
            rune_number: number,
        }
    }

    fn area(id: u32) -> TelemetryMarker {
        TelemetryMarker::Area { area_id: id }
    }

    fn tagged(marker: TelemetryMarker, sample_rate: u32, gain_db: f32) -> Vec<f32> {
        let mut samples = vec![0i32; sample_rate as usize / 3];
        embed_marker(
            &mut samples,
            1,
            16,
            sample_rate,
            marker,
            MarkerConfig {
                gain_db,
                ..MarkerConfig::default()
            },
        )
        .unwrap();
        interleaved_i32_to_mono(&samples, 1, 16).unwrap()
    }

    #[test]
    fn payload_round_trips_runes_full_area_range_and_frontend() {
        for marker in [
            rune(1),
            rune(33),
            area(1),
            area(137),
            area(1023),
            TelemetryMarker::Frontend,
        ] {
            let payload = encode_payload(marker).unwrap();
            assert_eq!(decode_payload(payload), Some(marker));
            assert_eq!(decode_payload(payload ^ 1), None);
        }
    }

    #[test]
    fn embedded_packets_round_trip_at_48khz() {
        for marker in [
            rune(1),
            rune(24),
            rune(33),
            area(1),
            area(137),
            TelemetryMarker::Frontend,
        ] {
            let mono = tagged(marker, 48_000, -30.0);
            let detections = detect_markers(&mono, 48_000, 0.56);
            assert_eq!(detections.len(), 1, "{marker:?}: {detections:?}");
            assert_eq!(detections[0].marker, marker);
            assert!(detections[0].confidence > 0.70);
        }
    }

    #[test]
    fn marker_survives_44100_to_48000_resampling() {
        let marker = area(99);
        let source = tagged(marker, 44_100, -26.0);
        let output_frames = source.len() * 160 / 147;
        let resampled = (0..output_frames)
            .map(|output| {
                let position = output as f32 * 147.0 / 160.0;
                let left = position.floor() as usize;
                let right = (left + 1).min(source.len() - 1);
                source[left] * (1.0 - position.fract()) + source[right] * position.fract()
            })
            .collect::<Vec<_>>();
        assert!(detect_markers(&resampled, 48_000, 0.50)
            .iter()
            .any(|detection| detection.marker == marker));
    }

    #[test]
    fn packet_survives_busy_audio_and_attenuation() {
        let marker = rune(31);
        let signal = tagged(marker, 48_000, -26.0);
        let mixed = signal
            .iter()
            .enumerate()
            .map(|(index, sample)| {
                let time = index as f32 / 48_000.0;
                *sample * 0.25
                    + (std::f32::consts::TAU * 440.0 * time).sin() * 0.20
                    + (std::f32::consts::TAU * 4_700.0 * time).sin() * 0.08
            })
            .collect::<Vec<_>>();
        assert!(detect_markers(&mixed, 48_000, 0.48)
            .iter()
            .any(|detection| detection.marker == marker));
    }

    #[test]
    fn ordinary_tones_do_not_pass_preamble_and_crc() {
        let samples = (0..48_000)
            .map(|index| {
                let time = index as f32 / 48_000.0;
                (std::f32::consts::TAU * 18_000.0 * time).sin() * 0.25
                    + (std::f32::consts::TAU * 19_000.0 * time).sin() * 0.10
            })
            .collect::<Vec<_>>();
        assert!(detect_markers(&samples, 48_000, 0.45).is_empty());
    }

    #[test]
    fn same_rune_is_detected_twice_250ms_apart() {
        let marker = rune(24);
        let signal = tagged(marker, 48_000, -26.0);
        let second_start = 12_000;
        let mut sequence = vec![0.0f32; second_start + signal.len()];
        for (index, sample) in signal.iter().enumerate() {
            sequence[index] += sample;
            sequence[second_start + index] += sample;
        }
        let detections = detect_markers(&sequence, 48_000, 0.50)
            .into_iter()
            .filter(|detection| detection.marker == marker)
            .collect::<Vec<_>>();
        assert_eq!(detections.len(), 2, "{detections:?}");
    }

    #[test]
    fn streaming_detector_bounds_memory_and_does_not_repeat_second_copy() {
        let marker = area(137);
        let signal = tagged(marker, 48_000, -26.0);
        let mut detector = StreamingDetector::new(48_000, 0.50).unwrap();
        let mut detections = Vec::new();
        for chunk in signal.chunks(997) {
            detections.extend(detector.push(chunk));
            assert!(detector.buffered_frames() < packet_frames(48_000) + 997);
        }
        assert_eq!(detections.len(), 1, "{detections:?}");
        assert_eq!(detections[0].marker, marker);
    }
}
