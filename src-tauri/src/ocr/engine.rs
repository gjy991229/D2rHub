use std::sync::{Mutex, MutexGuard, OnceLock};

// NOTE: Since you are using paddle-ocr-rs, we import the necessary structs.
// Please check the exact API from the paddle-ocr-rs docs, as it might differ slightly.
// 假设这里使用了 kreuzberg_paddle_ocr 这个包
use kreuzberg_paddle_ocr::ocr_lite::OcrLite;

// 用于从 ONNX 模型元数据提取正确字典
use ort::session::Session;

// `OcrResult` is the unified struct we use in pipeline.rs
pub struct OcrResult {
    pub text: String,
}

// Wrapping OcrLite in a Mutex because inference might require mutability or exclusive access
static ENGINE: OnceLock<Mutex<Option<OcrLite>>> = OnceLock::new();

fn lock_recover<'a, T>(mutex: &'a Mutex<T>, label: &str) -> MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            crate::logger::log_msg(
                "WARN",
                "OCR",
                &format!("{label} mutex was poisoned; recovering its protected state"),
            );
            mutex.clear_poison();
            poisoned.into_inner()
        }
    }
}

/// 初始化 OCR 引擎。
/// @param app_data_dir 主数据目录（Windows 默认为 %APPDATA%/D2RHub），引擎优先从此加载
/// @param resource_dir Tauri 资源目录（NSIS 安装时位于 &lt;exe&gt; 同级），
///                    若 app_data_dir 下无模型则回退到 resource_dir/_up_/assets/models
pub fn init_engine(
    app_data_dir: &str,
    resource_dir: Option<&std::path::Path>,
) -> Result<(), String> {
    let engine_mutex = ENGINE.get_or_init(|| Mutex::new(None));
    if lock_recover(engine_mutex, "OCR engine").is_some() {
        return Ok(());
    }

    // 主路径：app_data_dir/assets/models/
    let mut model_dir = std::path::PathBuf::from(app_data_dir)
        .join("assets")
        .join("models");

    // 若主路径不存在，尝试 NSIS _up_ 回退路径
    if !model_dir.exists() {
        if let Some(res_dir) = resource_dir {
            let fallback = res_dir.join("_up_").join("assets").join("models");
            if fallback.exists() {
                crate::logger::log_msg(
                    "INFO",
                    "OCR",
                    &format!("主路径模型不存在，使用 _up_ 回退: {}", fallback.display()),
                );
                model_dir = fallback;
            }
        }
    }
    let det_model = model_dir.join("ch_PP-OCRv5_det_mobile.onnx");
    let cls_model = model_dir.join("ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx");
    let rec_model = model_dir.join("ch_PP-OCRv5_rec_mobile.onnx");

    let mut ocr = OcrLite::new();

    // 当前打包的 ONNX Runtime 使用 CPU provider。
    // PP-OCRv5 模型用 CTC 训练，输出 index 0 是 blank token "#"。
    // 但 ONNX 元数据里只有字符没有 "#"，score_to_text_line 显式跳过 index 0，
    // 所以 keys[0] 必须是 "#"，否则索引 1..N 全部错位 1 → 乱码。
    //
    // 策略：优先用预先打包好的 ppocr_keys_v1_fixed.txt（已补 "#"），
    // 不存在则从 ONNX 模型元数据临时提取（首次运行或模型更新后一次性生成）。
    let fixed_path = {
        let path = model_dir.join("ppocr_keys_v1_fixed.txt");
        if path.exists() {
            path
        } else {
            // 从模型元数据提取原字典，补 "#" 后持久化
            let chars = {
                let mut builder =
                    Session::builder().map_err(|e| format!("ort Session::builder 失败: {}", e))?;
                let session = builder
                    .commit_from_file(&rec_model)
                    .map_err(|e| format!("ort commit_from_file 失败: {}", e))?;
                let metadata = session
                    .metadata()
                    .map_err(|e| format!("ort metadata 失败: {}", e))?;
                metadata
                    .custom("character")
                    .ok_or_else(|| "模型元数据缺少 'character' 字段".to_string())?
            };
            let fixed_content = format!("#\n{}", chars.trim_end());
            std::fs::write(&path, &fixed_content)
                .map_err(|e| format!("无法写入修复字典: {}", e))?;
            path
        }
    };

    let det_model = det_model
        .to_str()
        .ok_or_else(|| "OCR 检测模型路径不是有效 Unicode".to_string())?;
    let cls_model = cls_model
        .to_str()
        .ok_or_else(|| "OCR 分类模型路径不是有效 Unicode".to_string())?;
    let rec_model = rec_model
        .to_str()
        .ok_or_else(|| "OCR 识别模型路径不是有效 Unicode".to_string())?;
    let fixed_path = fixed_path
        .to_str()
        .ok_or_else(|| "OCR 字典路径不是有效 Unicode".to_string())?;
    ocr.init_models_with_dict(
        det_model, cls_model, rec_model, fixed_path, 2, // thread_num
    )
    .map_err(|e| format!("Failed to init paddle-ocr models: {:?}", e))?;

    *lock_recover(engine_mutex, "OCR engine") = Some(ocr);

    crate::logger::log_msg(
        "INFO",
        "OCR",
        "paddle-ocr-rs CPU engine initialized successfully",
    );
    Ok(())
}

