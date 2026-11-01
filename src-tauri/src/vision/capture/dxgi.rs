//! Borderless Windows desktop capture using DXGI Desktop Duplication.
//!
//! Unlike Windows Graphics Capture, Desktop Duplication does not add the colored
//! capture indicator border. A fresh desktop frame is acquired for each OCR
//! request; only the requested client-area ROI is copied to the CPU.
//! V14 reads the OCR ROI directly from the acquired DXGI texture with a GPU compute shader.
//! Only the compact ROI pixel buffer is read back; the full desktop texture is never
//! staged to CPU on the normal capture path. Persistent duplication, no-image-update
//! handling, ROI cache and black-frame retry behavior are retained.
use super::{region, Captured, Roi};
use std::{
    ffi::c_void,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use windows::{
    core::Interface,
    Win32::{
        Foundation::{BOOL, HWND, POINT, RECT},
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, Fxc::{D3DCompile, D3DCOMPILE_ENABLE_STRICTNESS, D3DCOMPILE_OPTIMIZATION_LEVEL3}},
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
                ID3D11ComputeShader, ID3D11Buffer, ID3D11UnorderedAccessView,
                D3D11_BUFFER_DESC, D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_UNORDERED_ACCESS,
                D3D11_RESOURCE_MISC_BUFFER_STRUCTURED, D3D11_USAGE_DEFAULT,
                D3D11_USAGE_STAGING, D3D11_CPU_ACCESS_READ,
                D3D11_MAP_READ,
                D3D11_MAPPED_SUBRESOURCE,
                D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_QUERY_DESC, D3D11_QUERY_EVENT,
            },
            Dxgi::{
                CreateDXGIFactory1, IDXGIFactory1, IDXGIAdapter, IDXGIAdapter1, IDXGIOutput,
                IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource, DXGI_OUTDUPL_FRAME_INFO,
            },
            Gdi::{ClientToScreen, MonitorFromWindow, MONITOR_DEFAULTTONEAREST},
        },
        UI::WindowsAndMessaging::{GetClientRect, IsIconic},
    },
};

const FRAME_TIMEOUT_MS: u32 = 1200;
const MAX_PIXELS: u64 = 40_000_000;
const STATS_INTERVAL: Duration = Duration::from_secs(60);

static CAPTURE_COUNT: AtomicU64 = AtomicU64::new(0);
static BLACK_FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static RETRY_ATTEMPT_COUNT: AtomicU64 = AtomicU64::new(0);
static RETRY_RECOVERED_COUNT: AtomicU64 = AtomicU64::new(0);
static RETRY_FAILED_COUNT: AtomicU64 = AtomicU64::new(0);
static UPDATED_IMAGE_FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static NO_IMAGE_UPDATE_COUNT: AtomicU64 = AtomicU64::new(0);
static CACHE_HIT_COUNT: AtomicU64 = AtomicU64::new(0);
static CACHE_MISS_COUNT: AtomicU64 = AtomicU64::new(0);
static UPDATED_BLACK_FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static PROTECTED_MASKED_FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static STATS_CLOCK: OnceLock<Instant> = OnceLock::new();
static LAST_STATS_REPORT_MS: AtomicU64 = AtomicU64::new(0);
static LIFECYCLE_MODE_LOGGED: OnceLock<()> = OnceLock::new();
static CORE_CREATED_COUNT: AtomicU64 = AtomicU64::new(0);
static CORE_REUSED_COUNT: AtomicU64 = AtomicU64::new(0);
static DUPLICATION_CREATED_COUNT: AtomicU64 = AtomicU64::new(0);
static DUPLICATION_REUSED_COUNT: AtomicU64 = AtomicU64::new(0);
static FIRST_SESSION_FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static FIRST_SESSION_FRAME_BLACK_COUNT: AtomicU64 = AtomicU64::new(0);
static REUSED_SESSION_FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static REUSED_SESSION_FRAME_BLACK_COUNT: AtomicU64 = AtomicU64::new(0);
static PERSISTENT_SESSION: Mutex<Option<DxgiSession>> = Mutex::new(None);

struct DxgiSession {
    monitor_key: usize,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    output: IDXGIOutput1,
    persistent_duplication: Option<IDXGIOutputDuplication>,
    desktop: RECT,
    first_acquire_pending: bool,
    last_valid_roi: Option<CachedRoi>,
    gpu_probe: Option<GpuProbe>,
}

/// Last known-good ROI. Cache identity includes the window/client geometry and
/// requested ROI so a stale image is never reused for a different crop.
struct CachedRoi {
    origin_x: i32,
    origin_y: i32,
    client_width: u32,
    client_height: u32,
    roi_x: u32,
    roi_y: u32,
    roi_width: u32,
    roi_height: u32,
    image: image::RgbImage,
}

impl CachedRoi {
    fn matches(&self, origin: POINT, cw: u32, ch: u32, x: u32, y: u32, w: u32, h: u32) -> bool {
        self.origin_x == origin.x && self.origin_y == origin.y
            && self.client_width == cw && self.client_height == ch
            && self.roi_x == x && self.roi_y == y
            && self.roi_width == w && self.roi_height == h
    }
}

fn store_valid_roi_cache(
    session: &mut DxgiSession,
    origin: POINT,
    cw: u32,
    ch: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    image: &image::RgbImage,
) {
    session.last_valid_roi = Some(CachedRoi {
        origin_x: origin.x,
        origin_y: origin.y,
        client_width: cw,
        client_height: ch,
        roi_x: x,
        roi_y: y,
        roi_width: w,
        roi_height: h,
        image: image.clone(),
    });
}

