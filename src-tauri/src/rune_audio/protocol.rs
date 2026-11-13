use super::catalog::{validate_marker, TelemetryMarker, MAX_ITEM_ID, RUNE_COUNT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::f32::consts::PI;

pub const PROTOCOL_VERSION: u8 = 7;
pub const PREAMBLE_CHIPS: usize = 63;
pub const RUNE_SIGNATURE_CHIPS: usize = 127;
pub const PAYLOAD_BITS: usize = 20;
pub const PACKET_REPETITIONS: usize = 2;
pub const PREAMBLE_CARRIER_HZ: f32 = 18_000.0;
pub const RUNE_SIGNATURE_CARRIER_HZ: f32 = 19_600.0;
pub const BIT_ZERO_HZ: f32 = 17_000.0;
pub const BIT_ONE_HZ: f32 = 19_000.0;
pub const CHIP_SECONDS: f32 = 0.00075;
pub const SYMBOL_SECONDS: f32 = 0.003;
pub const PREAMBLE_PAYLOAD_GAP_SECONDS: f32 = 0.002;
pub const PACKET_GAP_SECONDS: f32 = 0.004;
pub const MARKER_OFFSET_SECONDS: f32 = 0.008;
pub const MIN_SAMPLE_RATE: u32 = 44_100;

const PACKET_MAGIC: u32 = 0b10;
const TYPE_ITEM: u32 = 0b00;
const TYPE_RUNE: u32 = 0b01;
const TYPE_AREA: u32 = 0b10;
const TYPE_FRONTEND: u32 = 0b11;
const PAYLOAD_HEADER_BITS: usize = 14;
const CRC_BITS: usize = 6;
const DROP_SIGNATURE_COUNT: usize = (RUNE_COUNT + MAX_ITEM_ID) as usize;

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
    /// Independently decoded copies at the physical packet repetition interval.
    #[serde(default)]
    pub repeated: bool,
}

fn m_sequence_63(feedback_mask: u8) -> [f32; PREAMBLE_CHIPS] {
    const DEGREE: usize = 6;
    let mut state = [1u8; DEGREE + PREAMBLE_CHIPS];
    for index in 0..(PREAMBLE_CHIPS - DEGREE) {
        state[index + DEGREE] = (0..DEGREE)
            .filter(|tap| feedback_mask & (1 << tap) != 0)
            .fold(0, |value, tap| value ^ state[index + tap]);
    }
    let mut output = [0.0f32; PREAMBLE_CHIPS];
    for (target, bit) in output.iter_mut().zip(state) {
        *target = if bit == 0 { -1.0 } else { 1.0 };
    }
    output
}

fn control_preamble() -> [f32; PREAMBLE_CHIPS] {
    // Primitive polynomial x^6 + x + 1.
    m_sequence_63(0b00_0011)
}

fn rune_preamble() -> [f32; PREAMBLE_CHIPS] {
    // A preferred degree-six sequence. Its cyclic cross-correlation with the
    // location sequence never exceeds 17 / 63, so a valid location packet can
    // no longer unlock the rune decoder.
    m_sequence_63(0b01_1011)
}

fn m_sequence_127(feedback_mask: u8) -> [f32; RUNE_SIGNATURE_CHIPS] {
    const DEGREE: usize = 7;
    let mut state = [1u8; DEGREE + RUNE_SIGNATURE_CHIPS];
    for index in 0..(RUNE_SIGNATURE_CHIPS - DEGREE) {
        state[index + DEGREE] = (0..DEGREE)
            .filter(|tap| feedback_mask & (1 << tap) != 0)
            .fold(0, |value, tap| value ^ state[index + tap]);
    }
    let mut output = [0.0f32; RUNE_SIGNATURE_CHIPS];
    for (target, bit) in output.iter_mut().zip(state) {
        *target = if bit == 0 { -1.0 } else { 1.0 };
    }
    output
}

fn rune_signature(rune_number: u32) -> [f32; RUNE_SIGNATURE_CHIPS] {
    // Degree-seven Gold codes have bounded cyclic auto/cross-correlation
    // (17 / 127 here). Unlike v5's cyclic shifts of one sequence, a one-chip
    // echo of R20 therefore cannot masquerade as R19 or R21.
    let left = m_sequence_127(0b000_0011);
    let right = m_sequence_127(0b000_1001);
    let shift = rune_number.saturating_sub(1) as usize % RUNE_SIGNATURE_CHIPS;
    let mut output = [0.0f32; RUNE_SIGNATURE_CHIPS];
    for (index, target) in output.iter_mut().enumerate() {
        *target = left[index] * right[(index + shift) % RUNE_SIGNATURE_CHIPS];
    }
    output
}

fn drop_signature_number(marker: TelemetryMarker) -> Option<u32> {
    match marker {
        TelemetryMarker::Rune { rune_number } => Some(rune_number),
        TelemetryMarker::Item { item_id } => Some(RUNE_COUNT + item_id),
        TelemetryMarker::Area { .. } | TelemetryMarker::Frontend | TelemetryMarker::Hostile => None,
    }
}