pub fn release_engine() {
    if let Some(engine_mutex) = ENGINE.get() {
        let mut engine = lock_recover(engine_mutex, "OCR engine");
        if engine.take().is_some() {
            crate::logger::log_msg("INFO", "OCR", "OCR engine released");
        }
    }
}

/// Returns a list of recognized text blocks
pub fn recognize_rgba(rgba: &[u8], w: u32, h: u32) -> Result<Vec<OcrResult>, String> {
    if w == 0 || h == 0 || rgba.is_empty() {
        return Ok(Vec::new());
    }

    if rgba.len() != (w * h * 4) as usize {
        return Err(format!(
            "Invalid buffer size: expected {}, got {}",
            w * h * 4,
            rgba.len()
        ));
    }

    let engine_mutex = ENGINE.get().ok_or("OCR Engine not initialized")?;
    let mut engine = lock_recover(engine_mutex, "OCR engine");
    let ocr = engine
        .as_mut()
        .ok_or_else(|| "OCR Engine not initialized".to_string())?;

    // Create an RgbImage directly from the RGBA slice in a single pass, avoiding extra copies
    let mut rgb_data = Vec::with_capacity((w * h * 3) as usize);
    for chunk in rgba.chunks_exact(4) {
        rgb_data.push(chunk[0]); // R
        rgb_data.push(chunk[1]); // G
        rgb_data.push(chunk[2]); // B
    }
    let rgb_img = image::RgbImage::from_raw(w, h, rgb_data)
        .ok_or("Failed to create RgbImage from raw data")?;

    // 假设 paddle_ocr_rs 有针对 DynamicImage 的 detect 方法，
    // 或者可能需要先转成 RGB 等格式。具体请参考其包文档。
    // 这里做个示例：
    let detect_res = ocr
        .detect(&rgb_img, 50, 1024, 0.5, 0.3, 1.6, false, false)
        .map_err(|e| format!("paddle-ocr-rs inference failed: {:?}", e))?;

    let mut final_results = Vec::new();
    for block in detect_res.text_blocks {
        final_results.push(OcrResult { text: block.text });
    }

    Ok(final_results)
}

#[cfg(test)]
mod tests {
    use super::lock_recover;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::Mutex;

    #[test]
    fn poisoned_mutex_is_recovered_and_cleared() {
        let mutex = Mutex::new(7_u8);
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = mutex.lock().expect("test mutex should initially lock");
            panic!("poison test mutex");
        }));

        assert!(mutex.is_poisoned());
        assert_eq!(*lock_recover(&mutex, "test"), 7);
        assert!(!mutex.is_poisoned());
    }
}
