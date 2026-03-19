use anyhow::{Context, Result};
use chrono::Local;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use xcap::Monitor;

use super::encoder::FfmpegEncoder;

#[derive(Debug, Clone)]
pub struct RecordSettings {
    pub monitor_index: usize,
    pub output_path: Option<PathBuf>,
    pub fps: u32,
    pub duration_secs: Option<u64>,
    pub preset: String,
}

impl Default for RecordSettings {
    fn default() -> Self {
        Self {
            monitor_index: 0,
            output_path: None,
            fps: 30,
            duration_secs: None,
            preset: "medium".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecorderState {
    Idle,
    Recording,
    Finishing,
}

pub struct RecorderHandle {
    cmd_tx: Mutex<Option<Sender<()>>>,
    stop_flag: Arc<Mutex<bool>>,
}

impl RecorderHandle {
    pub fn start(
        settings: RecordSettings,
    ) -> Result<(Self, std::thread::JoinHandle<Result<PathBuf>>)> {
        let (cmd_tx, cmd_rx) = channel::<()>();
        let stop_flag: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

        let stop_flag_clone = Arc::clone(&stop_flag);

        let handle = thread::spawn(move || run_recording(settings, cmd_rx, stop_flag_clone));

        Ok((
            Self {
                cmd_tx: Mutex::new(Some(cmd_tx)),
                stop_flag,
            },
            handle,
        ))
    }

    pub fn stop(&self) {
        *self.stop_flag.lock() = true;
        if let Some(tx) = self.cmd_tx.lock().take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for RecorderHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn start_recording(settings: RecordSettings) -> Result<PathBuf> {
    let stop_flag: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

    #[cfg(feature = "record")]
    {
        let sf = Arc::clone(&stop_flag);
        ctrlc::set_handler(move || {
            *sf.lock() = true;
        })
        .ok();
    }

    let monitors = Monitor::all().context("Failed to enumerate monitors")?;

    if settings.monitor_index >= monitors.len() {
        anyhow::bail!(
            "Monitor index {} out of range (found {} monitor(s))",
            settings.monitor_index,
            monitors.len()
        );
    }

    let monitor = &monitors[settings.monitor_index];
    let width = monitor.width();
    let height = monitor.height();

    let output_path = resolve_record_path(&settings, width, height)?;

    let encoder = FfmpegEncoder::new(width, height, settings.fps, &output_path, &settings.preset)?;

    let encoder = Arc::new(Mutex::new(encoder));

    let frame_duration = Duration::from_secs_f64(1.0 / settings.fps as f64);
    let start_time = Instant::now();
    let mut frame_count: u64 = 0;
    let fps_u64 = settings.fps as u64;
    let max_frames = settings.duration_secs.map(|d| d * fps_u64);

    loop {
        if *stop_flag.lock() {
            break;
        }

        if let Some(max) = max_frames {
            if frame_count >= max {
                break;
            }
        }

        let frame_start = Instant::now();

        let frame = monitor.capture_image().context("Failed to capture frame")?;

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
            std::io::Write::flush(&mut std::io::stdout()).ok();
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

    Ok(output_path)
}

fn run_recording(
    settings: RecordSettings,
    cmd_rx: std::sync::mpsc::Receiver<()>,
    stop_flag: Arc<Mutex<bool>>,
) -> Result<PathBuf> {
    let monitors = Monitor::all().context("Failed to enumerate monitors")?;

    if settings.monitor_index >= monitors.len() {
        anyhow::bail!("Monitor index out of range");
    }

    let monitor = &monitors[settings.monitor_index];
    let width = monitor.width();
    let height = monitor.height();
    let output_path = resolve_record_path(&settings, width, height)?;

    let encoder = FfmpegEncoder::new(width, height, settings.fps, &output_path, &settings.preset)?;

    let encoder = Arc::new(Mutex::new(encoder));

    let frame_duration = Duration::from_secs_f64(1.0 / settings.fps as f64);
    let start_time = Instant::now();
    let mut frame_count: u64 = 0;
    let fps_u64 = settings.fps as u64;
    let max_frames = settings.duration_secs.map(|d| d * fps_u64);

    loop {
        if *stop_flag.lock() {
            break;
        }

        if cmd_rx.try_recv().is_ok() {
            break;
        }

        if let Some(max) = max_frames {
            if frame_count >= max {
                break;
            }
        }

        let frame_start = Instant::now();

        let frame = match monitor.capture_image() {
            Ok(f) => f,
            Err(e) => {
                log::warn!("Frame capture failed: {}", e);
                thread::sleep(frame_duration);
                continue;
            }
        };

        {
            let mut enc = encoder.lock();
            if let Err(e) = enc.write_frame(frame.as_raw()) {
                log::warn!("Frame write failed: {}", e);
            }
        }

        frame_count += 1;

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
    log::info!(
        "Recording done: {} frames in {:.1}s (avg {:.1} fps) -> {}",
        frame_count,
        total_time,
        avg_fps,
        output_path.display()
    );

    Ok(output_path)
}

fn resolve_record_path(settings: &RecordSettings, width: u32, height: u32) -> Result<PathBuf> {
    if let Some(ref path) = settings.output_path {
        let p = PathBuf::from(path);
        if p.extension().is_none() {
            return Ok(p.with_extension("mp4"));
        }
        return Ok(p.clone());
    }

    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    Ok(dir.join(format!("capture_{}_{}x{}.mp4", timestamp, width, height)))
}
