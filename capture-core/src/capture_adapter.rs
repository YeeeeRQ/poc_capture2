use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam::channel;
use image::ImageEncoder;
use parking_lot::Mutex;

use windows_capture::{
    capture::{CaptureControl, Context as WinCtx, GraphicsCaptureApiHandler},
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    },
};

#[derive(Debug, Clone)]
pub struct RecordSettings {
    pub monitor_index: usize,
    pub output_path: Option<PathBuf>,
    pub duration_secs: Option<u64>,
    pub preset: String,
    pub target_fps: u32,
}

impl Default for RecordSettings {
    fn default() -> Self {
        Self {
            monitor_index: 0,
            output_path: None,
            duration_secs: None,
            preset: "medium".to_string(),
            target_fps: 60,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CaptureSettings {
    pub width: u32,
    pub height: u32,
    pub output_path: PathBuf,
    pub preset: String,
    pub duration_secs: Option<u64>,
    pub target_fps: u32,
    pub frame_tx: Option<channel::Sender<EncodedFrame>>,
    pub done_tx: Option<std::sync::mpsc::Sender<()>>,
}

#[derive(Clone)]
pub struct ScreenshotRequest {
    pub path: PathBuf,
    pub format: String,
    pub quality: u8,
}

pub struct EncodedFrame {
    pub data: Vec<u8>,
    pub timestamp: Duration,
}

pub struct CaptureState {
    pub rgba_buffer: Vec<u8>,
    pub start_time: Option<Instant>,
    pub duration: Option<Duration>,
    pub frame_count: Arc<AtomicU64>,
    pub total_elapsed: Arc<Mutex<Duration>>,
    pub last_log_elapsed: Arc<Mutex<Duration>>,
    pub stop_requested: Arc<AtomicU64>,
    pub last_sent_time: Arc<Mutex<Instant>>,
    pub frame_tx: Option<channel::Sender<EncodedFrame>>,
    pub done_tx: Option<std::sync::mpsc::Sender<()>>,
    pub snapshot_buffer: Arc<Mutex<Vec<u8>>>,
}

impl Default for CaptureState {
    fn default() -> Self {
        Self {
            rgba_buffer: Vec::new(),
            start_time: None,
            duration: None,
            frame_count: Arc::new(AtomicU64::new(0)),
            total_elapsed: Arc::new(Mutex::new(Duration::ZERO)),
            last_log_elapsed: Arc::new(Mutex::new(Duration::ZERO)),
            stop_requested: Arc::new(AtomicU64::new(0)),
            last_sent_time: Arc::new(Mutex::new(Instant::now())),
            frame_tx: None,
            done_tx: None,
            snapshot_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

pub struct CaptureHandler {
    pub settings: CaptureSettings,
    pub state: CaptureState,
}

impl CaptureHandler {
    pub fn setup(&mut self, settings: CaptureSettings) {
        self.state.frame_tx = settings.frame_tx.clone();
        self.state.done_tx = settings.done_tx.clone();
        self.settings = settings;
        self.state.start_time = Some(Instant::now());
        if let Some(dur) = self.settings.duration_secs {
            self.state.duration = Some(Duration::from_secs(dur));
        }
        log::info!(
            "Recording: {}x{} @ {} fps, output={}",
            self.settings.width,
            self.settings.height,
            self.settings.target_fps,
            self.settings.output_path.display()
        );
    }
}

pub fn encode_screenshot_to_file(
    req: &ScreenshotRequest,
    pixels: &[u8],
    width: u32,
    height: u32,
) -> anyhow::Result<()> {
    let start = Instant::now();
    log::info!("[Screenshot] encoding started, {} bytes", pixels.len());
    let file = std::fs::File::create(&req.path)?;
    let mut writer = std::io::BufWriter::new(file);
    let ext = req.format.to_lowercase();
    if ext == "bmp" {
        let encoder = image::codecs::bmp::BmpEncoder::new(&mut writer);
        encoder.write_image(pixels, width, height, image::ExtendedColorType::Rgba8)?;
    } else if ext == "jpg" || ext == "jpeg" {
        let mut rgb_data = Vec::with_capacity((width as usize * height as usize) * 3);
        for chunk in pixels.chunks_exact(4) {
            rgb_data.push(chunk[0]);
            rgb_data.push(chunk[1]);
            rgb_data.push(chunk[2]);
        }
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, req.quality);
        encoder.write_image(&rgb_data, width, height, image::ExtendedColorType::Rgb8)?;
    } else {
        let encoder = image::codecs::png::PngEncoder::new(&mut writer);
        encoder.write_image(pixels, width, height, image::ExtendedColorType::Rgba8)?;
    }
    log::info!(
        "Screenshot saved: {} ({:.1}ms)",
        req.path.display(),
        start.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = CaptureSettings;

    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(_ctx: WinCtx<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            settings: CaptureSettings {
                width: 0,
                height: 0,
                output_path: PathBuf::new(),
                preset: "medium".to_string(),
                duration_secs: None,
                target_fps: 60,
                frame_tx: None,
                done_tx: None,
            },
            state: CaptureState::default(),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if self.state.stop_requested.load(Ordering::SeqCst) > 0 {
            return Ok(());
        }

        if self.state.frame_tx.is_none() {
            return Ok(());
        }

        let width = self.settings.width as usize;
        let height = self.settings.height as usize;
        let frame_size = width * height * 4;

        let interval = Duration::from_secs_f64(1.0 / self.settings.target_fps as f64);
        let now = Instant::now();
        {
            let mut last = self.state.last_sent_time.lock();
            if now.duration_since(*last) < interval {
                return Ok(());
            }
            *last = now;
        }

        if self.state.rgba_buffer.len() < frame_size {
            self.state.rgba_buffer.resize(frame_size, 0);
        }

        let mut fb = frame.buffer()?;
        let bgra = fb.as_nopadding_buffer()?;
        let copy_len = bgra.len().min(frame_size);

        bgra_to_rgba_inplace(&bgra[..copy_len], &mut self.state.rgba_buffer[..frame_size]);

        let timestamp = self
            .state
            .start_time
            .map(|s| s.elapsed())
            .unwrap_or_default();

        let frame = EncodedFrame {
            data: self.state.rgba_buffer[..frame_size].to_vec(),
            timestamp,
        };

        if let Some(ref tx) = self.state.frame_tx {
            if tx.send(frame).is_err() {
                log::warn!("Frame channel send failed (encoder thread died)");
            } else {
                self.state.frame_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        let total = timestamp;
        *self.state.total_elapsed.lock() = total;

        let last_log = *self.state.last_log_elapsed.lock();
        if total.saturating_sub(last_log) >= Duration::from_secs(1) {
            *self.state.last_log_elapsed.lock() = total;
            let written = self.state.frame_count.load(Ordering::Relaxed);
            let fps = if total.as_secs_f64() > 0.0 {
                written as f64 / total.as_secs_f64()
            } else {
                0.0
            };
            log::info!(
                "[{}s] Written={} fps={:.1}",
                format_f64(total.as_secs_f64(), 1),
                written,
                fps,
            );
        }

        if let Some(max) = self.state.duration {
            if total >= max {
                self.request_stop();
            }
        }

        {
            let mut snap = self.state.snapshot_buffer.lock();
            *snap = self.state.rgba_buffer[..frame_size].to_vec();
        }

        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.request_stop();
        Ok(())
    }
}

impl CaptureHandler {
    pub fn request_stop(&mut self) {
        let prev = self.state.stop_requested.fetch_add(1, Ordering::SeqCst);
        if prev == 0 {
            drop(self.state.frame_tx.take());
            if let Some(tx) = self.state.done_tx.take() {
                let _ = tx.send(());
            }
            let fw = self.state.frame_count.load(Ordering::Relaxed);
            let total_time = self
                .state
                .start_time
                .map(|s| s.elapsed())
                .unwrap_or_default();
            let avg_fps = if total_time.as_secs_f64() > 0.0 {
                fw as f64 / total_time.as_secs_f64()
            } else {
                0.0
            };
            let output_size = self
                .settings
                .output_path
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0);
            log::info!(
                "Recording done: frames={}, elapsed={}s, avg_fps={:.1}, output_size={}",
                fw,
                format_f64(total_time.as_secs_f64(), 2),
                avg_fps,
                output_size
            );
        }
    }
}

#[inline]
fn bgra_to_rgba_inplace(frame: &[u8], rgba: &mut [u8]) {
    let n = frame.len().min(rgba.len()) / 4;
    for i in 0..n {
        let j = i * 4;
        rgba[j] = frame[j + 2];
        rgba[j + 1] = frame[j + 1];
        rgba[j + 2] = frame[j];
        rgba[j + 3] = frame[j + 3];
    }
}

fn format_f64(v: f64, decimals: usize) -> String {
    format!("{:.1$}", v, decimals)
}

#[allow(unused_variables)]
pub fn setup_dpi_awareness() {}

pub fn get_monitor(index: usize) -> Result<Monitor, windows_capture::monitor::Error> {
    Monitor::from_index(index + 1)
}

pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>, windows_capture::monitor::Error> {
    let monitors = Monitor::enumerate()?;
    let mut result = Vec::with_capacity(monitors.len());
    for (i, m) in monitors.iter().enumerate() {
        result.push(MonitorInfo {
            index: i,
            name: m.name().unwrap_or_else(|_| "Unknown".to_string()),
            width: m.width().unwrap_or(0),
            height: m.height().unwrap_or(0),
            x: 0,
            y: 0,
        });
    }
    Ok(result)
}

pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
}

pub fn build_capture_settings(
    width: u32,
    height: u32,
    settings: &RecordSettings,
) -> CaptureSettings {
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let output_path = if let Some(ref path) = settings.output_path {
        let p = PathBuf::from(path);
        if p.extension().is_none() {
            p.with_extension("mp4")
        } else {
            p.clone()
        }
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(format!("capture_{}_{}x{}.mp4", timestamp, width, height))
    };

    CaptureSettings {
        width,
        height,
        output_path,
        preset: settings.preset.clone(),
        duration_secs: settings.duration_secs,
        target_fps: settings.target_fps,
        frame_tx: None,
        done_tx: None,
    }
}

pub fn start_capture(
    monitor: Monitor,
    capture_settings: CaptureSettings,
) -> Result<
    CaptureControl<CaptureHandler, Box<dyn std::error::Error + Send + Sync>>,
    windows_capture::capture::GraphicsCaptureApiError<Box<dyn std::error::Error + Send + Sync>>,
> {
    let min_interval = if capture_settings.target_fps > 0 {
        MinimumUpdateIntervalSettings::Custom(Duration::from_secs_f64(
            1.0 / capture_settings.target_fps as f64,
        ))
    } else {
        MinimumUpdateIntervalSettings::Default
    };

    let settings = Settings::new(
        monitor,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::WithBorder,
        SecondaryWindowSettings::Default,
        min_interval,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        capture_settings,
    );

    CaptureHandler::start_free_threaded(settings)
}
