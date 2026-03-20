use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Local;
use eframe::egui;
use parking_lot::Mutex;

use capture_core::{
    list_monitors, take_screenshot, MonitorInfo, RecordSettings, RecorderHandle, ScreenshotRequest,
    ScreenshotSettings,
};

use crate::main_window;
use crate::settings::Settings;
use crate::settings_window;

pub struct CaptureApp {
    pub monitors: Vec<MonitorInfo>,
    pub selected_monitor: usize,
    pub is_recording: bool,
    record_start: Option<Instant>,
    pub is_pinned: bool,
    handle: Mutex<Option<RecorderHandle>>,
    join_handle:
        Mutex<Option<std::thread::JoinHandle<std::result::Result<PathBuf, anyhow::Error>>>>,
    pub quit_flag: Arc<AtomicBool>,
    pub screenshot_loading: bool,
    screenshot_loading_pending: Arc<AtomicBool>,
    settings: Settings,
    pub settings_open: Arc<AtomicBool>,
    settings_loaded: Option<Settings>,
    settings_was_open: bool,
    pub main_viewport_rect: Option<egui::Rect>,
    window_rounded: bool,
    session_folder: PathBuf,
}

impl CaptureApp {
    pub fn new(quit_flag: Arc<AtomicBool>, settings: Settings) -> Self {
        let monitors = list_monitors().unwrap_or_default();

        let session_folder = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(format!("capture_{}", Local::now().format("%Y%m%d_%H%M%S")));

        if let Err(e) = std::fs::create_dir_all(&session_folder) {
            log::error!("Failed to create session folder: {}", e);
        } else {
            log::info!("Session folder: {}", session_folder.display());
        }

        Self {
            monitors,
            selected_monitor: 0,
            is_recording: false,
            record_start: None,
            is_pinned: true,
            handle: Mutex::new(None),
            join_handle: Mutex::new(None),
            quit_flag,
            screenshot_loading: false,
            screenshot_loading_pending: Arc::new(AtomicBool::new(false)),
            settings,
            settings_open: Arc::new(AtomicBool::new(false)),
            settings_loaded: None,
            settings_was_open: false,
            main_viewport_rect: None,
            window_rounded: false,
            session_folder,
        }
    }

    pub fn screenshot(&mut self) {
        if self.selected_monitor >= self.monitors.len() {
            return;
        }
        let ext = self.settings.screenshot_format.clone();
        let path = self.session_folder.join(format!(
            "screenshot_{}.{}",
            Local::now().format("%Y%m%d_%H%M%S"),
            ext
        ));
        let request = ScreenshotRequest {
            path: path.clone(),
            format: ext.clone(),
            quality: self.settings.screenshot_quality,
        };
        self.screenshot_loading = true;
        if self.is_recording {
            if let Some(ref handle) = *self.handle.lock() {
                handle.take_screenshot(request);
            }
        } else {
            match take_screenshot(&ScreenshotSettings {
                monitor_index: self.selected_monitor,
                output_path: Some(path),
                format: ext,
                quality: self.settings.screenshot_quality,
                draw_border: self.settings.draw_border,
            }) {
                Ok(p) => log::info!("Screenshot saved: {}", p.display()),
                Err(e) => log::error!("Screenshot failed: {}", e),
            }
        }
        let pending = Arc::clone(&self.screenshot_loading_pending);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(800));
            pending.store(true, Ordering::SeqCst);
        });
    }

    pub fn toggle_recording(&mut self) {
        if self.is_recording {
            self.stop_recording();
        } else {
            self.start_recording();
        }
    }

    pub fn start_recording(&mut self) {
        if self.selected_monitor >= self.monitors.len() {
            return;
        }

        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let output_path = self
            .session_folder
            .join(format!("capture_{}.mp4", timestamp));

        let settings = RecordSettings {
            monitor_index: self.selected_monitor,
            output_path: Some(output_path),
            duration_secs: None,
            target_fps: self.settings.fps,
            preset: "medium".to_string(),
            draw_border: self.settings.draw_border,
        };

        match RecorderHandle::start(settings) {
            Ok((handle, jh)) => {
                *self.handle.lock() = Some(handle);
                *self.join_handle.lock() = Some(jh);
                self.is_recording = true;
                self.record_start = Some(Instant::now());
                log::info!("Recording started");
            }
            Err(e) => {
                log::error!("Failed to start recording: {}", e);
            }
        }
    }

    pub fn stop_recording(&mut self) {
        log::info!("[App] stop_recording called");
        if let Some(mut h) = self.handle.lock().take() {
            h.stop();
        }
        std::thread::sleep(Duration::from_millis(500));
        if let Some(jh) = self.join_handle.lock().take() {
            if let Ok(Err(e)) = jh.join() {
                log::error!("Recording error: {}", e);
            }
        }
        self.is_recording = false;
        self.record_start = None;
        log::info!("[App] stop_recording finished");
    }

    pub fn timer_text(&self) -> String {
        if let Some(start) = self.record_start {
            let s = start.elapsed().as_secs();
            format!("{:02}:{:02}", (s / 60) % 60, s % 60)
        } else {
            "00:00".to_string()
        }
    }
}

impl eframe::App for CaptureApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(tray_state) = crate::tray::get_tray_state() {
            if tray_state.quit_flag.load(Ordering::SeqCst) {
                self.stop_recording();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                std::process::exit(0);
            }

            if tray_state.take_show_window() {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(475.0, 48.0)));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }

            if tray_state.take_screenshot() {
                self.screenshot();
            }

            if tray_state.take_recording_toggle() {
                self.toggle_recording();
            }
        }

        if self.quit_flag.load(Ordering::SeqCst) {
            self.stop_recording();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            std::process::exit(0);
        }

        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            self.main_viewport_rect = Some(rect);

            #[cfg(windows)]
            {
                if !self.window_rounded {
                    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                    if let Ok(handle) = frame.window_handle() {
                        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
                            let hwnd = win32_handle.hwnd.get() as isize;
                            let scale_factor =
                                ctx.input(|i| i.viewport().native_pixels_per_point.unwrap_or(1.0));
                            let width = ((rect.max.x - rect.min.x) * scale_factor) as i32;
                            let height = ((rect.max.y - rect.min.y) * scale_factor) as i32;
                            if crate::windows_window::set_window_rounded_corner(
                                hwnd, width, height, 8,
                            ) {
                                self.window_rounded = true;
                            }
                        }
                    }
                }
            }
        }

        let bg = if self.is_recording {
            egui::Color32::from_rgb(180, 20, 20)
        } else {
            egui::Color32::from_rgb(30, 30, 42)
        };
        let txt = egui::Color32::from_rgb(215, 215, 225);

        if self.settings_was_open && !self.settings_open.load(Ordering::SeqCst) {
            self.settings_loaded = None;
        }
        self.settings_was_open = self.settings_open.load(Ordering::SeqCst);

        if self.settings_open.load(Ordering::SeqCst) && self.settings_loaded.is_none() {
            self.settings_loaded = Some(Settings::load());
        }

        settings_window::show(
            ctx,
            &self.settings_open,
            &self.settings_loaded,
            &self.main_viewport_rect,
        );

        main_window::show(ctx, self, bg, txt);

        if self.screenshot_loading_pending.load(Ordering::SeqCst) {
            self.screenshot_loading_pending
                .store(false, Ordering::SeqCst);
            self.screenshot_loading = false;
        }

        if self.is_recording {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}
