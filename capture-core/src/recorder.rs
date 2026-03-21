use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam::channel;
use parking_lot::{Condvar, Mutex};
use std::sync::atomic::AtomicBool;

use super::capture_adapter::{
    build_capture_settings, encode_screenshot_to_file, get_monitor, setup_dpi_awareness,
    EncodedFrame, RecordSettings, ScreenshotRequest,
};
use super::perf_sampler::{PerfSampler, PerfSamplerHandle};
use super::perf_stats::PerfStats;
use windows_capture::capture::CaptureControl;
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
};

pub struct RecorderHandle {
    control: Option<
        CaptureControl<
            super::capture_adapter::CaptureHandler,
            Box<dyn std::error::Error + Send + Sync>,
        >,
    >,
    #[allow(dead_code)]
    screenshot_stopped: Arc<AtomicBool>,
    #[allow(dead_code)]
    screenshot_join_handle: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    stop_flag: Arc<Mutex<bool>>,
    done_flag: Arc<Mutex<bool>>,
    done_condvar: Arc<Condvar>,
    screenshot_request: Arc<Mutex<Option<ScreenshotRequest>>>,
    screenshot_done: Arc<Condvar>,
    perf_sampler: Option<PerfSamplerHandle>,
    video_encoder: Option<Arc<parking_lot::Mutex<Option<VideoEncoder>>>>,
}