fn marker_from_drop_signature(signature_number: u32) -> Option<TelemetryMarker> {
    if (1..=RUNE_COUNT).contains(&signature_number) {
        Some(TelemetryMarker::Rune {
            rune_number: signature_number,
        })
    } else {
        let item_id = signature_number.checked_sub(RUNE_COUNT)?;
        (1..=MAX_ITEM_ID)
            .contains(&item_id)
            .then_some(TelemetryMarker::Item { item_id })
    }
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
        TelemetryMarker::Item { item_id } => (TYPE_ITEM, item_id),
        TelemetryMarker::Rune { rune_number } => (TYPE_RUNE, rune_number),
        TelemetryMarker::Area { area_id } => (TYPE_AREA, area_id),
        TelemetryMarker::Frontend => (TYPE_FRONTEND, 0),
        TelemetryMarker::Hostile => (TYPE_FRONTEND, 1),
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
        TYPE_ITEM => TelemetryMarker::Item { item_id: id },
        TYPE_RUNE => TelemetryMarker::Rune { rune_number: id },
        TYPE_AREA => TelemetryMarker::Area { area_id: id },
        TYPE_FRONTEND if id == 0 => TelemetryMarker::Frontend,
        TYPE_FRONTEND if id == 1 => TelemetryMarker::Hostile,
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
    payload_offset_frames(sample_rate)
        + (symbol_frames(sample_rate) * PAYLOAD_BITS)
            .max(chip_frames(sample_rate) * RUNE_SIGNATURE_CHIPS)
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
    marker: TelemetryMarker,
    payload: u32,
    amplitude: f64,
) {
    let scene = matches!(
        marker,
        TelemetryMarker::Area { .. } | TelemetryMarker::Frontend
    );
    let preamble_hz = if marker == TelemetryMarker::Frontend {
        20_600.0
    } else if scene {
        21_000.0
    } else {
        PREAMBLE_CARRIER_HZ
    };
    let code = if drop_signature_number(marker).is_some() {
        rune_preamble()
    } else {
        control_preamble()
    };
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
                * (std::f64::consts::TAU * preamble_hz as f64 * time).sin();
            for channel in 0..channels {
                mixed[frame * channels + channel] += value;
            }
        }
    }

    let payload_start = packet_start + payload_offset_frames(sample_rate);
    if let Some(signature_number) = drop_signature_number(marker) {
        let signature = rune_signature(signature_number);
        for (chip_index, sign) in signature.iter().enumerate() {
            for offset in 0..chip_len {
                let phase = offset as f64 / chip_len as f64;
                let envelope = (std::f64::consts::PI * phase).sin().powi(2);
                let frame = payload_start + chip_index * chip_len + offset;
                let time = frame as f64 / sample_rate as f64;
                let value = amplitude
                    * envelope
                    * *sign as f64
                    * (std::f64::consts::TAU * RUNE_SIGNATURE_CARRIER_HZ as f64 * time).sin();
                for channel in 0..channels {
                    mixed[frame * channels + channel] += value;
                }
            }
        }
        return;
    }

    let symbol_len = symbol_frames(sample_rate);
    for bit_index in 0..PAYLOAD_BITS {
        let shift = PAYLOAD_BITS - 1 - bit_index;
        let frequency = if (payload >> shift) & 1 == 0 {
            if marker == TelemetryMarker::Frontend {
                20_100.0
            } else if scene {
                20_500.0
            } else {
                BIT_ZERO_HZ
            }
        } else {
            if marker == TelemetryMarker::Frontend {
                21_100.0
            } else if scene {
                21_500.0
            } else {
                BIT_ONE_HZ
            }
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
    embed_marker_with_delay(
        samples,
        channels,
        bits_per_sample,
        sample_rate,
        marker,
        config,
        0.0,
    )
}

pub fn embed_marker_with_delay(
    samples: &mut Vec<i32>,
    channels: usize,
    bits_per_sample: u32,
    sample_rate: u32,
    marker: TelemetryMarker,
    config: MarkerConfig,
    additional_delay_seconds: f32,
) -> Result<EmbeddingReport, String> {
    let config = config.validate()?;
    let payload = encode_payload(marker)?;
    if !additional_delay_seconds.is_finite() || !(0.0..=30.0).contains(&additional_delay_seconds) {
        return Err("声纹附加延迟必须位于 0 到 30 秒之间".to_string());
    }
    if sample_rate < MIN_SAMPLE_RATE {
        return Err(format!(
            "FLAC 采样率 {}Hz 过低；v7 超声数据包最低要求 {}Hz",
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
    let marker_offset =
        marker_offset_frames(sample_rate) + seconds_frames(sample_rate, additional_delay_seconds);
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
            marker,
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
    preamble_hz: f32,
    zero_hz: f32,
    one_hz: f32,
    sample_rate: u32,
    chip_len: usize,
    symbol_len: usize,
    payload_offset: usize,
    packet_len: usize,
    marker_len: usize,
    step: usize,
    control_code: [f32; PREAMBLE_CHIPS],
    rune_code: [f32; PREAMBLE_CHIPS],
    rune_signatures: [[f32; RUNE_SIGNATURE_CHIPS]; DROP_SIGNATURE_COUNT],
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
            preamble_hz: PREAMBLE_CARRIER_HZ,
            zero_hz: BIT_ZERO_HZ,
            one_hz: BIT_ONE_HZ,
            control_code: control_preamble(),
            rune_code: rune_preamble(),
            rune_signatures: std::array::from_fn(|index| rune_signature(index as u32 + 1)),
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
        let zero = tone_energy(symbol, plan.sample_rate, plan.zero_hz);
        let one = tone_energy(symbol, plan.sample_rate, plan.one_hz);
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

// Fit simultaneous Gold signatures before judging any one identity. All
// thresholds below refer to identity evidence, never the shared preamble.
fn separate_drop_signatures(
    chip_i: &[f32; RUNE_SIGNATURE_CHIPS],
    chip_q: &[f32; RUNE_SIGNATURE_CHIPS],
    plan: &DetectorPlan,
    threshold: f32,
) -> Vec<(TelemetryMarker, f32)> {
    const N: usize = RUNE_SIGNATURE_CHIPS;
    const MAX_COMPONENTS: usize = 16;
    let mut residual_i = *chip_i;
    let mut residual_q = *chip_q;
    let original_power: f32 = (0..N)
        .map(|k| chip_i[k] * chip_i[k] + chip_q[k] * chip_q[k])
        .sum();
    if original_power / (N as f32) < 0.000_005f32.powi(2) {
        return Vec::new();
    }
    let mut components: Vec<(usize, f32, f32)> = Vec::new();
    for _ in 0..MAX_COMPONENTS {
        let power: f32 = (0..N)
            .map(|k| residual_i[k].powi(2) + residual_q[k].powi(2))
            .sum();
        // Do not promote numerical cancellation residue to a new identity.
        if power < original_power * 0.0001 {
            break;
        }
        let mut best = (0, 0.0f32, 0.0f32, 0.0f32);
        for (index, code) in plan.rune_signatures.iter().enumerate() {
            if components.iter().any(|c| c.0 == index) {
                continue;
            }
            let i: f32 = (0..N).map(|k| code[k] * residual_i[k]).sum();
            let q: f32 = (0..N).map(|k| code[k] * residual_q[k]).sum();
            let score = (i * i + q * q) / (N as f32 * power);
            if score > best.3 {
                best = (index, i / N as f32, q / N as f32, score);
            }
        }
        // This is candidate admission only. Each fitted component still has
        // to pass full-length and two independent temporal-support checks.
        if best.3 < 0.24f32.powi(2) {
            break;
        }
        components.push((best.0, 0.0, 0.0));
        // Coordinate least squares removes cross-talk between nonorthogonal
        // Gold codes instead of treating the first subtraction as exact.
        for _ in 0..6 {
            for component in &mut components {
                let code = &plan.rune_signatures[component.0];
                let di = (0..N).map(|k| code[k] * residual_i[k]).sum::<f32>() / N as f32;
                let dq = (0..N).map(|k| code[k] * residual_q[k]).sum::<f32>() / N as f32;
                component.1 += di;
                component.2 += dq;
                for k in 0..N {
                    residual_i[k] -= di * code[k];
                    residual_q[k] -= dq * code[k];
                }
            }
        }
    }
    components
        .iter()
        .filter_map(|&(index, ai, aq)| {
            let code = &plan.rune_signatures[index];
            let mut correlations = [(0.0f32, 0.0f32, 0.0f32); 2];
            for k in 0..N {
                let i = residual_i[k] + ai * code[k];
                let q = residual_q[k] + aq * code[k];
                let part = usize::from(k >= N / 2);
                correlations[part].0 += code[k] * i;
                correlations[part].1 += code[k] * q;
                correlations[part].2 += i * i + q * q;
            }
            let [(i0, q0, p0), (i1, q1, p1)] = correlations;
            let a0 = i0.hypot(q0);
            let a1 = i1.hypot(q1);
            let score = (i0 + i1).hypot(q0 + q1) / (N as f32 * (p0 + p1)).sqrt();
            let first = a0 / ((N / 2) as f32 * p0).sqrt();
            let second = a1 / ((N - N / 2) as f32 * p1).sqrt();
            let phase_agreement = (i0 * i1 + q0 * q1) / (a0 * a1);
            // A localized burst, a side lobe or an unrelated phase in one half
            // must not become a high-confidence rune after a strong sync header.
            (score >= threshold.max(0.56)
                && first >= 0.40
                && second >= 0.40
                && phase_agreement >= 0.5
                && N as f32 * (ai * ai + aq * aq) >= original_power * 0.0001)
                .then(|| {
                    marker_from_drop_signature(index as u32 + 1)
                        .map(|marker| (marker, score.min(1.0)))
                })?
        })
        .collect()
}

fn decode_rune_signatures(
    demod_i: &[f32],
    demod_q: &[f32],
    plan: &DetectorPlan,
    start: usize,
    threshold: f32,
) -> Vec<(TelemetryMarker, f32)> {
    let signature_start = start + plan.payload_offset;
    let mut chip_i = [0.0f32; RUNE_SIGNATURE_CHIPS];
    let mut chip_q = [0.0f32; RUNE_SIGNATURE_CHIPS];
    let mut carrier_power = 0.0f32;
    for chip_index in 0..RUNE_SIGNATURE_CHIPS {
        let chip_start = signature_start + chip_index * plan.chip_len;
        for (offset, &weight) in plan.chip_weights.iter().enumerate().take(plan.chip_len) {
            let frame = chip_start + offset;
            chip_i[chip_index] += demod_i[frame] * weight;
            chip_q[chip_index] += demod_q[frame] * weight;
        }
        chip_i[chip_index] /= plan.weight_sum;
        chip_q[chip_index] /= plan.weight_sum;
        carrier_power +=
            chip_i[chip_index] * chip_i[chip_index] + chip_q[chip_index] * chip_q[chip_index];
    }
    let carrier_level = (carrier_power / RUNE_SIGNATURE_CHIPS as f32).sqrt();
    if carrier_level < 0.000_005 {
        return Vec::new();
    }

    separate_drop_signatures(&chip_i, &chip_q, plan, threshold)
}

// Estimate carrier phase and gain from the received samples, not from the
// configured embedding gain (game volume and resampling change both).
fn fit_drop_segment(
    samples: &[f32],
    plan: &DetectorPlan,
    start: usize,
    code: &[f32],
    frequency: f32,
) -> (f32, f32, f32) {
    let mut ss = 0.0;
    let mut cc = 0.0;
    let mut sc = 0.0;
    let mut ys = 0.0;
    let mut yc = 0.0;
    let mut yy = 0.0;
    let mut chip_s = 0.0;
    let mut chip_c = 0.0;
    let mut chip_power = 0.0;
    for (k, &sample) in samples[start..start + code.len() * plan.chip_len]
        .iter()
        .enumerate()
    {
        let envelope = plan.chip_weights[k % plan.chip_len] * code[k / plan.chip_len];
        let (sin, cos) =
            (std::f32::consts::TAU * frequency * k as f32 / plan.sample_rate as f32).sin_cos();
        let s = envelope * sin;
        let c = envelope * cos;
        ss += s * s;
        cc += c * c;
        sc += s * c;
        ys += sample * s;
        yc += sample * c;
        yy += sample * sample;
        chip_s += sample * s;
        chip_c += sample * c;
        if (k + 1) % plan.chip_len == 0 {
            chip_power += chip_s * chip_s + chip_c * chip_c;
            chip_s = 0.0;
            chip_c = 0.0;
        }
    }
    let determinant = ss * cc - sc * sc;
    if determinant <= 1e-12 || yy <= 1e-16 {
        return (0.0, 0.0, 0.0);
    }
    let a = (ys * cc - yc * sc) / determinant;
    let b = (yc * ss - ys * sc) / determinant;
    (
        a,
        b,
        ((ys * ys + yc * yc) / (code.len() as f32 * chip_power).max(1e-20)).sqrt(),
    )
}

fn cancel_drop_packet(samples: &mut [f32], plan: &DetectorPlan, detection: &Detection) -> bool {
    let Some(id) = drop_signature_number(detection.marker) else {
        return false;
    };
    let code = &plan.rune_signatures[id as usize - 1];
    let nominal = detection.start_frame as usize;
    let begin = nominal.saturating_sub(plan.step);
    let end = (nominal + plan.step).min(samples.len() - plan.packet_len);
    let mut best = (nominal, 0.0f32);
    for start in begin..=end {
        let (_, _, quality) = fit_drop_segment(
            samples,
            plan,
            start + plan.payload_offset,
            code,
            RUNE_SIGNATURE_CARRIER_HZ,
        );
        if quality > best.1 {
            best = (start, quality);
        }
    }
    // Require strong identity evidence before cancellation. Broadband energy
    // is not a rejection gate: loud low-frequency game audio is orthogonal
    // to this template and must not block recovery of a high-frequency packet.
    if best.1 < 0.66 || detection.confidence < 0.66 {
        return false;
    }
    for (offset, code, carrier) in [
        (0, plan.rune_code.as_slice(), PREAMBLE_CARRIER_HZ),
        (
            plan.payload_offset,
            code.as_slice(),
            RUNE_SIGNATURE_CARRIER_HZ,
        ),
    ] {
        let start = best.0 + offset;
        let (a, b, _) = fit_drop_segment(samples, plan, start, code, carrier);
        for k in 0..code.len() * plan.chip_len {
            let envelope = plan.chip_weights[k % plan.chip_len] * code[k / plan.chip_len];
            let (sin, cos) =
                (std::f32::consts::TAU * carrier * k as f32 / plan.sample_rate as f32).sin_cos();
            samples[start + k] -= envelope * (a * sin + b * cos);
        }
    }
    true
}

fn detect_ready_samples(samples: &[f32], plan: &DetectorPlan, threshold: f32) -> Vec<Detection> {
    let mut detections = detect_pass(samples, plan, threshold);
    if !detections
        .iter()
        .any(|d| drop_signature_number(d.marker).is_some())
    {
        return detections;
    }
    let mut residual = samples.to_vec();
    let mut cancelled = Vec::<(TelemetryMarker, u64)>::new();
    for _ in 0..8 {
        let mut changed = false;
        let mut strongest: Vec<_> = detections
            .iter()
            .filter(|d| drop_signature_number(d.marker).is_some())
            .collect();
        strongest.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
        for detection in strongest {
            // Both physical repetitions can mask another drop. They are
            // separate cancellation targets but still one emitted event.
            let stride =
                (plan.packet_len + seconds_frames(plan.sample_rate, PACKET_GAP_SECONDS)) as u64;
            for start in [
                Some(detection.start_frame),
                detection.start_frame.checked_sub(stride),
                detection.start_frame.checked_add(stride),
            ]
            .into_iter()
            .flatten()
            {
                if start as usize + plan.packet_len > residual.len()
                    || cancelled.iter().any(|(marker, at)| {
                        *marker == detection.marker && at.abs_diff(start) < plan.chip_len as u64
                    })
                {
                    continue;
                }
                let packet = Detection {
                    marker: detection.marker,
                    confidence: detection.confidence,
                    start_frame: start,
                    repeated: false,
                };
                if cancel_drop_packet(&mut residual, plan, &packet) {
                    cancelled.push((packet.marker, start));
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
        let recovered = detect_pass(&residual, plan, threshold);
        let mut added = false;
        for detection in recovered {
            if drop_signature_number(detection.marker).is_none() {
                continue;
            }
            if !detections.iter().any(|d| {
                d.marker == detection.marker
                    && d.start_frame.abs_diff(detection.start_frame) < plan.marker_len as u64
            }) {
                detections.push(detection);
                added = true;
            }
        }
        if !added {
            break;
        }
    }
    detections.sort_by_key(|d| d.start_frame);
    detections
}

fn detect_pass(samples: &[f32], plan: &DetectorPlan, threshold: f32) -> Vec<Detection> {
    if samples.len() < plan.packet_len {
        return Vec::new();
    }
    let (demod_i, demod_q) = demodulate(samples, plan.sample_rate, plan.preamble_hz);
    let (signature_i, signature_q) = if plan.preamble_hz == PREAMBLE_CARRIER_HZ {
        demodulate(samples, plan.sample_rate, RUNE_SIGNATURE_CARRIER_HZ)
    } else {
        (Vec::new(), Vec::new())
    };
    let mut raw = Vec::new();
    for start in (0..=samples.len() - plan.packet_len).step_by(plan.step) {
        let mut control_correlation_i = 0.0f32;
        let mut control_correlation_q = 0.0f32;
        let mut rune_correlation_i = 0.0f32;
        let mut rune_correlation_q = 0.0f32;
        let mut carrier_power = 0.0f32;
        for chip_index in 0..PREAMBLE_CHIPS {
            let chip_start = start + chip_index * plan.chip_len;
            let mut chip_i = 0.0f32;
            let mut chip_q = 0.0f32;
            for (offset, &weight) in plan.chip_weights.iter().enumerate().take(plan.chip_len) {
                let frame = chip_start + offset;
                chip_i += demod_i[frame] * weight;
                chip_q += demod_q[frame] * weight;
            }
            chip_i /= plan.weight_sum;
            chip_q /= plan.weight_sum;
            carrier_power += chip_i * chip_i + chip_q * chip_q;
            control_correlation_i += plan.control_code[chip_index] * chip_i;
            control_correlation_q += plan.control_code[chip_index] * chip_q;
            rune_correlation_i += plan.rune_code[chip_index] * chip_i;
            rune_correlation_q += plan.rune_code[chip_index] * chip_q;
        }
        let carrier_level = (carrier_power / PREAMBLE_CHIPS as f32).sqrt();
        if carrier_level < 0.000_005 {
            continue;
        }
        let control_preamble_confidence = (control_correlation_i * control_correlation_i
            + control_correlation_q * control_correlation_q)
            .sqrt()
            / (PREAMBLE_CHIPS as f32 * carrier_level);
        let rune_preamble_confidence = (rune_correlation_i * rune_correlation_i
            + rune_correlation_q * rune_correlation_q)
            .sqrt()
            / (PREAMBLE_CHIPS as f32 * carrier_level);
        if control_preamble_confidence < threshold && rune_preamble_confidence < threshold {
            continue;
        }
        if control_preamble_confidence >= threshold {
            if let Some((marker, bit_quality)) = decode_candidate(samples, plan, start) {
                if matches!(
                    marker,
                    TelemetryMarker::Hostile
                        | TelemetryMarker::Area { .. }
                        | TelemetryMarker::Frontend
                ) {
                    raw.push(Detection {
                        marker,
                        confidence: (control_preamble_confidence * 0.75 + bit_quality * 0.25)
                            .min(1.0),
                        start_frame: start as u64,
                        repeated: false,
                    });
                }
            }
        }
        if plan.preamble_hz == PREAMBLE_CARRIER_HZ && rune_preamble_confidence >= threshold {
            for (marker, signature_confidence) in
                decode_rune_signatures(&signature_i, &signature_q, plan, start, threshold)
            {
                let confidence = rune_preamble_confidence.min(signature_confidence).min(1.0);
                if confidence >= threshold {
                    raw.push(Detection {
                        marker,
                        confidence,
                        start_frame: start as u64,
                        repeated: false,
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
    let suppression_radius = plan.marker_len as u64;
    let mut selected: Vec<Detection> = Vec::new();
    let stride = (plan.packet_len + seconds_frames(plan.sample_rate, PACKET_GAP_SECONDS)) as u64;
    let paired: Vec<bool> = raw
        .iter()
        .map(|d| {
            drop_signature_number(d.marker).is_some()
                && raw.iter().any(|other| {
                    other.marker == d.marker
                        && other.start_frame.abs_diff(d.start_frame).abs_diff(stride)
                            <= plan.step as u64 * 2
                })
        })
        .collect();
    for (mut detection, repeated) in raw.into_iter().zip(paired) {
        detection.repeated = repeated;
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

/// Incremental detector. Control packets use CRC; drops use separated Gold
/// signatures with temporal support checks. NMS deduplicates repeated packets.
pub struct StreamingDetector {
    plan: DetectorPlan,
    scene_plan: DetectorPlan,
    frontend_plan: DetectorPlan,
    threshold: f32,
    buffer: Vec<f32>,
    buffer_start_frame: u64,
    last_detections: HashMap<TelemetryMarker, u64>,
    pending_confirmation: HashMap<TelemetryMarker, u64>,
}

impl StreamingDetector {
    pub fn new(sample_rate: u32, threshold: f32) -> Result<Self, String> {
        if sample_rate < MIN_SAMPLE_RATE {
            return Err(format!("检测采样率最低要求 {MIN_SAMPLE_RATE}Hz"));
        }
        if !(0.0..=1.0).contains(&threshold) {
            return Err("识别阈值必须位于 0 到 1 之间".to_string());
        }
        let mut scene_plan = DetectorPlan::new(sample_rate);
        scene_plan.preamble_hz = 21_000.0;
        scene_plan.zero_hz = 20_500.0;
        scene_plan.one_hz = 21_500.0;
        let mut frontend_plan = DetectorPlan::new(sample_rate);
        frontend_plan.preamble_hz = 20_600.0;
        frontend_plan.zero_hz = 20_100.0;
        frontend_plan.one_hz = 21_100.0;
        Ok(Self {
            frontend_plan,
            scene_plan,
            plan: DetectorPlan::new(sample_rate),
            threshold,
            buffer: Vec::new(),
            buffer_start_frame: 0,
            last_detections: HashMap::new(),
            pending_confirmation: HashMap::new(),
        })
    }

    pub fn push(&mut self, samples: &[f32]) -> Vec<Detection> {
        self.buffer.extend_from_slice(samples);
        if self.buffer.len() < self.plan.packet_len {
            return Vec::new();
        }
        let max_start = self.buffer.len() - self.plan.packet_len;
        let candidate_count = max_start / self.plan.step + 1;
        // Retain a full extra packet for cancellation and repetition evidence
        // across capture boundaries; this does not delay strong detections.
        let history_steps = self.plan.packet_len.div_ceil(self.plan.step);
        let consumed_frames = candidate_count.saturating_sub(history_steps) * self.plan.step;
        let mut detections = detect_ready_samples(&self.buffer, &self.plan, self.threshold);
        detections.extend(
            detect_ready_samples(&self.buffer, &self.scene_plan, self.threshold)
                .into_iter()
                .filter(|detection| {
                    matches!(
                        detection.marker,
                        TelemetryMarker::Area { .. } | TelemetryMarker::Frontend
                    )
                }),
        );
        detections.extend(
            detect_ready_samples(&self.buffer, &self.frontend_plan, self.threshold)
                .into_iter()
                .filter(|detection| detection.marker == TelemetryMarker::Frontend),
        );
        detections.sort_by_key(|detection| detection.start_frame);
        for detection in &mut detections {
            detection.start_frame += self.buffer_start_frame;
        }
        self.buffer.drain(..consumed_frames);
        self.buffer_start_frame += consumed_frames as u64;

        detections.retain_mut(|detection| {
            let stride = (self.plan.packet_len
                + seconds_frames(self.plan.sample_rate, PACKET_GAP_SECONDS))
                as u64;
            let paired = self
                .pending_confirmation
                .get(&detection.marker)
                .is_some_and(|first| {
                    let gap = detection.start_frame.abs_diff(*first);
                    (detection.start_frame > *first
                        && gap.abs_diff(stride) <= self.plan.step as u64 * 2)
                        || (detection.repeated && gap <= self.plan.step as u64 * 2)
                });
            if paired {
                detection.repeated = true;
                self.pending_confirmation.remove(&detection.marker);
            }
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
            if !duplicate && drop_signature_number(detection.marker).is_some() {
                if !detection.repeated && detection.confidence < 0.66 {
                    self.pending_confirmation
                        .insert(detection.marker, detection.start_frame);
                } else {
                    self.pending_confirmation.remove(&detection.marker);
                }
            }
            !duplicate || paired
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

    fn item(id: u32) -> TelemetryMarker {
        TelemetryMarker::Item { item_id: id }
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

    fn cyclic_correlation<const N: usize>(left: &[f32; N], right: &[f32; N], shift: usize) -> i32 {
        (0..N)
            .map(|index| (left[index] * right[(index + shift) % N]) as i32)
            .sum()
    }

    #[test]
    fn scene_audio_payloads_round_trip() {
        for marker in [
            TelemetryMarker::Area { area_id: 1 },
            TelemetryMarker::Area { area_id: 1023 },
            TelemetryMarker::Frontend,
        ] {
            assert_eq!(
                decode_payload(encode_payload(marker).unwrap()),
                Some(marker)
            );
        }
    }
    #[test]
    fn area_audio_survives_stream_boundaries() {
        for marker in [
            TelemetryMarker::Area { area_id: 6 },
            TelemetryMarker::Area { area_id: 104 },
        ] {
            let signal = tagged(marker, 48_000, -26.0);
            let mut detector = StreamingDetector::new(48_000, 0.50).unwrap();
            let detections: Vec<_> = signal
                .chunks(997)
                .flat_map(|chunk| detector.push(chunk))
                .collect();
            assert_eq!(detections.len(), 1);
            assert_eq!(detections[0].marker, marker);
        }
    }
    #[test]
    fn hostile_control_is_distinct_and_survives_stream_chunk_boundaries() {
        let marker = TelemetryMarker::Hostile;
        assert_ne!(
            encode_payload(marker),
            encode_payload(TelemetryMarker::Frontend)
        );
        for rate in [44_100, 48_000] {
            let samples = tagged(marker, rate, -18.0);
            let mut detector = StreamingDetector::new(rate, 0.56).unwrap();
            let detections: Vec<_> = samples
                .chunks(997)
                .flat_map(|chunk| detector.push(chunk))
                .collect();
            assert!(detections.iter().any(|d| d.marker == marker));
            assert!(detections.iter().all(|d| d.marker == marker));
        }
    }

    #[test]
    fn payload_round_trips_drops_and_hostile_only() {
        for marker in [
            rune(1),
            rune(33),
            item(1),
            item(MAX_ITEM_ID),
            TelemetryMarker::Hostile,
            TelemetryMarker::Hostile,
            TelemetryMarker::Hostile,
            TelemetryMarker::Hostile,
        ] {
            let payload = encode_payload(marker).unwrap();
            assert_eq!(decode_payload(payload), Some(marker));
            assert_eq!(decode_payload(payload ^ 1), None);
        }
    }

    #[test]
    fn item_signatures_round_trip_at_both_ends_of_the_reserved_range() {
        for marker in [item(1), item(50), item(MAX_ITEM_ID)] {
            let detections = detect_markers(&tagged(marker, 48_000, -26.0), 48_000, 0.50);
            assert!(detections
                .iter()
                .any(|detection| detection.marker == marker));
            assert!(detections
                .iter()
                .all(|detection| detection.marker == marker));
        }
    }

    #[test]
    fn simultaneous_runes_and_items_use_independent_gold_signatures() {
        let expected = [rune(7), rune(24), item(1), item(39), item(50)];
        let signals = expected.map(|marker| tagged(marker, 48_000, -24.0));
        let mut mixed = vec![0.0f32; signals[0].len()];
        for signal in signals {
            for (target, sample) in mixed.iter_mut().zip(signal) {
                *target += sample;
            }
        }
        let markers = detect_markers(&mixed, 48_000, 0.50)
            .into_iter()
            .map(|detection| detection.marker)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(markers, expected.into_iter().collect());
    }

    #[test]
    fn drop_detections_never_bypass_the_configured_final_threshold() {
        let threshold = 0.56;
        let signals = [
            tagged(rune(2), 48_000, -24.0),
            tagged(rune(5), 48_000, -24.0),
            tagged(rune(8), 48_000, -24.0),
            tagged(item(33), 48_000, -24.0),
        ];
        let mut mixed = vec![0.0f32; signals[0].len()];
        for signal in signals {
            for (target, sample) in mixed.iter_mut().zip(signal) {
                *target += sample;
            }
        }
        let detections = detect_markers(&mixed, 48_000, threshold);
        assert!(!detections.is_empty());
        assert!(
            detections
                .iter()
                .all(|detection| detection.confidence >= threshold),
            "{detections:?}"
        );
    }

    #[test]
    #[ignore = "requires D2RHUB_V7_FALSE_POSITIVE_WAV"]
    fn countess_overlap_recording_does_not_invent_item_47() {
        let path = std::env::var("D2RHUB_V7_FALSE_POSITIVE_WAV")
            .expect("set D2RHUB_V7_FALSE_POSITIVE_WAV to the diagnostic WAV path");
        let mut reader = hound::WavReader::open(path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.sample_format, hound::SampleFormat::Int);
        assert_eq!(spec.bits_per_sample, 16);
        let interleaved = reader
            .samples::<i16>()
            .map(|sample| sample.unwrap() as i32)
            .collect::<Vec<_>>();
        let mono = interleaved_i32_to_mono(&interleaved, spec.channels as usize, 16).unwrap();
        let detections = detect_markers(&mono, spec.sample_rate, 0.56);
        assert!(detections
            .iter()
            .any(|detection| detection.marker == item(33)));
        for expected in [rune(2), rune(5), rune(8)] {
            assert!(detections
                .iter()
                .any(|detection| detection.marker == expected));
        }
        assert!(
            detections
                .iter()
                .all(|detection| detection.marker != item(47)),
            "{detections:?}"
        );
    }

    #[test]
    fn v7_code_families_have_bounded_cyclic_correlation() {
        let location = control_preamble();
        let rune_sync = rune_preamble();
        let preamble_peak = (0..PREAMBLE_CHIPS)
            .map(|shift| cyclic_correlation(&location, &rune_sync, shift).abs())
            .max()
            .unwrap();
        assert!(preamble_peak <= 17, "preamble peak: {preamble_peak}");

        let signatures = (1..=33).map(rune_signature).collect::<Vec<_>>();
        for (left_index, left) in signatures.iter().enumerate() {
            for shift in 1..RUNE_SIGNATURE_CHIPS {
                let correlation = cyclic_correlation(left, left, shift).abs();
                assert!(
                    correlation <= 17,
                    "R{} autocorrelation shift {shift}: {correlation}",
                    left_index + 1
                );
            }
            for (right_index, right) in signatures.iter().enumerate().skip(left_index + 1) {
                for shift in 0..RUNE_SIGNATURE_CHIPS {
                    let correlation = cyclic_correlation(left, right, shift).abs();
                    assert!(
                        correlation <= 17,
                        "R{} vs R{} shift {shift}: {correlation}",
                        left_index + 1,
                        right_index + 1
                    );
                }
            }
        }
    }

    #[test]
    fn embedded_packets_round_trip_at_48khz() {
        for marker in [
            rune(1),
            rune(24),
            rune(33),
            TelemetryMarker::Hostile,
            TelemetryMarker::Hostile,
            TelemetryMarker::Hostile,
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
        for marker in [
            TelemetryMarker::Hostile,
            rune(24),
            TelemetryMarker::Frontend,
            TelemetryMarker::Area { area_id: 1 },
        ] {
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
    }

    #[test]
    fn simultaneous_frontend_and_town_are_both_decoded() {
        let town = tagged(TelemetryMarker::Area { area_id: 1 }, 48_000, -40.0);
        let menu = tagged(TelemetryMarker::Frontend, 48_000, -40.0);
        let mixed: Vec<_> = town.iter().zip(menu).map(|(a, b)| a + b).collect();
        let found = detect_markers(&mixed, 48_000, 0.56);
        assert!(found.iter().any(|d| d.marker == TelemetryMarker::Frontend));
        assert!(found
            .iter()
            .any(|d| d.marker == TelemetryMarker::Area { area_id: 1 }));
    }

    #[test]
    fn half_second_scene_heartbeats_survive_streaming_and_overlap() {
        for rate in [44_100, 48_000] {
            let town = TelemetryMarker::Area { area_id: 1 };
            let menu = TelemetryMarker::Frontend;
            let mut audio = vec![0.0; rate as usize * 4];
            for marker in [town, menu] {
                let packet = tagged(marker, rate, -40.0);
                for start in (0..rate as usize * 3).step_by(rate as usize / 2) {
                    for (index, sample) in packet.iter().enumerate() {
                        audio[start + index] += sample * 0.25;
                    }
                }
            }
            let mut decoder = StreamingDetector::new(rate, 0.56).unwrap();
            let found: Vec<_> = audio
                .chunks(4800)
                .flat_map(|chunk| decoder.push(chunk))
                .collect();
            for marker in [town, menu] {
                let beats: Vec<_> = found.iter().filter(|d| d.marker == marker).collect();
                assert_eq!(beats.len(), 6, "rate={rate}: {beats:?}");
                assert!(beats.windows(2).all(|pair| {
                    pair[1]
                        .start_frame
                        .abs_diff(pair[0].start_frame)
                        .abs_diff(rate as u64 / 2)
                        < 100
                }));
            }
        }
    }

    #[test]
    fn quiet_scene_watermarks_survive_both_rates() {
        for rate in [44_100, 48_000] {
            for marker in [
                TelemetryMarker::Frontend,
                TelemetryMarker::Area { area_id: 1 },
            ] {
                let signal = tagged(marker, rate, -40.0);
                let mixed: Vec<_> = signal
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        *value * 0.25
                            + (std::f32::consts::TAU * 440.0 * index as f32 / rate as f32).sin()
                                * 0.1
                    })
                    .collect();
                assert!(detect_markers(&mixed, rate, 0.56)
                    .iter()
                    .any(|detection| detection.marker == marker));
            }
        }
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
                    + (std::f32::consts::TAU * 19_600.0 * time).sin() * 0.10
            })
            .collect::<Vec<_>>();
        assert!(detect_markers(&samples, 48_000, 0.45).is_empty());
    }

    #[test]
    fn control_sync_cannot_unlock_a_valid_rune_signature() {
        let mut hybrid = tagged(TelemetryMarker::Hostile, 48_000, -26.0);
        let rune_signal = tagged(rune(27), 48_000, -26.0);
        let signature_len = chip_frames(48_000) * RUNE_SIGNATURE_CHIPS;
        let packet_len = packet_frames(48_000);
        let packet_gap = seconds_frames(48_000, PACKET_GAP_SECONDS);
        for repetition in 0..PACKET_REPETITIONS {
            let packet_start =
                marker_offset_frames(48_000) + repetition * (packet_len + packet_gap);
            let signature_start = packet_start + payload_offset_frames(48_000);
            for index in signature_start..signature_start + signature_len {
                hybrid[index] += rune_signal[index];
            }
        }
        let detections = detect_markers(&hybrid, 48_000, 0.50);
        assert!(detections
            .iter()
            .any(|detection| detection.marker == TelemetryMarker::Hostile));
        assert!(
            detections
                .iter()
                .all(|detection| !matches!(detection.marker, TelemetryMarker::Rune { .. })),
            "{detections:?}"
        );
    }

    #[test]
    fn one_chip_rune_echo_does_not_alias_adjacent_runes() {
        let marker = rune(20);
        let signal = tagged(marker, 48_000, -26.0);
        let echo_delay = chip_frames(48_000);
        let mut mixed = vec![0.0f32; signal.len() + echo_delay];
        for (index, sample) in signal.iter().enumerate() {
            mixed[index] += *sample;
            mixed[index + echo_delay] += *sample * 0.72;
        }
        let detections = detect_markers(&mixed, 48_000, 0.50);
        assert!(detections
            .iter()
            .any(|detection| detection.marker == marker));
        assert!(
            detections
                .iter()
                .all(|detection| detection.marker == marker),
            "{detections:?}"
        );
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
    fn simultaneous_different_runes_decode_without_time_slots() {
        let signals = [
            tagged(rune(1), 48_000, -26.0),
            tagged(rune(4), 48_000, -26.0),
            tagged(rune(7), 48_000, -26.0),
            tagged(rune(12), 48_000, -26.0),
            tagged(rune(16), 48_000, -26.0),
            tagged(rune(20), 48_000, -26.0),
            tagged(rune(24), 48_000, -26.0),
            tagged(rune(33), 48_000, -26.0),
        ];
        let output_len = signals.iter().map(Vec::len).max().unwrap();
        let mut mixed = vec![0.0f32; output_len];
        for signal in signals {
            for (target, sample) in mixed.iter_mut().zip(signal) {
                *target += sample;
            }
        }
        let mut markers = detect_markers(&mixed, 48_000, 0.50)
            .into_iter()
            .map(|detection| detection.marker)
            .collect::<Vec<_>>();
        markers.sort_by_key(|marker| match marker {
            TelemetryMarker::Rune { rune_number } => *rune_number,
            TelemetryMarker::Item { item_id } => RUNE_COUNT + *item_id,
            TelemetryMarker::Area { .. } | TelemetryMarker::Frontend | TelemetryMarker::Hostile => {
                u32::MAX
            }
        });
        assert_eq!(
            markers,
            vec![
                rune(1),
                rune(4),
                rune(7),
                rune(12),
                rune(16),
                rune(20),
                rune(24),
                rune(33)
            ]
        );
    }

    #[test]
    fn simultaneous_hostile_and_runes_remain_independent() {
        let signals = [
            tagged(TelemetryMarker::Hostile, 48_000, -26.0),
            tagged(rune(7), 48_000, -26.0),
            tagged(rune(20), 48_000, -26.0),
        ];
        let mut mixed = vec![0.0f32; signals[0].len()];
        for signal in signals {
            for (target, sample) in mixed.iter_mut().zip(signal) {
                *target += sample;
            }
        }
        let markers = detect_markers(&mixed, 48_000, 0.50)
            .into_iter()
            .map(|detection| detection.marker)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(markers.len(), 3, "{markers:?}");
        assert!(markers.contains(&TelemetryMarker::Hostile));
        assert!(markers.contains(&rune(7)));
        assert!(markers.contains(&rune(20)));
    }

    #[test]
    #[ignore = "requires D2RHUB_REAL_AUDIO_WAVS with semicolon-separated diagnostic WAV paths"]
    fn real_v5_recordings_cannot_create_v7_phantom_runes() {
        let paths = std::env::var("D2RHUB_REAL_AUDIO_WAVS")
            .expect("set D2RHUB_REAL_AUDIO_WAVS before running this diagnostic regression");
        for path in paths.split(';').filter(|path| !path.is_empty()) {
            let mut reader = hound::WavReader::open(path).unwrap();
            let spec = reader.spec();
            assert_eq!(spec.sample_format, hound::SampleFormat::Int);
            assert_eq!(spec.bits_per_sample, 16);
            let interleaved = reader
                .samples::<i16>()
                .map(|sample| sample.unwrap() as i32)
                .collect::<Vec<_>>();
            let mut mono =
                interleaved_i32_to_mono(&interleaved, spec.channels as usize, 16).unwrap();

            let baseline = detect_markers(&mono, spec.sample_rate, 0.56);
            assert!(
                baseline
                    .iter()
                    .all(|detection| !matches!(detection.marker, TelemetryMarker::Rune { .. })),
                "phantom v7 rune in {path}: {baseline:?}"
            );

            let signal = tagged(rune(20), spec.sample_rate, -26.0);
            let insert_at = spec.sample_rate as usize;
            for (target, sample) in mono[insert_at..].iter_mut().zip(signal) {
                *target += sample;
            }
            let detections = detect_markers(&mono, spec.sample_rate, 0.56);
            let runes = detections
                .iter()
                .filter_map(|detection| match detection.marker {
                    TelemetryMarker::Rune { rune_number } => Some(rune_number),
                    TelemetryMarker::Item { .. }
                    | TelemetryMarker::Area { .. }
                    | TelemetryMarker::Frontend
                    | TelemetryMarker::Hostile => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(runes, vec![20], "real-audio overlay failed in {path}");
        }
    }

    #[test]
    fn streaming_detector_bounds_memory_and_does_not_repeat_second_copy() {
        let marker = TelemetryMarker::Hostile;
        let signal = tagged(marker, 48_000, -26.0);
        let mut detector = StreamingDetector::new(48_000, 0.50).unwrap();
        let mut detections = Vec::new();
        for chunk in signal.chunks(997) {
            detections.extend(detector.push(chunk));
            assert!(detector.buffered_frames() < 2 * packet_frames(48_000) + 997);
        }
        assert_eq!(detections.len(), 1, "{detections:?}");
        assert_eq!(detections[0].marker, marker);
    }
    #[test]
    fn all_runes_survive_a_twenty_db_stronger_drop() {
        for rate in [44_100, 48_000] {
            for number in 1..=33 {
                let weak = tagged(rune(number), rate, -40.0);
                let strong_marker = rune(number % 33 + 1);
                let strong = tagged(strong_marker, rate, -20.0);
                let mixed: Vec<_> = weak.iter().zip(strong).map(|(a, b)| a + b).collect();
                let found: std::collections::HashSet<_> = detect_markers(&mixed, rate, 0.56)
                    .into_iter()
                    .map(|d| d.marker)
                    .collect();
                assert_eq!(
                    found,
                    [rune(number), strong_marker].into_iter().collect(),
                    "rate={rate}, rune={number}"
                );
            }
        }
    }

    #[test]
    fn asynchronous_25_28_32_with_noise_and_echo_have_no_extra_identities() {
        let rate = 48_000;
        for delay in [0, 9, 36, 177, 960, 2400] {
            let mut mixed = vec![0.0; 24000];
            for (number, offset, gain) in
                [(25, 0, -32.0), (28, delay, -24.0), (32, delay * 2, -28.0)]
            {
                for (k, value) in tagged(rune(number), rate, gain).into_iter().enumerate() {
                    mixed[k + offset] += value;
                    mixed[k + offset + 72] += value * 0.25;
                }
            }
            let mut state = 0x12345678u32;
            for (k, value) in mixed.iter_mut().enumerate() {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                *value += (state as f64 / u32::MAX as f64 - 0.5) as f32 * 0.001
                    + (std::f32::consts::TAU * 4700.0 * k as f32 / rate as f32).sin() * 0.05;
            }
            let mut detector = StreamingDetector::new(rate, 0.56).unwrap();
            let detections: Vec<_> = mixed
                .chunks(997)
                .flat_map(|chunk| detector.push(chunk))
                .collect();
            for number in [25, 28, 32] {
                let events: Vec<_> = detections
                    .iter()
                    .filter(|d| d.marker == rune(number))
                    .collect();
                assert!((1..=2).contains(&events.len()), "delay={delay}: {events:?}");
                if events.len() == 2 {
                    assert!(events[1].repeated);
                }
                assert!(
                    events.iter().any(|d| d.confidence >= 0.66 || d.repeated),
                    "no actionable evidence: delay={delay}: {events:?}"
                );
            }
            let found: std::collections::HashSet<_> =
                detections.into_iter().map(|d| d.marker).collect();
            assert_eq!(
                found,
                [rune(25), rune(28), rune(32)].into_iter().collect(),
                "delay={delay}"
            );
        }
    }

    #[test]
    fn strong_sync_cannot_certify_noise_or_a_partial_identity() {
        let rate = 48_000;
        for number in [25, 28, 32] {
            for keep_half in [false, true] {
                let mut signal = tagged(rune(number), rate, -26.0);
                let plan = DetectorPlan::new(rate);
                let mut random = 0x98765432u32;
                for packet in 0..PACKET_REPETITIONS {
                    let start = marker_offset_frames(rate)
                        + packet * (plan.packet_len + seconds_frames(rate, PACKET_GAP_SECONDS))
                        + plan.payload_offset;
                    for k in 0..RUNE_SIGNATURE_CHIPS * plan.chip_len {
                        if !keep_half || k >= (RUNE_SIGNATURE_CHIPS / 2) * plan.chip_len {
                            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                            signal[start + k] =
                                (random as f64 / u32::MAX as f64 - 0.5) as f32 * 0.05;
                        }
                    }
                }
                assert!(
                    detect_markers(&signal, rate, 0.56).is_empty(),
                    "rune={number}, half={keep_half}"
                );
            }
        }
    }

    #[test]
    fn unstructured_signature_noise_does_not_fit_phantom_drops() {
        let plan = DetectorPlan::new(48000);
        let mut random = 0x12345678u32;
        for trial in 0..1000 {
            let mut i = [0.0; RUNE_SIGNATURE_CHIPS];
            let mut q = i;
            for value in i.iter_mut().chain(q.iter_mut()) {
                random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                *value = (random as f64 / u32::MAX as f64 - 0.5) as f32;
            }
            assert!(
                separate_drop_signatures(&i, &q, &plan, 0.56).is_empty(),
                "trial={trial}"
            );
        }
    }
}
