use anyhow::{Context, Result};
use chrono::Local;
use image::ImageEncoder;
use std::path::PathBuf;
use xcap::Monitor;

use crate::cli::ScreenshotArgs;

pub fn list_monitors() -> Result<()> {
    let monitors = Monitor::all().context("Failed to enumerate monitors")?;

    println!("Available monitors ({}):", monitors.len());
    for (i, monitor) in monitors.iter().enumerate() {
        let name = monitor.name();
        let (w, h) = (monitor.x(), monitor.y());
        println!("  [{}] {} ({}x{})", i, name, w, h);
    }

    Ok(())
}

pub fn take_screenshot(args: &ScreenshotArgs) -> Result<PathBuf> {
    let monitors = Monitor::all().context("Failed to enumerate monitors")?;

    let monitor_idx = args.monitor;
    if monitor_idx >= monitors.len() {
        anyhow::bail!(
            "Monitor index {} out of range (found {} monitor(s))",
            monitor_idx,
            monitors.len()
        );
    }

    let monitor = &monitors[monitor_idx];
    let rgba_image = monitor
        .capture_image()
        .with_context(|| format!("Failed to capture monitor {}", monitor_idx))?;

    let (width, height) = (rgba_image.width(), rgba_image.height());
    let raw_rgba = rgba_image.into_raw();

    let output_path = resolve_output_path(args)?;

    let ext = args.format.to_lowercase();
    let quality = args.quality;

    let file = std::fs::File::create(&output_path)?;
    let mut writer = std::io::BufWriter::new(file);

    if ext == "jpg" || ext == "jpeg" {
        let mut rgb_data = Vec::with_capacity((width * height * 3) as usize);
        for chunk in raw_rgba.chunks_exact(4) {
            rgb_data.push(chunk[0]);
            rgb_data.push(chunk[1]);
            rgb_data.push(chunk[2]);
        }

        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, quality);
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

fn resolve_output_path(args: &ScreenshotArgs) -> Result<PathBuf> {
    if let Some(ref path) = args.output {
        let p = PathBuf::from(path);
        if p.extension().is_none() {
            let ext = if args.format == "jpg" { "jpg" } else { "png" };
            return Ok(p.with_extension(ext));
        }
        return Ok(p);
    }

    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let ext = if args.format == "jpg" { "jpg" } else { "png" };
    let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    Ok(dir.join(format!("screenshot_{}.{}", timestamp, ext)))
}