/// Counts every capture request, including requests that return early.
/// The guard also gives the statistics reporter a single exit point.
struct CaptureStatsGuard;

impl CaptureStatsGuard {
    fn new() -> Self {
        CAPTURE_COUNT.fetch_add(1, Ordering::Relaxed);
        Self
    }
}

impl Drop for CaptureStatsGuard {
    fn drop(&mut self) {
        maybe_report_capture_stats();
    }
}

/// Report one interval summary. No per-frame or per-retry log is emitted.
fn maybe_report_capture_stats() {
    let clock = STATS_CLOCK.get_or_init(Instant::now);
    let now_ms = clock.elapsed().as_millis() as u64;
    let interval_ms = STATS_INTERVAL.as_millis() as u64;
    let previous = LAST_STATS_REPORT_MS.load(Ordering::Relaxed);
    if now_ms < previous.saturating_add(interval_ms)
        || LAST_STATS_REPORT_MS
            .compare_exchange(previous, now_ms, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
    {
        return;
    }

    let captures = CAPTURE_COUNT.swap(0, Ordering::Relaxed);
    let black_frames = BLACK_FRAME_COUNT.swap(0, Ordering::Relaxed);
    let retry_attempts = RETRY_ATTEMPT_COUNT.swap(0, Ordering::Relaxed);
    let recovered = RETRY_RECOVERED_COUNT.swap(0, Ordering::Relaxed);
    let retry_failed = RETRY_FAILED_COUNT.swap(0, Ordering::Relaxed);
    let no_image_update = NO_IMAGE_UPDATE_COUNT.swap(0, Ordering::Relaxed);
    let cache_hits = CACHE_HIT_COUNT.swap(0, Ordering::Relaxed);
    let protected_masked = PROTECTED_MASKED_FRAME_COUNT.swap(0, Ordering::Relaxed);
    let mode = if std::env::var("D2RHUB_DXGI_LIFECYCLE")
        .map(|v| v.eq_ignore_ascii_case("fresh"))
        .unwrap_or(false) { "fresh" } else { "persistent" };
    let black_rate = if captures == 0 { 0.0 } else { black_frames as f64 * 100.0 / captures as f64 };

    crate::logger::log_msg(
        "INFO",
        "VisionCapture",
        &format!(
            "DXGI 近 60 秒摘要：mode={} captures={} black={}({:.1}%) retry={} recovered={} failed={} no_update={} cache_hit={} protected={}",
            mode, captures, black_frames, black_rate, retry_attempts, recovered,
            retry_failed, no_image_update, cache_hits, protected_masked
        ),
    );
}

/// Release the persistent DXGI duplication/device session when vision is stopped or reconfigured.
pub(super) fn release_sessions() {
    if let Ok(mut session) = PERSISTENT_SESSION.lock() {
        *session = None;
    }
}

pub(super) fn capture(pid: u32, roi: Option<Roi>) -> Result<Captured, String> {
    let _stats_guard = CaptureStatsGuard::new();
    if roi.is_some_and(|r| !r.valid()) { return Err("截图区域无效".into()); }
    let hwnd_raw = crate::infrastructure::system::find_game_hwnd(pid).ok_or("目标游戏窗口不存在")?;
    let hwnd = HWND(hwnd_raw as *mut _);
    let _dpi = crate::infrastructure::game_layout_windows::PhysicalDpiScope::enter();

    let (mut client, mut origin) = (RECT::default(), POINT::default());
    unsafe {
        if IsIconic(hwnd).as_bool() { return Err("目标游戏已最小化，无法核验".into()); }
        GetClientRect(hwnd, &mut client).map_err(|e| e.to_string())?;
        if !ClientToScreen(hwnd, &mut origin).as_bool() { return Err("无法定位游戏客户区".into()); }
    }
    let (cw, ch) = ((client.right - client.left) as u32, (client.bottom - client.top) as u32);
    region(cw, ch, None)?;
    let (x, y, w, h) = region(cw, ch, roi)?;
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let monitor_key = monitor.0 as usize;

    // Production default: reuse one Duplication session. Set
    // D2RHUB_DXGI_LIFECYCLE=fresh only when explicitly testing per-request
    // DuplicateOutput behavior.
    let persistent_mode = !std::env::var("D2RHUB_DXGI_LIFECYCLE")
        .map(|v| v.eq_ignore_ascii_case("fresh"))
        .unwrap_or(false);
    LIFECYCLE_MODE_LOGGED.get_or_init(|| {
        crate::logger::log_msg(
            "INFO", "VisionCapture",
            &format!("DXGI 生命周期对照模式：{}", if persistent_mode { "persistent（复用 Duplication 会话）" } else { "fresh（每次请求重新 DuplicateOutput）" }),
        );
    });

    let mut guard = PERSISTENT_SESSION.lock()
        .map_err(|_| "DXGI 会话锁已中毒".to_string())?;
    if guard.as_ref().map_or(true, |session| session.monitor_key != monitor_key) {
        *guard = Some(create_session(monitor, monitor_key)?);
    } else {
        CORE_REUSED_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    let session = guard.as_mut().ok_or("DXGI 会话初始化失败")?;

    // Both experiment arms share the same D3D11 device/context. Only the
    // IDXGIOutputDuplication lifetime differs, isolating DuplicateOutput.
    let duplication = if persistent_mode {
        if session.persistent_duplication.is_none() {
            let created = unsafe { session.output.DuplicateOutput(&session.device) }
                .map_err(|e| format!("DXGI 桌面复制初始化失败：{e}，HRESULT=0x{:08X}", e.code().0 as u32))?;
            session.persistent_duplication = Some(created);
            session.first_acquire_pending = true;
            DUPLICATION_CREATED_COUNT.fetch_add(1, Ordering::Relaxed);
        } else {
            DUPLICATION_REUSED_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        session.persistent_duplication.as_ref().unwrap().clone()
    } else {
        let created = unsafe { session.output.DuplicateOutput(&session.device) }
            .map_err(|e| format!("DXGI 桌面复制初始化失败：{e}，HRESULT=0x{:08X}", e.code().0 as u32))?;
        DUPLICATION_CREATED_COUNT.fetch_add(1, Ordering::Relaxed);
        created
    };

    let result = capture_with_session(session, &duplication, persistent_mode, origin, cw, ch, x, y, w, h);
    if persistent_mode && result.as_ref().err().is_some_and(|e| {
        let lower = e.to_ascii_lowercase();
        lower.contains("access_lost") || lower.contains("887a0026")
    }) {
        // Recreate only the duplication object; retain the same D3D11 core.
        session.persistent_duplication = None;
        session.first_acquire_pending = true;
        session.last_valid_roi = None;
    }
    result
}

// V14: GPU extracts every ROI pixel from the acquired DXGI texture. CPU only
// unpacks the compact uint buffer into image::RgbImage; it no longer copies or
// scans the full desktop texture on the normal capture path.
struct GpuProbe {
    shader: ID3D11ComputeShader,
    params_buffer: ID3D11Buffer,
    event_query: windows::Win32::Graphics::Direct3D11::ID3D11Query,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GpuParams {
    tex_width: u32,
    tex_height: u32,
    rect_x: u32,
    rect_y: u32,
    rect_width: u32,
    rect_height: u32,
    grid_x: u32,
    grid_y: u32,
    output_base: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
}

const GPU_ROI_CS: &str = r#"
Texture2D<float4> SourceTexture : register(t0);
RWStructuredBuffer<uint> Pixels : register(u0);
cbuffer Params : register(b0) {
    uint TexWidth; uint TexHeight;
    uint RectX; uint RectY;
    uint RectWidth; uint RectHeight;
    uint GridX; uint GridY;
    uint OutputBase; uint Pad0; uint Pad1; uint Pad2;
};
[numthreads(8, 8, 1)]
void CSMain(uint3 tid : SV_DispatchThreadID) {
    if (tid.x >= GridX || tid.y >= GridY) return;
    uint sx = RectX + tid.x;
    uint sy = RectY + tid.y;
    if (sx >= TexWidth || sy >= TexHeight) return;
    float3 rgb = saturate(SourceTexture.Load(int3(sx, sy, 0)).rgb);
    uint r = (uint)(rgb.r * 255.0 + 0.5);
    uint g = (uint)(rgb.g * 255.0 + 0.5);
    uint b = (uint)(rgb.b * 255.0 + 0.5);
    Pixels[OutputBase + tid.y * RectWidth + tid.x] = (r << 16) | (g << 8) | b;
}
"#;

fn create_gpu_probe(device: &ID3D11Device) -> Result<GpuProbe, String> {
    use windows::core::PCSTR;
    let mut bytecode = None;
    let mut errors = None;
    let entry = b"CSMain\0";
    let target = b"cs_5_0\0";
    unsafe {
        D3DCompile(
            GPU_ROI_CS.as_ptr() as *const c_void,
            GPU_ROI_CS.len(),
            PCSTR::null(), None, None,
            PCSTR(entry.as_ptr()), PCSTR(target.as_ptr()),
            D3DCOMPILE_ENABLE_STRICTNESS | D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0, &mut bytecode, Some(std::ptr::from_mut(&mut errors)),
        )
    }.map_err(|e| {
        let detail = errors.as_ref().map(|b| unsafe {
            let p = b.GetBufferPointer() as *const u8;
            let n = b.GetBufferSize();
            String::from_utf8_lossy(std::slice::from_raw_parts(p, n)).into_owned()
        }).unwrap_or_default();
        format!("DXGI ROI Compute Shader 编译失败：{e} {detail}")
    })?;
    let blob = bytecode.ok_or("D3DCompile 未返回 ROI Shader 字节码")?;
    let bytes = unsafe { std::slice::from_raw_parts(blob.GetBufferPointer() as *const u8, blob.GetBufferSize()) };
    let mut shader = None;
    unsafe { device.CreateComputeShader(bytes, None, Some(&mut shader)) }
        .map_err(|e| format!("DXGI ROI Compute Shader 创建失败：{e}"))?;
    let shader = shader.ok_or("ROI Compute Shader 创建成功但对象为空")?;

    let params_desc = D3D11_BUFFER_DESC {
        ByteWidth: std::mem::size_of::<GpuParams>() as u32,
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
        CPUAccessFlags: 0, MiscFlags: 0, StructureByteStride: 0,
    };
    let mut params_buffer = None;
    unsafe { device.CreateBuffer(&params_desc, None, Some(&mut params_buffer)) }
        .map_err(|e| format!("GPU ROI 参数 Buffer 创建失败：{e}"))?;
    let params_buffer = params_buffer.ok_or("GPU ROI 参数 Buffer 为空")?;

    let query_desc = D3D11_QUERY_DESC { Query: D3D11_QUERY_EVENT, MiscFlags: 0 };
    let mut event_query = None;
    unsafe { device.CreateQuery(&query_desc, Some(&mut event_query)) }
        .map_err(|e| format!("GPU ROI EVENT Query 创建失败：{e}"))?;
    let event_query = event_query.ok_or("GPU ROI EVENT Query 为空")?;
    Ok(GpuProbe { shader, params_buffer, event_query })
}

fn gpu_extract_roi(
    session: &mut DxgiSession,
    texture: &ID3D11Texture2D,
    desc: &D3D11_TEXTURE2D_DESC,
    roi: (u32, u32, u32, u32),
) -> Result<image::RgbImage, String> {
    let (rx, ry, rw, rh) = roi;
    if rw == 0 || rh == 0 { return Err("GPU ROI 尺寸为零".into()); }
    let pixel_count = u64::from(rw) * u64::from(rh);
    let byte_width = pixel_count.checked_mul(4).filter(|v| *v <= u32::MAX as u64)
        .ok_or("GPU ROI 输出 Buffer 尺寸溢出")? as u32;
    let device = session.device.clone();
    if session.gpu_probe.is_none() { session.gpu_probe = Some(create_gpu_probe(&device)?); }
    let probe = session.gpu_probe.as_ref().ok_or("GPU ROI Probe 未初始化")?;
    let context = session.context.clone();

    let output_desc = D3D11_BUFFER_DESC {
        ByteWidth: byte_width, Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
        CPUAccessFlags: 0, MiscFlags: D3D11_RESOURCE_MISC_BUFFER_STRUCTURED.0 as u32,
        StructureByteStride: 4,
    };
    let mut output = None;
    unsafe { device.CreateBuffer(&output_desc, None, Some(&mut output)) }
        .map_err(|e| format!("GPU ROI 输出 Buffer 创建失败：{e}"))?;
    let output = output.ok_or("GPU ROI 输出 Buffer 为空")?;
    let mut uav = None;
    unsafe { device.CreateUnorderedAccessView(&output, None, Some(&mut uav)) }
        .map_err(|e| format!("GPU ROI UAV 创建失败：{e}"))?;
    let uav = uav.ok_or("GPU ROI UAV 为空")?;

    let staging_desc = D3D11_BUFFER_DESC {
        ByteWidth: byte_width, Usage: D3D11_USAGE_STAGING,
        BindFlags: 0, CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0, StructureByteStride: 0,
    };
    let mut staging = None;
    unsafe { device.CreateBuffer(&staging_desc, None, Some(&mut staging)) }
        .map_err(|e| format!("GPU ROI Readback Buffer 创建失败：{e}"))?;
    let staging = staging.ok_or("GPU ROI Readback Buffer 为空")?;

    let srv = {
        let mut value = None;
        unsafe { device.CreateShaderResourceView(texture, None, Some(&mut value)) }
            .map_err(|e| format!("DXGI 原始纹理 SRV 创建失败：{e}"))?;
        value.ok_or("DXGI 原始纹理 SRV 为空")?
    };
    let params = GpuParams {
        tex_width: desc.Width, tex_height: desc.Height,
        rect_x: rx, rect_y: ry, rect_width: rw, rect_height: rh,
        grid_x: rw, grid_y: rh, output_base: 0, ..Default::default()
    };
    unsafe {
        context.UpdateSubresource(&probe.params_buffer, 0, None, &params as *const _ as *const c_void, 0, 0);
        context.CSSetShader(Some(&probe.shader), None);
        context.CSSetShaderResources(0, Some(&[Some(srv.clone())]));
        context.CSSetUnorderedAccessViews(0, 1, Some(&[Some(uav.clone())] as *const Option<ID3D11UnorderedAccessView>), None);
        context.CSSetConstantBuffers(0, Some(&[Some(probe.params_buffer.clone())]));
        context.Dispatch((rw + 7) / 8, (rh + 7) / 8, 1);
        context.CSSetShaderResources(0, Some(&[None]));
        context.CSSetUnorderedAccessViews(0, 1, Some(&[None] as *const Option<ID3D11UnorderedAccessView>), None);
        context.CSSetShader(None, None);
        context.CopyResource(&staging, &output);
        context.End(&probe.event_query);
        context.Flush();
    }
    wait_for_event_query(&context, &probe.event_query, "GPU ROI 输出同步")?;
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe { context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }
        .map_err(|e| format!("GPU ROI Readback Map 失败：{e}"))?;
    let result = unsafe {
        if mapped.pData.is_null() {
            Err("GPU ROI Readback 地址为空".to_string())
        } else {
            let packed = std::slice::from_raw_parts(mapped.pData as *const u32, pixel_count as usize);
            let mut rgb = vec![0u8; pixel_count as usize * 3];
            for (i, value) in packed.iter().copied().enumerate() {
                rgb[i * 3] = ((value >> 16) & 0xff) as u8;
                rgb[i * 3 + 1] = ((value >> 8) & 0xff) as u8;
                rgb[i * 3 + 2] = (value & 0xff) as u8;
            }
            image::RgbImage::from_raw(rw, rh, rgb)
                .ok_or_else(|| "GPU ROI 图像长度无效".to_string())
        }
    };
    unsafe { context.Unmap(&staging, 0); }
    result
}

fn create_session(
    monitor: windows::Win32::Graphics::Gdi::HMONITOR,
    monitor_key: usize,
) -> Result<DxgiSession, String> {
    let (adapter, output) = find_adapter_output(monitor)?;
    let (device, context) = create_device(&adapter)?;
    let output_desc = unsafe { output.GetDesc() }
        .map_err(|e| format!("读取 DXGI 显示器信息失败：{e}"))?;
    let output: IDXGIOutput1 = output.cast().map_err(|e| format!("DXGI 输出初始化失败：{e}"))?;
    CORE_CREATED_COUNT.fetch_add(1, Ordering::Relaxed);
    Ok(DxgiSession {
        monitor_key,
        device,
        context,
        output,
        persistent_duplication: None,
        desktop: output_desc.DesktopCoordinates,
        first_acquire_pending: true,
        last_valid_roi: None,
        gpu_probe: None,
    })
}

fn cpu_grid_stats(image: &image::RgbImage, grid_x: u32, grid_y: u32) -> [u32; 4] {
    if image.width() == 0 || image.height() == 0 || grid_x == 0 || grid_y == 0 {
        return [0; 4];
    }
    let mut stats = [0u32; 4];
    for gy in 0..grid_y {
        for gx in 0..grid_x {
            let px = ((gx as u64 * image.width() as u64) / grid_x as u64)
                .min(image.width() as u64 - 1) as u32;
            let py = ((gy as u64 * image.height() as u64) / grid_y as u64)
                .min(image.height() as u64 - 1) as u32;
            let pixel = image.get_pixel(px, py).0;
            let luma = *pixel.iter().max().unwrap_or(&0) as u32;
            stats[0] += 1;
            if luma > 0 { stats[1] += 1; }
            stats[2] = stats[2].max(luma);
            stats[3] += luma;
        }
    }
    stats
}

#[allow(clippy::too_many_arguments)]
fn capture_with_session(
    session: &mut DxgiSession,
    duplication: &IDXGIOutputDuplication,
    persistent_mode: bool,
    origin: POINT,
    cw: u32,
    ch: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Result<Captured, String> {
    // Clone the COM interface handles so the session cache can be mutably
    // updated later without holding immutable borrows into `session`.
    let device = session.device.clone();
    let context = session.context.clone();
    let desktop = session.desktop;
    let started = Instant::now();
    let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
    let mut resource: Option<IDXGIResource> = None;
    if let Err(e) = unsafe { duplication.AcquireNextFrame(FRAME_TIMEOUT_MS, &mut frame_info, &mut resource) } {
        return Err(format!("等待桌面帧失败（游戏可能被其他窗口遮挡或显卡不支持）：{}，HRESULT=0x{:08X}", e, e.code().0 as u32));
    }

    let is_first_session_frame = if persistent_mode {
        let first = session.first_acquire_pending;
        session.first_acquire_pending = false;
        first
    } else {
        true
    };
    if is_first_session_frame {
        FIRST_SESSION_FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
    } else {
        REUSED_SESSION_FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    let last_present_ticks = frame_info.LastPresentTime;
    let protected_masked = frame_info.ProtectedContentMaskedOut.as_bool();
    if protected_masked {
        PROTECTED_MASKED_FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    if last_present_ticks != 0 {
        UPDATED_IMAGE_FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    let frame_class = if last_present_ticks == 0 {
        "no_desktop_image_update"
    } else if frame_info.AccumulatedFrames == 0 {
        "present_timestamp_without_accumulated_frames"
    } else {
        "desktop_image_update"
    };

    let result = (|| {
        // LastPresentTime == 0 means DXGI did not report a new desktop image.
        // OCR does not need cursor-only updates, so reuse the last known-good
        // ROI only when the exact window/client/ROI geometry still matches.
        if last_present_ticks == 0 {
            NO_IMAGE_UPDATE_COUNT.fetch_add(1, Ordering::Relaxed);
            if let Some(cache) = session.last_valid_roi.as_ref() {
                if cache.matches(origin, cw, ch, x, y, w, h) {
                    CACHE_HIT_COUNT.fetch_add(1, Ordering::Relaxed);
                    return Ok(Captured {
                        image: cache.image.clone(),
                        metrics: super::CaptureMetrics {
                            wait_frame_ms: started.elapsed().as_secs_f64() * 1000.0,
                            readback_ms: 0.0,
                            client_width: cw, client_height: ch,
                            output_width: w, output_height: h,
                            total_ms: started.elapsed().as_secs_f64() * 1000.0,
                            ..Default::default()
                        },
                    });
                }
            }
            CACHE_MISS_COUNT.fetch_add(1, Ordering::Relaxed);
            // This acquisition contains no new desktop image. Reading its texture
            // after a cache miss can misclassify an unchanged/stale surface as a
            // fresh black frame. Release it and wait for a frame with a nonzero
            // LastPresentTime instead of doing a Staging readback.
            return Err("DXGI_NO_IMAGE_UPDATE_CACHE_MISS".into());
        }
        let resource = resource.ok_or("DXGI 未返回桌面帧")?;
        let texture: ID3D11Texture2D = resource.cast().map_err(|e| e.to_string())?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc); }
        let format = desc.Format.0;
        if !matches!(format, 87 | 91 | 88 | 93 | 28 | 29) {
            return Err(format!("DXGI 返回了暂不支持的像素格式：{}（纹理 {}x{}）", format, desc.Width, desc.Height));
        }
        if desc.SampleDesc.Count != 1 {
            return Err(format!("DXGI 桌面帧纹理 sample_count={}，无法直接创建匹配的 staging 纹理", desc.SampleDesc.Count));
        }

        if u64::from(desc.Width) * u64::from(desc.Height) > MAX_PIXELS {
            return Err("桌面截图尺寸超过安全限制".into());
        }
        // V14: GPU extracts the complete ROI directly from the acquired source texture.
        let client_x = i64::from(origin.x) - i64::from(desktop.left);
        let client_y = i64::from(origin.y) - i64::from(desktop.top);
        let roi_left = (client_x + i64::from(x)).max(0).min(i64::from(desc.Width)) as u32;
        let roi_top = (client_y + i64::from(y)).max(0).min(i64::from(desc.Height)) as u32;
        let roi_right = (client_x + i64::from(x) + i64::from(w)).max(0).min(i64::from(desc.Width)) as u32;
        let roi_bottom = (client_y + i64::from(y) + i64::from(h)).max(0).min(i64::from(desc.Height)) as u32;
        if roi_right <= roi_left || roi_bottom <= roi_top {
            return Err("GPU ROI 与 DXGI 输出没有交集".into());
        }
        let roi_fully_inside = roi_left as i64 == client_x + i64::from(x)
            && roi_top as i64 == client_y + i64::from(y)
            && roi_right - roi_left == w && roi_bottom - roi_top == h;
        if !roi_fully_inside {
            return Err(format!("GPU ROI 超出 DXGI 输出边界：requested={}x{} clipped={}x{}", w, h, roi_right-roi_left, roi_bottom-roi_top));
        }
        let image = gpu_extract_roi(session, &texture, &desc,
            (roi_left, roi_top, roi_right - roi_left, roi_bottom - roi_top))?;
        let sampled = image.pixels().step_by(97).map(|p| (u32::from(p.0[0]) + u32::from(p.0[1]) + u32::from(p.0[2])) / 3).collect::<Vec<_>>();
        let max_luma = sampled.iter().copied().max().unwrap_or(0);
        if max_luma < 5 {
            return Err(format!("DXGI 返回黑帧：class={}，LastPresentTime={}，AccumulatedFrames={}，format={}，纹理={}x{}，截图={}x{}，采样最大亮度={}",
                frame_class, last_present_ticks, frame_info.AccumulatedFrames, format, desc.Width, desc.Height, image.width(), image.height(), max_luma));
        }
        Ok(Captured {
            image,
            metrics: super::CaptureMetrics {
                wait_frame_ms: started.elapsed().as_secs_f64() * 1000.0,
                readback_ms: started.elapsed().as_secs_f64() * 1000.0,
                client_width: cw, client_height: ch, output_width: w, output_height: h,
                total_ms: started.elapsed().as_secs_f64() * 1000.0,
                ..Default::default()
            },
        })
    })();

    // Preserve V12's exact ownership sequence: release the normal frame before retry.
    unsafe { let _ = duplication.ReleaseFrame(); }
    if last_present_ticks != 0
        && result.as_ref().err().is_some_and(|e| e.contains("DXGI 返回黑帧"))
    {
        UPDATED_BLACK_FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    if let Ok(captured) = result.as_ref() {
        // Update only after a non-black, successful capture. This also seeds
        // the cache when the first acquired frame had no present timestamp but
        // its fallback readback nevertheless contained a valid ROI.
        // A black frame never overwrites the last known-good OCR image.
        store_valid_roi_cache(session, origin, cw, ch, x, y, w, h, &captured.image);
    }
    match result {
        Ok(captured) => Ok(captured),
        Err(error) if error.contains("DXGI 返回黑帧") => {
            BLACK_FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
            if is_first_session_frame {
                FIRST_SESSION_FRAME_BLACK_COUNT.fetch_add(1, Ordering::Relaxed);
            } else {
                REUSED_SESSION_FRAME_BLACK_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            retry_for_next_updated_frame(
                error, duplication, &device, &context, desktop, origin, cw, ch,
                x, y, w, h, started, session,
            )
        }
        Err(error) if error == "DXGI_NO_IMAGE_UPDATE_CACHE_MISS" => {
            // Not a black frame: the acquired frame explicitly carried no new
            // desktop image and there was no geometry-matched ROI cache.
            retry_for_next_updated_frame(
                "DXGI 无桌面图像更新且没有匹配的有效缓存".to_string(),
                duplication, &device, &context, desktop, origin, cw, ch,
                x, y, w, h, started, session,
            )
        }
        Err(error) => Err(error),
    }
}

#[allow(clippy::too_many_arguments)]
fn retry_for_next_updated_frame(
    original_error: String,
    duplication: &windows::Win32::Graphics::Dxgi::IDXGIOutputDuplication,
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    desktop: RECT,
    origin: POINT,
    cw: u32,
    ch: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    started: Instant,
    session: &mut DxgiSession,
) -> Result<Captured, String> {
    match retry_black_frame_capture(
        duplication, device, context, desktop, origin, cw, ch, x, y, w, h, started,
    ) {
        Ok(captured) => {
            store_valid_roi_cache(session, origin, cw, ch, x, y, w, h, &captured.image);
            Ok(captured)
        }
        Err(retry_error) => Err(format!("{original_error}；等待后续有效桌面帧失败：{retry_error}")),
    }
}

/// V12: retry a black normal capture by acquiring up to three subsequent
/// Desktop Duplication frames. Read each acquired frame before ReleaseFrame,
/// using the capture() function's existing immediate context. A successful ROI
/// image is returned directly to the OCR pipeline.
#[allow(clippy::too_many_arguments)]
fn retry_black_frame_capture(
    duplication: &windows::Win32::Graphics::Dxgi::IDXGIOutputDuplication,
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    desktop: RECT,
    origin: POINT,
    cw: u32,
    ch: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    started: Instant,
) -> Result<Captured, String> {
    const MAX_RETRIES: usize = 3;
    const RETRY_TIMEOUT_MS: u32 = 180;
    let client_x = i64::from(origin.x) - i64::from(desktop.left);
    let client_y = i64::from(origin.y) - i64::from(desktop.top);
    let mut last_error = "没有取得可用重试帧".to_string();

    for attempt in 1..=MAX_RETRIES {
        RETRY_ATTEMPT_COUNT.fetch_add(1, Ordering::Relaxed);
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
    
        match unsafe {
            duplication.AcquireNextFrame(RETRY_TIMEOUT_MS, &mut info, &mut resource)
        } {
            Ok(()) => {
                // Do not spend a GPU readback on a cursor-only/no-image-update
                // frame. The caller already has a last-known-good ROI cache
                // when one is available; otherwise try the next acquired frame.
                if info.LastPresentTime == 0 {
                    NO_IMAGE_UPDATE_COUNT.fetch_add(1, Ordering::Relaxed);
                    drop(resource);
                    if let Err(release_error) = unsafe { duplication.ReleaseFrame() } {
                        RETRY_FAILED_COUNT.fetch_add(1, Ordering::Relaxed);
                        return Err(format!("DXGI 重试帧[{}] 无图像更新且 ReleaseFrame 失败：{}", attempt, release_error));
                    }
                    last_error = format!(
                        "DXGI 重试帧[{}] 无桌面图像更新：AccumulatedFrames={} LastPresentTime={} LastMouseUpdateTime={} MetadataBytes={}",
                        attempt, info.AccumulatedFrames, info.LastPresentTime,
                        info.LastMouseUpdateTime, info.TotalMetadataBufferSize
                    );
                    continue;
                }

                let frame_result = (|| -> Result<Captured, String> {
                    let resource = resource.as_ref().ok_or("DXGI 重试帧没有 IDXGIResource")?;
                    let texture: ID3D11Texture2D = resource
                        .cast()
                        .map_err(|e| format!("DXGI 重试帧转换 Texture2D 失败：{e}"))?;

                    let mut desc = D3D11_TEXTURE2D_DESC::default();
                    unsafe { texture.GetDesc(&mut desc); }
                    if !matches!(desc.Format.0, 87 | 91 | 88 | 93 | 28 | 29)
                        || desc.SampleDesc.Count != 1
                    {
                        return Err(format!(
                            "DXGI 重试帧纹理格式不支持：{}x{} format={} sample_count={}",
                            desc.Width, desc.Height, desc.Format.0, desc.SampleDesc.Count
                        ));
                    }
                    if u64::from(desc.Width) * u64::from(desc.Height) > MAX_PIXELS {
                        return Err("DXGI 重试帧尺寸超过安全限制".into());
                    }

                    let staging_desc = D3D11_TEXTURE2D_DESC {
                        Width: desc.Width,
                        Height: desc.Height,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: desc.Format,
                        SampleDesc: desc.SampleDesc,
                        Usage: D3D11_USAGE_STAGING,
                        BindFlags: 0,
                        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                        MiscFlags: 0,
                    };
                    let mut staging = None;
                    unsafe { device.CreateTexture2D(&staging_desc, None, Some(&mut staging)) }
                        .map_err(|e| format!("DXGI 重试帧创建 Staging 失败：{e}"))?;
                    let staging = staging.ok_or("DXGI 重试帧 Staging 纹理为空")?;

                    let query_desc = D3D11_QUERY_DESC {
                        Query: D3D11_QUERY_EVENT,
                        MiscFlags: 0,
                    };
                    let mut query = None;
                    unsafe { device.CreateQuery(&query_desc, Some(&mut query)) }
                        .map_err(|e| format!("DXGI 重试帧创建 EVENT Query 失败：{e}"))?;
                    let query = query.ok_or("DXGI 重试帧 EVENT Query 为空")?;

                    unsafe {
                        context.CopyResource(&staging, &texture);
                        context.End(&query);
                        context.Flush();
                    }
                    wait_for_event_query(
                        &context,
                        &query,
                        &format!("DXGI 重试帧[{}] GPU 复制同步", attempt),
                    )?;

                    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                    unsafe { context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }
                        .map_err(|e| format!("DXGI 重试帧 Map 失败：{e}"))?;

                    let image_result = unsafe {
                        let pitch = mapped.RowPitch as usize;
                        let required_len = pitch.checked_mul(desc.Height as usize);
                        if mapped.pData.is_null()
                            || pitch < desc.Width as usize * 4
                            || required_len.is_none()
                        {
                            Err("DXGI 重试帧 Map 地址或 RowPitch 无效".to_string())
                        } else {
                            let data = std::slice::from_raw_parts(
                                mapped.pData as *const u8,
                                required_len.unwrap(),
                            );
                            let request_left = client_x + i64::from(x);
                            let request_top = client_y + i64::from(y);
                            let request_right = request_left + i64::from(w);
                            let request_bottom = request_top + i64::from(h);
                            let copy_left = request_left.max(0);
                            let copy_top = request_top.max(0);
                            let copy_right = request_right.min(i64::from(desc.Width));
                            let copy_bottom = request_bottom.min(i64::from(desc.Height));

                            if copy_left >= copy_right || copy_top >= copy_bottom {
                                Err("DXGI 重试帧中游戏客户区与输出没有交集".to_string())
                            } else {
                                let mut roi_max_luma = 0u8;
                                for sy in (copy_top..copy_bottom).step_by(7) {
                                    for sx in (copy_left..copy_right).step_by(7) {
                                        let off = sy as usize * pitch + sx as usize * 4;
                                        let luma = ((u16::from(data[off])
                                            + u16::from(data[off + 1])
                                            + u16::from(data[off + 2])) / 3) as u8;
                                        roi_max_luma = roi_max_luma.max(luma);
                                    }
                                }

                                let mut rgb = vec![0u8; w as usize * h as usize * 3];
                                for src_y in copy_top..copy_bottom {
                                    let src_x = copy_left as usize;
                                    let src_start = src_y as usize * pitch + src_x * 4;
                                    let copy_width = (copy_right - copy_left) as usize;
                                    let dst_x = (copy_left - request_left) as usize;
                                    let dst_y = (src_y - request_top) as usize;
                                    let dst_start = (dst_y * w as usize + dst_x) * 3;
                                    for (i, pixel) in data[
                                        src_start..src_start + copy_width * 4
                                    ].chunks_exact(4).enumerate() {
                                        let dst = dst_start + i * 3;
                                        if matches!(desc.Format.0, 28 | 29) {
                                            rgb[dst..dst + 3].copy_from_slice(&pixel[..3]);
                                        } else {
                                            rgb[dst..dst + 3]
                                                .copy_from_slice(&[pixel[2], pixel[1], pixel[0]]);
                                        }
                                    }
                                }



                                if roi_max_luma < 5 {
                                    Err(format!(
                                        "DXGI 重试帧[{}] ROI 仍为黑帧：sample_max={}",
                                        attempt, roi_max_luma
                                    ))
                                } else {
                                    image::RgbImage::from_raw(w, h, rgb)
                                        .ok_or_else(|| "DXGI 重试帧 ROI 图像长度无效".to_string())
                                }
                            }
                        }
                    };
                    unsafe { context.Unmap(&staging, 0); }
                    let image = image_result?;
                    Ok(Captured {
                        image,
                        metrics: super::CaptureMetrics {
                            wait_frame_ms: started.elapsed().as_secs_f64() * 1000.0,
                            readback_ms: started.elapsed().as_secs_f64() * 1000.0,
                            client_width: cw,
                            client_height: ch,
                            output_width: w,
                            output_height: h,
                            total_ms: started.elapsed().as_secs_f64() * 1000.0,
                            ..Default::default()
                        },
                    })
                })();

                // Always release the duplication frame, including failed readbacks.
                drop(resource);
                let release_result = unsafe { duplication.ReleaseFrame() };
                if let Err(error) = release_result {
                    RETRY_FAILED_COUNT.fetch_add(1, Ordering::Relaxed);
                    return Err(format!("DXGI 重试帧[{}] ReleaseFrame 失败：{error}", attempt));
                }
                match frame_result {
                    Ok(captured) => {
                        RETRY_RECOVERED_COUNT.fetch_add(1, Ordering::Relaxed);
                        return Ok(captured);
                    }
                    Err(error) => {
                        last_error = error;
                    }
                }
            }
            Err(error) => {
                last_error = format!("DXGI 重试帧[{}] AcquireNextFrame 失败：{}，HRESULT=0x{:08X}", attempt, error, error.code().0 as u32);
                break;
            }
        }
    }

    RETRY_FAILED_COUNT.fetch_add(1, Ordering::Relaxed);
    Err(format!("DXGI 黑帧重试失败（最多 3 次）：{last_error}"))
}

fn wait_for_event_query(context: &ID3D11DeviceContext, query: &windows::Win32::Graphics::Direct3D11::ID3D11Query, label: &str) -> Result<(), String> {
    let started = Instant::now();
    let deadline = started + Duration::from_millis(750);
    let mut polls = 0u32;
    loop {
        polls += 1;
        let mut completed = BOOL(0);
        let status = unsafe { context.GetData(query, Some(&mut completed as *mut _ as *mut std::ffi::c_void), std::mem::size_of::<BOOL>() as u32, 0) };
        match status {
            Ok(()) if completed.as_bool() => return Ok(()),
            Ok(()) => {}
            Err(e) => return Err(format!("{} GetData失败：{e}", label)),
        }
        if Instant::now() >= deadline { break; }
        std::thread::sleep(Duration::from_millis(1));
    }
    Err(format!("{} 等待超时：polls={} wait_ms={:.2}", label, polls, started.elapsed().as_secs_f64() * 1000.0))
}

fn create_device(adapter: &IDXGIAdapter1) -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
    let adapter: IDXGIAdapter = adapter.cast().map_err(|e| format!("转换 DXGI Adapter 接口失败：{e}"))?;
    let (mut device, mut context) = (None, None);
    unsafe {
        D3D11CreateDevice(
            Some(&adapter), D3D_DRIVER_TYPE_UNKNOWN,
            windows::Win32::Foundation::HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION,
            Some(&mut device), None, Some(&mut context),
        ).map_err(|e| format!("在目标显示器适配器上创建 D3D11 设备失败：{e}"))?;
    }
    Ok((device.ok_or("Direct3D 设备为空")?, context.ok_or("Direct3D 上下文为空")?))
}

fn find_adapter_output(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> Result<(IDXGIAdapter1, IDXGIOutput), String> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(|e| format!("创建 DXGI Factory 失败：{e}"))?;
    let mut adapter_index = 0u32;
    while let Ok(adapter) = unsafe { factory.EnumAdapters1(adapter_index) } {
        let mut output_index = 0u32;
        while let Ok(output) = unsafe { adapter.EnumOutputs(output_index) } {
            let desc = unsafe { output.GetDesc() }.map_err(|e| format!("读取 DXGI 输出信息失败：{e}"))?;
            if desc.Monitor == monitor { return Ok((adapter, output)); }
            output_index += 1;
        }
        adapter_index += 1;
    }
    Err("未找到游戏窗口所在显示器对应的 DXGI Adapter/Output".into())
}