use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui;

use crate::main_window;
use crate::settings::Settings;
use crate::settings_window;

pub struct CaptureApp {
    pub is_pinned: bool,
    pub quit_flag: Arc<AtomicBool>,
    pub screenshot_loading: bool,
    screenshot_loading_pending: Arc<AtomicBool>,
    pub settings_open: Arc<AtomicBool>,
    settings_loaded: Option<Settings>,
    settings_was_open: bool,
    pub main_viewport_rect: Option<egui::Rect>,
    window_rounded: bool,
}

impl CaptureApp {
    pub fn new(quit_flag: Arc<AtomicBool>) -> Self {
        Self {
            is_pinned: true,
            quit_flag,
            screenshot_loading: false,
            screenshot_loading_pending: Arc::new(AtomicBool::new(false)),
            settings_open: Arc::new(AtomicBool::new(false)),
            settings_loaded: None,
            settings_was_open: false,
            main_viewport_rect: None,
            window_rounded: false,
        }
    }
}

impl eframe::App for CaptureApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let (is_recording, timer_text, monitors, selected_monitor) = if let Some(tray_state) =
            crate::tray::get_tray_state()
        {
            if tray_state.quit_flag.load(Ordering::SeqCst) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            if tray_state.take_show_window() {
                log::info!("[App] take_show_window=true, sending Visible(true)");
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(475.0, 48.0)));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }

            if tray_state.take_screenshot() {
                log::info!("[App] Screenshot requested from tray");
                tray_state.screenshot_flag.store(true, Ordering::SeqCst);
            }

            if tray_state.take_recording_toggle() {
                log::info!("[App] Recording toggle requested from tray");
                tray_state
                    .recording_toggle_flag
                    .store(true, Ordering::SeqCst);
            }

            let is_rec = tray_state.is_recording.load(Ordering::SeqCst);
            let timer = if is_rec {
                if let Some(start) = *tray_state.record_start.lock().unwrap() {
                    let s = start.elapsed().as_secs();
                    format!("{:02}:{:02}", (s / 60) % 60, s % 60)
                } else {
                    "00:00".to_string()
                }
            } else {
                "00:00".to_string()
            };
            let monitors = tray_state.monitors.lock().unwrap().clone();
            let selected = tray_state.selected_monitor.load(Ordering::SeqCst);

            (is_rec, timer, monitors, selected)
        } else {
            (false, "00:00".to_string(), Vec::new(), 0)
        };

        if self.quit_flag.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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

        let bg = if is_recording {
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

        main_window::show(
            ctx,
            self,
            bg,
            txt,
            is_recording,
            timer_text,
            monitors,
            selected_monitor,
        );

        if self.screenshot_loading_pending.load(Ordering::SeqCst) {
            self.screenshot_loading_pending
                .store(false, Ordering::SeqCst);
            self.screenshot_loading = false;
        }

        ctx.request_repaint_after(Duration::from_millis(100));
    }
}
