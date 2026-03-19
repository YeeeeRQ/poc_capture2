use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Local;
use image::ImageEncoder;
use parking_lot::{Condvar, Mutex};
use std::path::PathBuf;

use windows_capture::{
    capture::GraphicsCaptureApiHandler,
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    },
};

#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone)]
pub struct ScreenshotSettings {
    pub monitor_index: usize,
    pub output_path: Option<PathBuf>,
    pub format: String,
    pub quality: u8,
}

impl Default for ScreenshotSettings {
    fn default() -> Self {
        Self {
            monitor_index: 0,
            output_path: None,
            format: "png".to_string(),
            quality: 90,
        }
    }
}

pub fn list_monitors() -> Result<Vec<MonitorInfo>> {
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

struct ScreenshotShared {
    result: Arc<Mutex<Option<Result<Vec<u8>>>>>,
    done: Arc<Condvar>,
    done_flag: Arc<Mutex<bool>>,
}

struct ScreenshotHandler {
    shared: ScreenshotShared,
}

impl GraphicsCaptureApiHandler for ScreenshotHandler {
    type Flags = ScreenshotShared;

    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: windows_capture::capture::Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self { shared: ctx.flags })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let width = frame.width();
        let height = frame.height();

        let mut fb = frame.buffer()?;
        let bgra = fb.as_nopadding_buffer()?;
        let frame_size = (width * height * 4) as usize;
        let copy_len = bgra.len().min(frame_size);

        let mut rgba = vec![0u8; frame_size];
        for i in 0..(copy_len / 4) {
            let j = i * 4;
            rgba[j] = bgra[j + 2];
            rgba[j + 1] = bgra[j + 1];
            rgba[j + 2] = bgra[j];
            rgba[j + 3] = bgra[j + 3];
        }

        *self.shared.result.lock() = Some(Ok(rgba));
        *self.shared.done_flag.lock() = true;
        self.shared.done.notify_one();

        capture_control.stop();
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        if !*self.shared.done_flag.lock() {
            *self.shared.result.lock() = Some(Err(anyhow::anyhow!("Capture session closed")));
            *self.shared.done_flag.lock() = true;
            self.shared.done.notify_one();
        }
        Ok(())
    }
}

pub fn take_screenshot(settings: &ScreenshotSettings) -> Result<PathBuf> {
    let monitors = Monitor::enumerate().context("Failed to enumerate monitors")?;

    if settings.monitor_index >= monitors.len() {
        anyhow::bail!(
            "Monitor index {} out of range (found {} monitor(s))",
            settings.monitor_index,
            monitors.len()
        );
    }

    let monitor = Monitor::from_index(settings.monitor_index + 1)
        .map_err(|e| anyhow::anyhow!("Failed to get monitor {}: {}", settings.monitor_index, e))?;

    let width = monitor.width().unwrap_or(0);
    let height = monitor.height().unwrap_or(0);

    let result = Arc::new(Mutex::new(None));
    let done = Arc::new(Condvar::new());
    let done_flag = Arc::new(Mutex::new(false));

    let shared = ScreenshotShared {
        result: Arc::clone(&result),
        done: Arc::clone(&done),
        done_flag: Arc::clone(&done_flag),
    };

    let screenshot_settings = Settings::new(
        monitor,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        shared,
    );

    let control = ScreenshotHandler::start_free_threaded(screenshot_settings)
        .map_err(|e| anyhow::anyhow!("Failed to start screenshot capture: {}", e))?;

    {
        let mut guard = done_flag.lock();
        while !*guard {
            done.wait(&mut guard);
        }
    }

    let _ = control.stop();

    let pixels = result
        .lock()
        .take()
        .context("Screenshot result not set")?
        .context("Screenshot failed")?;

    let output_path = resolve_screenshot_path(settings)?;

    let file = std::fs::File::create(&output_path)?;
    let mut writer = std::io::BufWriter::new(file);

    let ext = settings.format.to_lowercase();
    if ext == "jpg" || ext == "jpeg" {
        let mut rgb_data = Vec::with_capacity((width * height * 3) as usize);
        for chunk in pixels.chunks_exact(4) {
            rgb_data.push(chunk[0]);
            rgb_data.push(chunk[1]);
            rgb_data.push(chunk[2]);
        }

        let encoder =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, settings.quality);
        encoder
            .write_image(&rgb_data, width, height, image::ExtendedColorType::Rgb8)
            .context("Failed to write JPEG")?;
    } else {
        let encoder = image::codecs::png::PngEncoder::new(&mut writer);
        encoder
            .write_image(&pixels, width, height, image::ExtendedColorType::Rgba8)
            .context("Failed to write PNG")?;
    }

    log::info!("Screenshot saved: {}", output_path.display());
    Ok(output_path)
}

fn resolve_screenshot_path(settings: &ScreenshotSettings) -> Result<PathBuf> {
    if let Some(ref path) = settings.output_path {
        let p = PathBuf::from(path);
        if p.extension().is_none() {
            let ext = if settings.format == "jpg" {
                "jpg"
            } else {
                "png"
            };
            return Ok(p.with_extension(ext));
        }
        return Ok(p.clone());
    }

    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let ext = if settings.format == "jpg" {
        "jpg"
    } else {
        "png"
    };
    let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    Ok(dir.join(format!("screenshot_{}.{}", timestamp, ext)))
}
