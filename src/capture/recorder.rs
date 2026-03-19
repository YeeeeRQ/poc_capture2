use anyhow::{Context, Result};
use chrono::Local;
use parking_lot::Mutex;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use xcap::Monitor;

use crate::cli::RecordArgs;
use crate::encoder::FfmpegEncoder;

pub fn start_recording(args: &RecordArgs) -> Result<()> {
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
    let width = monitor.width();
    let height = monitor.height();
    let fps = args.fps;

    println!(
        "Recording monitor [{}] {}x{} @ {} fps...",
        monitor_idx, width, height, fps
    );

    let output_path = resolve_output_path(args, width, height)?;

    let encoder = Arc::new(Mutex::new(FfmpegEncoder::new(
        width,
        height,
        fps,
        &output_path,
        &args.preset,
    )?));

    let frame_duration = Duration::from_secs_f64(1.0 / fps as f64);

    let start_time = Instant::now();
    let mut frame_count: u64 = 0;
    let fps_u64 = fps as u64;
    let max_frames = args.duration.map(|d| d * fps_u64);

    let stop_flag: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

    ctrlc::set_handler({
        let stop_flag = Arc::clone(&stop_flag);
        move || {
            *stop_flag.lock() = true;
        }
    })
    .context("Failed to set Ctrl+C handler")?;

    println!("Recording... Press Ctrl+C to stop.");

    loop {
        if *stop_flag.lock() {
            println!("\nStopping...");
            break;
        }

        if let Some(max) = max_frames {
            if frame_count >= max {
                println!("\nDuration limit reached.");
                break;
            }
        }

        let frame_start = Instant::now();

        let frame = monitor
            .capture_image()
            .with_context(|| "Failed to capture frame")?;

        {
            let mut enc = encoder.lock();
            enc.write_frame(frame.as_raw())?;
        }

        frame_count += 1;

        if frame_count % fps_u64 == 1 {
            let elapsed = start_time.elapsed().as_secs_f64();
            let current_fps = if elapsed > 0.0 {
                frame_count as f64 / elapsed
            } else {
                0.0
            };
            print!(
                "\rFrame: {} | Time: {:.1}s | FPS: {:.1}   ",
                frame_count, elapsed, current_fps
            );
            std::io::stdout().flush().ok();
        }

        let elapsed = frame_start.elapsed();
        if elapsed < frame_duration {
            thread::sleep(frame_duration - elapsed);
        }
    }

    drop(encoder);

    let total_time = start_time.elapsed().as_secs_f64();
    let avg_fps = if total_time > 0.0 {
        frame_count as f64 / total_time
    } else {
        0.0
    };
    println!(
        "\nDone! {} frames in {:.1}s (avg {:.1} fps) -> {}",
        frame_count,
        total_time,
        avg_fps,
        output_path.display()
    );

    Ok(())
}

fn resolve_output_path(args: &RecordArgs, width: u32, height: u32) -> Result<PathBuf> {
    if let Some(ref path) = args.output {
        let p = PathBuf::from(path);
        if p.extension().is_none() {
            return Ok(p.with_extension("mp4"));
        }
        return Ok(p);
    }

    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    Ok(dir.join(format!("capture_{}_{}x{}.mp4", timestamp, width, height)))
}
