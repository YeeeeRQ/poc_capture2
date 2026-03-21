use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Local;
use parking_lot::RwLock;

use capture_core::{
    take_screenshot, PerfStats, RecordSettings, RecorderHandle, ScreenshotSettings,
};

use crate::settings::Settings;

pub struct WorkerState {
    pub session_folder: PathBuf,
    pub session_has_content: Arc<AtomicBool>,
    pub settings: Arc<RwLock<Settings>>,
    pub perf_stats: Option<Arc<PerfStats>>,
}

impl WorkerState {
    pub fn new(
        session_folder: PathBuf,
        settings: Arc<RwLock<Settings>>,
        perf_stats: Option<Arc<PerfStats>>,
    ) -> Self {
        Self {
            session_folder,
            session_has_content: Arc::new(AtomicBool::new(false)),
            settings,
            perf_stats,
        }
    }
}

pub fn spawn_worker(state: WorkerState) {
    let session_folder = state.session_folder.clone();
    let session_has_content = Arc::clone(&state.session_has_content);
    let settings = state.settings;
    let perf_stats = state.perf_stats;

    std::thread::spawn(move || {
        log::info!("[Worker] Worker thread started");

        loop {
            if let Some(tray_state) = crate::tray::get_tray_state() {
                if tray_state.quit_flag.load(Ordering::SeqCst) {
                    log::info!("[Worker] Quit flag detected");

                    if tray_state.is_recording.load(Ordering::SeqCst) {
                        log::info!("[Worker] Stopping recording before quit");
                        if let Some(mut handle) = tray_state.handle.lock().take() {
                            handle.stop();
                        }
                        tray_state.is_recording.store(false, Ordering::SeqCst);
                    }

                    if !session_has_content.load(Ordering::SeqCst) {
                        if session_folder.exists() {
                            if let Ok(entries) = std::fs::read_dir(&session_folder) {
                                if entries.count() == 0 {
                                    if std::fs::remove_dir(&session_folder).is_ok() {
                                        log::info!(
                                            "[Worker] Removed empty session folder: {}",
                                            session_folder.display()
                                        );
                                    }
                                }
                            }
                        }
                    }
                    log::info!("[Worker] Exiting process");
                    std::process::exit(0);
                }

                if tray_state.show_window_flag.swap(false, Ordering::SeqCst) {
                    log::info!("[Worker] Show window requested");
                    let hwnd = tray_state.hwnd.load(Ordering::SeqCst);
                    if hwnd != 0 {
                        let state = tray_state.window_state.lock().clone();
                        crate::windows_window::show_window(hwnd, &state);
                    } else {
                        log::warn!("[Worker] HWND not yet available");
                    }
                }

                let is_recording = tray_state.is_recording.load(Ordering::SeqCst);

                if !is_recording {
                    if tray_state.screenshot_flag.swap(false, Ordering::SeqCst) {
                        log::info!("[Worker] Screenshot requested");
                        let monitors = tray_state.monitors.lock();
                        let selected = tray_state.selected_monitor.load(Ordering::SeqCst);
                        if selected >= monitors.len() {
                            log::warn!("[Worker] Invalid monitor selected");
                        } else {
                            let s = settings.read();
                            let ext = s.screenshot_format.clone();
                            let path = session_folder.join(format!(
                                "screenshot_{}.{}",
                                Local::now().format("%Y%m%d_%H%M%S"),
                                ext
                            ));

                            match take_screenshot(&ScreenshotSettings {
                                monitor_index: selected,
                                output_path: Some(path.clone()),
                                format: ext,
                                quality: s.screenshot_quality,
                                draw_border: s.draw_border,
                            }) {
                                Ok(p) => {
                                    session_has_content.store(true, Ordering::SeqCst);
                                    log::info!("[Worker] Screenshot saved: {}", p.display());
                                }
                                Err(e) => log::error!("[Worker] Screenshot failed: {}", e),
                            }
                        }
                    }

                    if tray_state
                        .recording_toggle_flag
                        .swap(false, Ordering::SeqCst)
                    {
                        log::info!(
                            "[Worker] Recording toggle detected at {:?}",
                            std::time::Instant::now()
                        );
                        let monitors = tray_state.monitors.lock();
                        let selected = tray_state.selected_monitor.load(Ordering::SeqCst);
                        if selected >= monitors.len() {
                            log::warn!("[Worker] Invalid monitor selected");
                        } else {
                            let s = settings.read();
                            let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
                            let output_path =
                                session_folder.join(format!("capture_{}.mp4", timestamp));

                            let record_settings = RecordSettings {
                                monitor_index: selected,
                                output_path: Some(output_path),
                                duration_secs: None,
                                target_fps: s.fps,
                                preset: "medium".to_string(),
                                draw_border: s.draw_border,
                            };

                            match RecorderHandle::start(record_settings, perf_stats.clone()) {
                                Ok((handle, _jh)) => {
                                    *tray_state.handle.lock() = Some(handle);
                                    tray_state.is_recording.store(true, Ordering::SeqCst);
                                    *tray_state.record_start.lock() = Some(Instant::now());
                                    session_has_content.store(true, Ordering::SeqCst);
                                    crate::tray::send_menu_update(
                                        crate::tray::TrayMenuUpdate::RecordingStarted,
                                    );
                                    log::info!(
                                        "[Worker] Menu update sent (start) at {:?}",
                                        std::time::Instant::now()
                                    );
                                    log::info!("[Worker] Recording started");
                                }
                                Err(e) => {
                                    log::error!("[Worker] Failed to start recording: {}", e);
                                }
                            }
                        }
                    }
                } else {
                    if tray_state
                        .recording_toggle_flag
                        .swap(false, Ordering::SeqCst)
                    {
                        log::info!("[Worker] Stopping recording");
                        if let Some(mut handle) = tray_state.handle.lock().take() {
                            handle.stop();
                        }
                        tray_state.is_recording.store(false, Ordering::SeqCst);
                        tray_state.record_start.lock().take();
                        crate::tray::send_menu_update(
                            crate::tray::TrayMenuUpdate::RecordingStopped,
                        );
                        log::info!(
                            "[Worker] Menu update sent (stop) at {:?}",
                            std::time::Instant::now()
                        );
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(10));
        }
    });

    log::info!("[Worker] Worker thread spawned");
}
