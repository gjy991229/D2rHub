use super::catalog::{
    area_definition, marker_signal_number, TelemetryMarker, RUNE_COUNT, TRACKED_AREA_IDS,
};
use super::protocol::{
    detect_markers, embed_marker, interleaved_i32_to_mono, MarkerConfig, PROTOCOL_VERSION,
};
use claxon::FlacReader;
use flacenc::component::BitRepr;
use flacenc::error::Verify;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const OUTPUT_DIRECTORY_NAME: &str = "D2RHubTagged";
const MANIFEST_FILE_NAME: &str = "d2rhub-audio-manifest.json";
/// 地面物品的 AudioEmitter 没有一次性模式；符文尾部保留一段静音，
/// 供 Sounds.txt 从该位置持续循环，避免同一个地面对象再次播报。
pub const RUNE_LOOP_TAIL_SECONDS: f32 = 0.25;

fn next_default_output_directory(input_directory: &Path) -> PathBuf {
    let first = input_directory.join(OUTPUT_DIRECTORY_NAME);
    if !first.exists() {
        return first;
    }
    for suffix in 2u32.. {
        let candidate = input_directory.join(format!("{OUTPUT_DIRECTORY_NAME}-{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u32 output-directory suffix space exhausted")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRequest {
    pub input_directory: String,
    pub output_directory: Option<String>,
    pub gain_db: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedFlac {
    pub marker: TelemetryMarker,
    pub marker_label: String,
    pub source_path: String,
    pub output_path: String,
    pub sample_rate: u32,
    pub channels: u32,
    pub bits_per_sample: u32,
    pub confidence: f32,
    pub frames_added: usize,
    pub output_gain: f32,
    pub loop_start_frame: Option<usize>,
    pub loop_tail_frames: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchReport {
    pub protocol_version: u8,
    pub marker_config: MarkerConfig,
    pub input_directory: String,
    pub output_directory: String,
    pub processed: Vec<ProcessedFlac>,
    pub missing_runes: Vec<u32>,
    pub missing_areas: Vec<u32>,
    pub missing_frontend: bool,
    pub skipped_files: Vec<String>,
}

fn marker_from_path(path: &Path) -> Option<TelemetryMarker> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("flac"))
    {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    if stem.eq_ignore_ascii_case("frontend") {
        return Some(TelemetryMarker::Frontend);
    }
    let (kind, number) =
        if let Some(number) = stem.strip_prefix('r').or_else(|| stem.strip_prefix('R')) {
            ('r', number)
        } else if let Some(number) = stem.strip_prefix('a').or_else(|| stem.strip_prefix('A')) {
            ('a', number)
        } else {
            return None;
        };
    if number.is_empty() || number.len() > 3 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if kind == 'r' && number.starts_with('0') && number.len() != 2 {
        return None;
    }
    let value = number.parse::<u32>().ok()?;
    if kind == 'r' && number.starts_with('0') && value > 9 {
        return None;
    }
    match kind {
        'r' if (1..=RUNE_COUNT).contains(&value) => {
            Some(TelemetryMarker::Rune { rune_number: value })
        }
        'a' if area_definition(value).is_some() && !number.starts_with('0') => {
            Some(TelemetryMarker::Area { area_id: value })
        }
        _ => None,
    }
}

fn marker_label(marker: TelemetryMarker) -> String {
    match marker {
        TelemetryMarker::Rune { rune_number } => format!("符文 #{rune_number}"),
        TelemetryMarker::Area { area_id } => area_definition(area_id)
            .map(|area| format!("场景 {}（Area Id {area_id}）", area.scene_name))
            .unwrap_or_else(|| format!("场景 Area Id {area_id}")),
        TelemetryMarker::Frontend => "主界面".to_string(),
    }
}

fn decode_flac(path: &Path) -> Result<(Vec<i32>, u32, u32, u32), String> {
    let mut reader = FlacReader::open(path)
        .map_err(|error| format!("读取 FLAC 失败 {}: {error}", path.display()))?;
    let stream_info = reader.streaminfo();
    let sample_rate = stream_info.sample_rate;
    let channels = stream_info.channels;
    let bits_per_sample = stream_info.bits_per_sample;
    let samples = reader
        .samples()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解码 FLAC 失败 {}: {error}", path.display()))?;
    Ok((samples, sample_rate, channels, bits_per_sample))
}

fn encode_flac(
    path: &Path,
    samples: &[i32],
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
) -> Result<(), String> {
    // flacenc 0.5.x can select pathological Rice parameters for clipped 24-bit
    // game assets in its fixed/LPC paths, inflating a ~100 KiB source to hundreds
    // of MiB. Constant/verbatim subframes remain lossless, deterministic and
    // bounded by the PCM size, which is the safer choice for user-supplied assets.
    let mut encoder = flacenc::config::Encoder::default();
    encoder.subframe_coding.use_fixed = false;
    encoder.subframe_coding.use_lpc = false;
    let config = encoder
        .into_verified()
        .map_err(|error| format!("FLAC 编码配置无效: {error:?}"))?;
    let source = flacenc::source::MemSource::from_samples(
        samples,
        channels as usize,
        bits_per_sample as usize,
        sample_rate as usize,
    );
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|error| format!("FLAC 编码失败 {}: {error:?}", path.display()))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|error| format!("FLAC 序列化失败 {}: {error:?}", path.display()))?;
    std::fs::write(path, sink.as_slice())
        .map_err(|error| format!("写入 FLAC 失败 {}: {error}", path.display()))
}

fn process_one(
    source_path: &Path,
    output_path: &Path,
    marker: TelemetryMarker,
    config: MarkerConfig,
) -> Result<ProcessedFlac, String> {
    if output_path.exists() {
        return Err(format!("输出文件已存在，未覆盖: {}", output_path.display()));
    }
    let (mut samples, sample_rate, channels, bits_per_sample) = decode_flac(source_path)?;
    let embedding = embed_marker(
        &mut samples,
        channels as usize,
        bits_per_sample,
        sample_rate,
        marker,
        config,
    )?;
    let (loop_start_frame, loop_tail_frames) = match marker {
        TelemetryMarker::Rune { .. } => {
            let loop_start = samples.len() / channels as usize;
            let tail_frames = (sample_rate as f32 * RUNE_LOOP_TAIL_SECONDS).round() as usize;
            samples.resize(samples.len() + tail_frames * channels as usize, 0);
            (Some(loop_start), tail_frames)
        }
        TelemetryMarker::Area { .. } | TelemetryMarker::Frontend => (None, 0),
    };

    let temp_path = output_path.with_extension("flac.d2rhub.tmp");
    if temp_path.exists() {
        std::fs::remove_file(&temp_path)
            .map_err(|error| format!("清理临时文件失败 {}: {error}", temp_path.display()))?;
    }
    encode_flac(&temp_path, &samples, sample_rate, channels, bits_per_sample)?;

    let verification = (|| {
        let (verified_samples, verified_rate, verified_channels, verified_bits) =
            decode_flac(&temp_path)?;
        let mono =
            interleaved_i32_to_mono(&verified_samples, verified_channels as usize, verified_bits)?;
        let detections = detect_markers(&mono, verified_rate, config.detection_threshold)
            .into_iter()
            .filter(|detection| detection.marker == marker)
            .collect::<Vec<_>>();
        if detections.len() != 1 {
            return Err(format!(
                "输出自检要求{}恰好出现一次，实际识别到 {} 次",
                marker_label(marker),
                detections.len()
            ));
        }
        detections
            .into_iter()
            .max_by(|left, right| {
                left.confidence
                    .partial_cmp(&right.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .ok_or_else(|| format!("输出自检未识别到{}", marker_label(marker)))
    })();

    let detection = match verification {
        Ok(detection) => detection,
        Err(error) => {
            let _ = std::fs::remove_file(&temp_path);
            return Err(error);
        }
    };
    std::fs::rename(&temp_path, output_path).map_err(|error| {
        let _ = std::fs::remove_file(&temp_path);
        format!("提交输出文件失败 {}: {error}", output_path.display())
    })?;

    Ok(ProcessedFlac {
        marker,
        marker_label: marker_label(marker),
        source_path: source_path.to_string_lossy().to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        sample_rate,
        channels,
        bits_per_sample,
        confidence: detection.confidence,
        frames_added: embedding.frames_added,
        output_gain: embedding.output_gain,
        loop_start_frame,
        loop_tail_frames,
    })
}

pub fn process_directory(request: BatchRequest) -> Result<BatchReport, String> {
    let input_directory = PathBuf::from(request.input_directory.trim());
    if !input_directory.is_dir() {
        return Err(format!("输入目录不存在: {}", input_directory.display()));
    }
    let output_directory = request
        .output_directory
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| next_default_output_directory(&input_directory));
    if output_directory == input_directory {
        return Err("输出目录不能与输入目录相同，避免覆盖 mod 原始资源".to_string());
    }
    std::fs::create_dir_all(&output_directory)
        .map_err(|error| format!("创建输出目录失败 {}: {error}", output_directory.display()))?;

    let config = MarkerConfig {
        gain_db: request.gain_db.unwrap_or(MarkerConfig::default().gain_db),
        ..MarkerConfig::default()
    }
    .validate()?;
    let mut candidates = Vec::new();
    let mut skipped_files = Vec::new();
    for entry in
        std::fs::read_dir(&input_directory).map_err(|error| format!("读取输入目录失败: {error}"))?
    {
        let path = entry
            .map_err(|error| format!("读取目录项失败: {error}"))?
            .path();
        if path.is_file() {
            if let Some(marker) = marker_from_path(&path) {
                candidates.push((marker, path));
            } else if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("flac"))
            {
                skipped_files.push(path.to_string_lossy().to_string());
            }
        }
    }
    candidates.sort_by_key(|(marker, _)| marker_signal_number(*marker).unwrap_or(u32::MAX));
    if candidates.is_empty() {
        return Err(
            "目录中没有找到 r1.flac-r33.flac、受支持的 a{AreaId}.flac 或 frontend.flac".to_string(),
        );
    }
    for pair in candidates.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(format!(
                "目录中存在重复的声纹身份：{}",
                marker_label(pair[0].0)
            ));
        }
    }

    let present_runes: std::collections::HashSet<u32> = candidates
        .iter()
        .filter_map(|(marker, _)| match marker {
            TelemetryMarker::Rune { rune_number } => Some(*rune_number),
            TelemetryMarker::Area { .. } | TelemetryMarker::Frontend => None,
        })
        .collect();
    let missing_runes = (1..=RUNE_COUNT)
        .filter(|number| !present_runes.contains(number))
        .collect();
    let present_areas: std::collections::HashSet<u32> = candidates
        .iter()
        .filter_map(|(marker, _)| match marker {
            TelemetryMarker::Area { area_id } => Some(*area_id),
            TelemetryMarker::Rune { .. } | TelemetryMarker::Frontend => None,
        })
        .collect();
    let missing_areas = TRACKED_AREA_IDS
        .iter()
        .copied()
        .filter(|area_id| !present_areas.contains(area_id))
        .collect();
    let missing_frontend = !candidates
        .iter()
        .any(|(marker, _)| *marker == TelemetryMarker::Frontend);
    let mut processed = Vec::new();
    for (marker, source_path) in candidates {
        let output_path = output_directory.join(
            source_path
                .file_name()
                .ok_or_else(|| format!("源文件名无效: {}", source_path.display()))?,
        );
        processed.push(process_one(&source_path, &output_path, marker, config)?);
    }

    let report = BatchReport {
        protocol_version: PROTOCOL_VERSION,
        marker_config: config,
        input_directory: input_directory.to_string_lossy().to_string(),
        output_directory: output_directory.to_string_lossy().to_string(),
        processed,
        missing_runes,
        missing_areas,
        missing_frontend,
        skipped_files,
    };
    let manifest_path = output_directory.join(MANIFEST_FILE_NAME);
    let manifest =
        serde_json::to_vec_pretty(&report).map_err(|error| format!("生成处理清单失败: {error}"))?;
    std::fs::write(&manifest_path, manifest)
        .map_err(|error| format!("写入处理清单失败 {}: {error}", manifest_path.display()))?;
    Ok(report)
}

#[tauri::command]
pub async fn process_rune_flac_directory(request: BatchRequest) -> Result<BatchReport, String> {
    tauri::async_runtime::spawn_blocking(move || process_directory(request))
        .await
        .map_err(|error| format!("等待 FLAC 处理任务失败: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::{
        decode_flac, encode_flac, marker_from_path, next_default_output_directory,
        process_directory, BatchRequest, RUNE_LOOP_TAIL_SECONDS,
    };
    use crate::rune_audio::catalog::TelemetryMarker;
    use std::path::Path;

    #[test]
    fn recognizes_rune_and_catalog_area_file_names() {
        assert_eq!(
            marker_from_path(Path::new("r1.flac")),
            Some(TelemetryMarker::Rune { rune_number: 1 })
        );
        assert_eq!(
            marker_from_path(Path::new("R33.FLAC")),
            Some(TelemetryMarker::Rune { rune_number: 33 })
        );
        assert_eq!(
            marker_from_path(Path::new("r01.flac")),
            Some(TelemetryMarker::Rune { rune_number: 1 })
        );
        assert_eq!(
            marker_from_path(Path::new("R09.FLAC")),
            Some(TelemetryMarker::Rune { rune_number: 9 })
        );
        assert_eq!(
            marker_from_path(Path::new("a1.flac")),
            Some(TelemetryMarker::Area { area_id: 1 })
        );
        assert_eq!(
            marker_from_path(Path::new("A25.FLAC")),
            Some(TelemetryMarker::Area { area_id: 25 })
        );
        assert_eq!(marker_from_path(Path::new("a20.flac")), None);
        assert_eq!(marker_from_path(Path::new("a01.flac")), None);
        assert_eq!(marker_from_path(Path::new("r010.flac")), None);
        assert_eq!(marker_from_path(Path::new("r34.flac")), None);
        assert_eq!(marker_from_path(Path::new("rune1.flac")), None);
        assert_eq!(
            marker_from_path(Path::new("FrontEnd.FLAC")),
            Some(TelemetryMarker::Frontend)
        );
        assert_eq!(marker_from_path(Path::new("r1.wav")), None);
    }

    #[test]
    fn default_output_directory_never_reuses_an_existing_batch() {
        let root =
            std::env::temp_dir().join(format!("d2rhub-output-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("D2RHubTagged")).unwrap();
        std::fs::create_dir_all(root.join("D2RHubTagged-2")).unwrap();
        assert_eq!(
            next_default_output_directory(&root),
            root.join("D2RHubTagged-3")
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn batch_processor_round_trips_real_32khz_flac_files_and_writes_manifest() {
        let root = std::env::temp_dir().join(format!("d2rhub-flac-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let sample_rate = 32_000u32;
        let samples: Vec<i32> = (0..sample_rate / 4)
            .flat_map(|frame| {
                let value = ((std::f32::consts::TAU * 440.0 * frame as f32 / sample_rate as f32)
                    .sin()
                    * 4_000.0) as i32;
                [value, value]
            })
            .collect();
        encode_flac(&root.join("r1.flac"), &samples, sample_rate, 2, 16).unwrap();
        encode_flac(&root.join("R33.FLAC"), &samples, sample_rate, 2, 16).unwrap();
        encode_flac(&root.join("a21.flac"), &samples, sample_rate, 2, 16).unwrap();
        encode_flac(&root.join("frontend.flac"), &samples, sample_rate, 2, 16).unwrap();
        std::fs::write(root.join("notes.flac"), b"not a candidate").unwrap();

        let report = process_directory(BatchRequest {
            input_directory: root.to_string_lossy().to_string(),
            output_directory: None,
            gain_db: Some(-26.0),
        })
        .unwrap();

        assert_eq!(report.processed.len(), 4);
        assert_eq!(report.missing_runes.len(), 31);
        assert_eq!(report.missing_areas.len(), 6);
        assert!(!report.missing_frontend);
        assert_eq!(
            report.processed[0].marker,
            TelemetryMarker::Rune { rune_number: 1 }
        );
        assert_eq!(
            report.processed[1].marker,
            TelemetryMarker::Rune { rune_number: 33 }
        );
        assert_eq!(
            report.processed[2].marker,
            TelemetryMarker::Area { area_id: 21 }
        );
        assert_eq!(report.processed[3].marker, TelemetryMarker::Frontend);
        assert!(report.processed.iter().all(|item| item.confidence > 0.8));
        let expected_tail_frames = (sample_rate as f32 * RUNE_LOOP_TAIL_SECONDS).round() as usize;
        for item in &report.processed[..2] {
            assert_eq!(item.loop_tail_frames, expected_tail_frames);
            assert!(item.loop_start_frame.is_some());
            let (tagged, _, tagged_channels, _) =
                decode_flac(Path::new(&item.output_path)).unwrap();
            let loop_start = item.loop_start_frame.unwrap() * tagged_channels as usize;
            assert_eq!(
                tagged.len() - loop_start,
                expected_tail_frames * tagged_channels as usize
            );
            assert!(tagged[loop_start..].iter().all(|sample| *sample == 0));
        }
        assert!(report.processed[2..]
            .iter()
            .all(|item| item.loop_start_frame.is_none() && item.loop_tail_frames == 0));
        assert!(root
            .join("D2RHubTagged")
            .join("d2rhub-audio-manifest.json")
            .is_file());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn clipped_24_bit_audio_has_a_bounded_output_size() {
        let root =
            std::env::temp_dir().join(format!("d2rhub-flac-bound-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("clipped.flac");
        let frames = 32_000usize;
        let samples = (0..frames * 2)
            .map(|index| {
                if index % 3 == 0 {
                    -8_388_608
                } else {
                    8_388_607
                }
            })
            .collect::<Vec<_>>();

        encode_flac(&path, &samples, 32_000, 2, 24).unwrap();

        let encoded_size = std::fs::metadata(&path).unwrap().len() as usize;
        let raw_pcm_size = samples.len() * 3;
        assert!(encoded_size <= raw_pcm_size + 64 * 1024);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
