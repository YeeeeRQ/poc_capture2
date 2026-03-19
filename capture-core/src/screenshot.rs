use anyhow::{Context, Result};
use chrono::Local;
use image::ImageEncoder;
use std::path::PathBuf;
use xcap::Monitor;

#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
}

impl MonitorInfo {
    pub fn all() -> Result<Vec<Self>> {
        let monitors = Monitor::all().context("Failed to enumerate monitors")?;
        Ok(monitors
            .iter()
            .enumerate()
            .map(|(i, m)| Self {
                index: i,
                name: m.name().to_string(),
                width: m.width(),
                height: m.height(),
                x: m.x(),
                y: m.y(),
            })
            .collect())
    }
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
    MonitorInfo::all()
}

pub fn take_screenshot(settings: &ScreenshotSettings) -> Result<PathBuf> {
    let monitors = Monitor::all().context("Failed to enumerate monitors")?;

    if settings.monitor_index >= monitors.len() {
        anyhow::bail!(
            "Monitor index {} out of range (found {} monitor(s))",
            settings.monitor_index,
            monitors.len()
        );
    }

    let monitor = &monitors[settings.monitor_index];
    let rgba_image = monitor
        .capture_image()
        .with_context(|| format!("Failed to capture monitor {}", settings.monitor_index))?;

    let (width, height) = (rgba_image.width(), rgba_image.height());
    let raw_rgba = rgba_image.into_raw();

    let output_path = resolve_screenshot_path(settings)?;

    let file = std::fs::File::create(&output_path)?;
    let mut writer = std::io::BufWriter::new(file);

    let ext = settings.format.to_lowercase();
    if ext == "jpg" || ext == "jpeg" {
        let mut rgb_data = Vec::with_capacity((width * height * 3) as usize);
        for chunk in raw_rgba.chunks_exact(4) {
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
            .write_image(&raw_rgba, width, height, image::ExtendedColorType::Rgba8)
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