impl RecorderHandle {
    pub fn start(
        settings: RecordSettings,
        perf_stats: Option<Arc<PerfStats>>,
    ) -> Result<(Self, std::thread::JoinHandle<Result<PathBuf>>)> {
        setup_dpi_awareness();

        let monitor = get_monitor(settings.monitor_index).map_err(|e| {
            anyhow::anyhow!("Failed to get monitor {}: {}", settings.monitor_index, e)
        })?;
        let width = monitor
            .width()
            .map_err(|e| anyhow::anyhow!("Failed to get monitor width: {}", e))?;
        let height = monitor
            .height()
            .map_err(|e| anyhow::anyhow!("Failed to get monitor height: {}", e))?;

        let capture_settings = build_capture_settings(width, height, &settings);
        let output_path = capture_settings.output_path.clone();
        let output_path_for_encoder = output_path.clone();
        let target_fps = settings.target_fps;

        let perf_sampler = if let Some(ref stats) = perf_stats {
            let csv_path = output_path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join(format!(
                    "performance_{}.csv",
                    chrono::Local::now().format("%Y%m%d_%H%M%S")
                ));
            let csv_path_display = csv_path.display().to_string();
            match PerfSampler::new(stats.clone(), csv_path) {
                Ok(s) => {
                    log::info!(
                        "[Recorder] Performance sampling enabled: {}",
                        csv_path_display
                    );
                    Some(PerfSamplerHandle::new(s))
                }
                Err(e) => {
                    log::error!("[Recorder] Failed to create perf sampler: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let sampler_clone = perf_sampler.clone();

        let (frame_tx, _frame_rx): (
            channel::Sender<EncodedFrame>,
            channel::Receiver<EncodedFrame>,
        ) = channel::bounded(2);
        let (done_tx, _done_rx) = std::sync::mpsc::channel();
        let done_tx_for_setup = done_tx.clone();

        let stop_flag: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let done_flag: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let done_condvar = Arc::new(Condvar::new());
        let screenshot_request: Arc<Mutex<Option<ScreenshotRequest>>> = Arc::new(Mutex::new(None));
        let screenshot_done: Arc<Condvar> = Arc::new(Condvar::new());
        let screenshot_stopped: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        let screenshot_join_handle: Arc<Mutex<Option<std::thread::JoinHandle<()>>>> =
            Arc::new(Mutex::new(None));

        let video_encoder: Option<Arc<parking_lot::Mutex<Option<VideoEncoder>>>> = {
            let bitrate = settings.bitrate.unwrap_or(15_000_000);
            let video_settings = VideoSettingsBuilder::new(width, height)
                .bitrate(bitrate)
                .frame_rate(target_fps);

            match VideoEncoder::new(
                video_settings,
                AudioSettingsBuilder::default().disabled(true),
                ContainerSettingsBuilder::default(),
                &output_path_for_encoder,
            ) {
                Ok(enc) => {
                    log::info!(
                        "[Recorder] VideoEncoder created: {}x{} @ {} fps, bitrate={}",
                        width,
                        height,
                        target_fps,
                        bitrate
                    );
                    Some(Arc::new(parking_lot::Mutex::new(Some(enc))))
                }
                Err(e) => {
                    log::error!("[Recorder] Failed to create VideoEncoder: {}", e);
                    None
                }
            }
        };

        let control = {
            let mut cap_settings = build_capture_settings(width, height, &settings);
            cap_settings.frame_tx = Some(frame_tx.clone());
            cap_settings.done_tx = None;
            cap_settings.perf_stats = perf_stats.clone();
            cap_settings.video_encoder = video_encoder.clone();

            super::capture_adapter::start_capture(monitor, cap_settings)
                .map_err(|e| anyhow::anyhow!("Failed to start capture: {}", e))?
        };

        let callback = control.callback();
        let snapshot_buffer = {
            let mut handler = callback.lock();
            let mut setup_settings = build_capture_settings(width, height, &settings);
            setup_settings.frame_tx = Some(frame_tx);
            setup_settings.done_tx = Some(done_tx_for_setup);
            setup_settings.perf_stats = perf_stats.clone();
            setup_settings.video_encoder = video_encoder.clone();
            handler.setup(setup_settings);
            Arc::clone(&handler.state.snapshot_buffer)
        };

        let screenshot_request_clone = Arc::clone(&screenshot_request);
        let screenshot_stopped_clone = Arc::clone(&screenshot_stopped);
        let snapshot_clone = Arc::clone(&snapshot_buffer);

        let jh = std::thread::Builder::new()
            .name("screenshot".to_string())
            .spawn(move || loop {
                if screenshot_stopped_clone.load(Ordering::Relaxed) {
                    break;
                }

                let req = {
                    let mut guard = screenshot_request_clone.lock();
                    guard.take()
                };

                match req {
                    Some(req) => {
                        let pixels = {
                            let snap = snapshot_clone.lock();
                            snap.clone()
                        };

                        if pixels.is_empty() {
                            log::warn!("[Screenshot] snapshot buffer empty, dropping");
                            continue;
                        }

                        let start = Instant::now();
                        if let Err(e) = encode_screenshot_to_file(&req, &pixels, width, height) {
                            log::error!("Screenshot encoding failed: {}", e);
                        }
                        log::info!(
                            "[Screenshot] done, encoding {:.1}ms",
                            start.elapsed().as_secs_f64() * 1000.0
                        );
                    }
                    None => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            })
            .expect("Failed to spawn screenshot thread");
        *screenshot_join_handle.lock() = Some(jh);

        log::info!(
            "Recording monitor [{}] {}x{} @ {} fps...",
            settings.monitor_index,
            width,
            height,
            target_fps
        );

        let stop_flag_clone = Arc::clone(&stop_flag);
        let done_flag_clone = Arc::clone(&done_flag);
        let done_condvar_clone = Arc::clone(&done_condvar);
        let halt_handle = control.halt_handle();
        let ss_stopped = Arc::clone(&screenshot_stopped);
        let ss_done = Arc::clone(&screenshot_done);
        let ss_jh = Arc::clone(&screenshot_join_handle);
        let video_encoder_clone = video_encoder.clone();
        let callback_clone = callback.clone();

        let perf_stats_clone = perf_stats.clone();

        let join_handle = std::thread::spawn(move || {
            loop {
                if *stop_flag_clone.lock() {
                    callback_clone.lock().request_stop();
                    break;
                }

                if halt_handle.load(Ordering::Relaxed) {
                    break;
                }

                if let Some(ref sampler) = sampler_clone {
                    if let Err(e) = sampler.sample() {
                        log::warn!("[Recorder] Sampling failed: {}", e);
                    }
                }

                std::thread::sleep(Duration::from_millis(50));
            }

            ss_stopped.store(true, Ordering::Relaxed);
            ss_done.notify_one();
            if let Some(jh) = ss_jh.lock().take() {
                let _ = jh.join();
            }

            if let Some(ref enc_arc) = video_encoder_clone {
                if let Some(enc) = enc_arc.lock().take() {
                    log::info!("[Recorder] Finishing VideoEncoder...");
                    if let Err(e) = enc.finish() {
                        log::error!("[Recorder] VideoEncoder finish failed: {}", e);
                    } else {
                        log::info!("[Recorder] VideoEncoder finished successfully");
                    }
                }
            }

            *done_flag_clone.lock() = true;
            done_condvar_clone.notify_one();

            if let Some(sampler) = sampler_clone {
                if let Err(e) = sampler.finish() {
                    log::error!("[Recorder] Failed to write CSV: {}", e);
                }
            }

            if let Some(stats) = perf_stats_clone.as_ref() {
                let duration = stats.elapsed_secs();
                let frames = stats.frames_captured.load(Ordering::Relaxed);
                let avg_fps = stats.capture_fps();
                let avg_encode_fps = stats.encode_fps();

                log::info!("=== Recording Performance ===");
                log::info!("Duration: {}", format_duration(duration));
                log::info!("Target FPS: {}", target_fps);
                log::info!("Captured Frames: {}", frames);
                log::info!("Avg Capture FPS: {:.1}", avg_fps);
                log::info!("Avg Encode FPS: {:.1}", avg_encode_fps);
                log::info!("");
                log::info!("Capture Stage (avg / max):");
                log::info!(
                    "  - Allocation:   {:.3}ms / {:.3}ms",
                    stats.avg_alloc_ms() / 1000.0,
                    stats.max_alloc_ms() as f64 / 1000.0
                );
                log::info!(
                    "  - BGRA→RGBA:   {:.3}ms / {:.3}ms",
                    stats.avg_convert_ms() / 1000.0,
                    stats.max_convert_ms() as f64 / 1000.0
                );
                log::info!(
                    "  - Channel Send: {:.3}ms / {:.3}ms",
                    stats.avg_send_ms() / 1000.0,
                    stats.max_send_ms() as f64 / 1000.0
                );
                log::info!(
                    "  - Snapshot:     {:.3}ms / {:.3}ms",
                    stats.avg_snapshot_ms() / 1000.0,
                    stats.max_snapshot_ms() as f64 / 1000.0
                );
            }

            Ok(output_path)
        });

        Ok((
            Self {
                stop_flag,
                done_flag,
                done_condvar,
                control: Some(control),
                screenshot_request,
                screenshot_done,
                screenshot_stopped,
                screenshot_join_handle,
                perf_sampler: perf_sampler.clone(),
                video_encoder,
            },
            join_handle,
        ))
    }

    pub fn stop(&mut self) {
        *self.stop_flag.lock() = true;
        if let Some(c) = self.control.take() {
            let _ = c.stop();
        }
        loop {
            if *self.done_flag.lock() {
                break;
            }
            let guard = self.done_condvar.wait(&mut self.done_flag.lock());
            let _ = guard;
            if *self.done_flag.lock() {
                break;
            }
        }
    }

    pub fn take_screenshot(&self, request: ScreenshotRequest) {
        {
            let mut guard = self.screenshot_request.lock();
            *guard = Some(request);
        }
        self.screenshot_done.notify_one();
    }
}

impl Drop for RecorderHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn format_f64(v: f64, decimals: usize) -> String {
    format!("{:.1$}", v, decimals)
}

fn format_duration(secs: f64) -> String {
    let total_secs = secs as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}
