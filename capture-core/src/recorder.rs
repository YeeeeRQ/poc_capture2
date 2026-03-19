use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossbeam::channel;
use parking_lot::{Condvar, Mutex};

use super::capture_adapter::{
    build_capture_settings, get_monitor, setup_dpi_awareness, EncodedFrame, RecordSettings,
};

pub struct RecorderHandle {
    stop_flag: Arc<Mutex<bool>>,
    done_flag: Arc<Mutex<bool>>,
    done_condvar: Arc<Condvar>,
    done_tx: std::sync::mpsc::Sender<()>,
}

impl RecorderHandle {
    pub fn start(
        settings: RecordSettings,
    ) -> Result<(Self, std::thread::JoinHandle<Result<std::path::PathBuf>>)> {
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
        let preset = settings.preset.clone();

        let (frame_tx, frame_rx): (
            channel::Sender<EncodedFrame>,
            channel::Receiver<EncodedFrame>,
        ) = channel::bounded(2);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let done_tx_for_encoder = done_tx.clone();
        let done_tx_for_setup = done_tx.clone();

        let stop_flag: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let done_flag: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let done_condvar = Arc::new(Condvar::new());

        let encoder_join = {
            let stop_flag = Arc::clone(&stop_flag);
            std::thread::Builder::new()
                .name("encoder".to_string())
                .spawn(move || {
                    let mut enc = match crate::encoder::FfmpegEncoder::new(
                        width,
                        height,
                        &output_path_for_encoder,
                        &preset,
                        target_fps,
                    ) {
                        Ok(e) => e,
                        Err(e) => {
                            log::error!("Failed to create encoder: {}", e);
                            let _ = done_tx_for_encoder.send(());
                            return;
                        }
                    };

                    let start_time = Instant::now();
                    let mut last_log = Duration::ZERO;

                    loop {
                        match frame_rx.recv_timeout(Duration::from_millis(100)) {
                            Ok(frame) => {
                                if enc.write_frame(&frame.data).is_err() {
                                    break;
                                }
                                let elapsed = start_time.elapsed();
                                if elapsed.saturating_sub(last_log) >= Duration::from_secs(1) {
                                    last_log = elapsed;
                                    log::info!(
                                        "[{}s] encoder fps={:.1}",
                                        format_f64(elapsed.as_secs_f64(), 1),
                                        enc.frames_written() as f64
                                            / elapsed.as_secs_f64().max(0.001)
                                    );
                                }
                            }
                            Err(channel::RecvTimeoutError::Timeout) => {
                                if done_rx.try_recv().is_ok() || *stop_flag.lock() {
                                    let _ = enc.finish();
                                    break;
                                }
                            }
                            Err(channel::RecvTimeoutError::Disconnected) => {
                                let _ = enc.finish();
                                break;
                            }
                        }
                    }
                    let _ = done_tx_for_encoder.send(());
                })
                .context("Failed to spawn encoder thread")?
        };

        let stop_flag_clone = Arc::clone(&stop_flag);
        let done_flag_clone = Arc::clone(&done_flag);
        let done_condvar_clone = Arc::clone(&done_condvar);

        let control = {
            let mut cap_settings = build_capture_settings(width, height, &settings);
            cap_settings.frame_tx = Some(frame_tx.clone());
            cap_settings.done_tx = None;

            super::capture_adapter::start_capture(monitor, cap_settings)
                .map_err(|e| anyhow::anyhow!("Failed to start capture: {}", e))?
        };

        let callback = control.callback();
        {
            let mut handler = callback.lock();
            let mut setup_settings = build_capture_settings(width, height, &settings);
            setup_settings.frame_tx = Some(frame_tx);
            setup_settings.done_tx = Some(done_tx_for_setup);
            handler.setup(setup_settings);
        }

        log::info!(
            "Recording monitor [{}] {}x{} @ {} fps...",
            settings.monitor_index,
            width,
            height,
            target_fps
        );

        let join_handle = std::thread::spawn(move || {
            loop {
                if *stop_flag_clone.lock() {
                    callback.lock().request_stop();
                    break;
                }

                if control.halt_handle().load(Ordering::Relaxed) {
                    break;
                }

                std::thread::sleep(Duration::from_millis(50));
            }

            let _ = encoder_join.join();
            *done_flag_clone.lock() = true;
            done_condvar_clone.notify_one();
            Ok(output_path)
        });

        Ok((
            Self {
                stop_flag,
                done_flag,
                done_condvar,
                done_tx,
            },
            join_handle,
        ))
    }

    pub fn stop(&self) {
        *self.stop_flag.lock() = true;
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
}

impl Drop for RecorderHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn format_f64(v: f64, decimals: usize) -> String {
    format!("{:.1$}", v, decimals)
}
