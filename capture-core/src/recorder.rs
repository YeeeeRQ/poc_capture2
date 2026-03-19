use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use parking_lot::Mutex;

use super::capture_adapter::{
    build_capture_settings, get_monitor, setup_dpi_awareness, CaptureHandler, RecordSettings,
};
use windows_capture::capture::CaptureControl;

pub struct RecorderHandle {
    stop_flag: Arc<Mutex<bool>>,
    control:
        Mutex<Option<CaptureControl<CaptureHandler, Box<dyn std::error::Error + Send + Sync>>>>,
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
        let target_fps = settings.target_fps;

        let control = super::capture_adapter::start_capture(monitor, capture_settings)
            .map_err(|e| anyhow::anyhow!("Failed to start capture: {}", e))?;

        let callback = control.callback();
        {
            let mut handler = callback.lock();
            handler.setup(build_capture_settings(width, height, &settings));
        }

        log::info!(
            "Recording monitor [{}] {}x{} @ {} fps...",
            settings.monitor_index,
            width,
            height,
            target_fps
        );

        let stop_flag: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let stop_flag_clone = Arc::clone(&stop_flag);
        let callback_clone = Arc::clone(&callback);
        let halt_handle = control.halt_handle();

        let join_handle = std::thread::spawn(move || {
            loop {
                if *stop_flag_clone.lock() {
                    callback_clone.lock().request_stop();
                    break;
                }

                if halt_handle.load(Ordering::Relaxed) {
                    break;
                }

                std::thread::sleep(Duration::from_millis(50));
            }

            Ok(output_path)
        });

        Ok((
            Self {
                stop_flag,
                control: Mutex::new(Some(control)),
            },
            join_handle,
        ))
    }

    pub fn stop(&self) {
        *self.stop_flag.lock() = true;
    }
}

impl Drop for RecorderHandle {
    fn drop(&mut self) {
        self.stop();
        if let Some(c) = self.control.lock().take() {
            let _ = c.stop();
        }
    }
}
