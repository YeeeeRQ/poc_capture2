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

use crate::settings::Settings;

const FORMAT_OPTIONS: &[&str] = &["jpg", "png", "bmp"];
const QUALITY_OPTIONS: &[u8] = &[20, 40, 60, 80, 100];

static SETTINGS_VIEWPORT_ID: std::sync::LazyLock<egui::ViewportId> =
    std::sync::LazyLock::new(|| egui::ViewportId::from_hash_of("settings_viewport"));

pub struct CaptureApp {
    monitors: Vec<MonitorInfo>,
    selected_monitor: usize,
    is_recording: bool,
    record_start: Option<Instant>,
    is_pinned: bool,
    handle: Mutex<Option<RecorderHandle>>,
    join_handle:
        Mutex<Option<std::thread::JoinHandle<std::result::Result<PathBuf, anyhow::Error>>>>,
    quit_flag: Arc<AtomicBool>,
    screenshot_loading: bool,
    screenshot_loading_pending: Arc<AtomicBool>,
    settings: Settings,
    settings_open: Arc<AtomicBool>,
    settings_loaded: Option<Settings>,
}

impl CaptureApp {
    pub fn new(quit_flag: Arc<AtomicBool>, settings: Settings) -> Self {
        let monitors = list_monitors().unwrap_or_default();

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
        }
    }

    fn screenshot(&mut self) {
        if self.selected_monitor >= self.monitors.len() {
            return;
        }
        let ext = self.settings.screenshot_format.clone();
        let path = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(format!(
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
                output_path: None,
                format: ext,
                quality: self.settings.screenshot_quality,
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

    fn toggle_recording(&mut self) {
        if self.is_recording {
            self.stop_recording();
        } else {
            self.start_recording();
        }
    }

    fn start_recording(&mut self) {
        if self.selected_monitor >= self.monitors.len() {
            return;
        }
        let settings = RecordSettings {
            monitor_index: self.selected_monitor,
            output_path: None,
            duration_secs: None,
            target_fps: self.settings.fps,
            preset: "medium".to_string(),
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

    fn stop_recording(&mut self) {
        if let Some(h) = self.handle.lock().take() {
            h.stop();
        }
        if let Some(jh) = self.join_handle.lock().take() {
            if let Ok(Err(e)) = jh.join() {
                log::error!("Recording error: {}", e);
            }
        }
        self.is_recording = false;
        self.record_start = None;
        log::info!("Recording stopped");
    }

    fn timer_text(&self) -> String {
        if let Some(start) = self.record_start {
            let s = start.elapsed().as_secs();
            format!("{:02}:{:02}", (s / 60) % 60, s % 60)
        } else {
            "00:00".to_string()
        }
    }
}

impl eframe::App for CaptureApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.quit_flag.load(Ordering::SeqCst) {
            self.stop_recording();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let bg = if self.is_recording {
            egui::Color32::from_rgb(180, 20, 20)
        } else {
            egui::Color32::from_rgb(30, 30, 42)
        };

        let txt = egui::Color32::from_rgb(215, 215, 225);

        if self.settings_open.load(Ordering::SeqCst) {
            if self.settings_loaded.is_none() {
                self.settings_loaded = Some(Settings::load());
            }
            let settings_open = Arc::clone(&self.settings_open);
            let clear_cache = Arc::new(AtomicBool::new(false));
            let clear_cache_clone = Arc::clone(&clear_cache);
            let local_settings =
                Arc::new(Mutex::new(self.settings_loaded.as_ref().unwrap().clone()));
            let local_settings_for_sync = Arc::clone(&local_settings);

            ctx.show_viewport_deferred(
                *SETTINGS_VIEWPORT_ID,
                egui::ViewportBuilder::default()
                    .with_title("设置")
                    .with_inner_size([260.0, 300.0])
                    .with_decorations(false)
                    .with_resizable(false),
                move |ctx, _class| {
                    let bg = egui::Color32::from_rgb(30, 30, 42);
                    let label_col = egui::Color32::from_rgb(180, 180, 190);
                    let settings = Arc::clone(&local_settings_for_sync);

                    egui::CentralPanel::default()
                        .frame(
                            egui::Frame::default()
                                .fill(bg)
                                .inner_margin(egui::Margin::same(12)),
                        )
                        .show(ctx, |ui| {
                            ui.set_min_height(220.0);

                            let drag_rect = ui.min_rect();
                            let drag_resp = ui.interact(
                                drag_rect,
                                egui::Id::new("settings_drag"),
                                egui::Sense::click(),
                            );
                            if drag_resp.hovered() {
                                ctx.set_cursor_icon(egui::CursorIcon::Grab);
                            }
                            if drag_resp.is_pointer_button_down_on() {
                                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                            }

                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("设置")
                                        .color(egui::Color32::WHITE)
                                        .size(16.0)
                                        .strong(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add_sized(
                                                [24.0, 24.0],
                                                egui::Button::new(
                                                    egui::RichText::new("×").size(18.0).color(
                                                        egui::Color32::from_rgb(180, 180, 190),
                                                    ),
                                                ),
                                            )
                                            .clicked()
                                        {
                                            settings_open.store(false, Ordering::SeqCst);
                                            clear_cache_clone.store(true, Ordering::SeqCst);
                                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                        }
                                    },
                                );
                            });

                            ui.add_space(8.0);

                            ui.label(egui::RichText::new("帧率").color(label_col).size(12.0));

                            {
                                let mut s = settings.lock();
                                let fps_label = if s.fps == 15 {
                                    "15 fps"
                                } else if s.fps == 30 {
                                    "30 fps"
                                } else {
                                    "60 fps"
                                };
                                egui::ComboBox::from_id_salt("fps")
                                    .selected_text(fps_label)
                                    .width(120.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut s.fps, 15u32, "15 fps");
                                        ui.selectable_value(&mut s.fps, 30u32, "30 fps");
                                        ui.selectable_value(&mut s.fps, 60u32, "60 fps");
                                    });
                            }

                            ui.add_space(8.0);

                            ui.label(egui::RichText::new("截图格式").color(label_col).size(12.0));

                            {
                                let mut s = settings.lock();
                                let mut current_fmt = s.screenshot_format.clone();
                                egui::ComboBox::from_id_salt("fmt")
                                    .selected_text(current_fmt.to_uppercase())
                                    .width(120.0)
                                    .show_ui(ui, |ui| {
                                        for &fmt in FORMAT_OPTIONS {
                                            ui.radio_value(
                                                &mut current_fmt,
                                                fmt.to_string(),
                                                fmt.to_uppercase(),
                                            );
                                        }
                                    });
                                if current_fmt != s.screenshot_format {
                                    s.screenshot_format = current_fmt;
                                }
                            }

                            ui.add_space(8.0);

                            ui.label(egui::RichText::new("截图品质").color(label_col).size(12.0));
                            {
                                let mut s = settings.lock();
                                egui::ComboBox::from_id_salt("q")
                                    .selected_text(format!("Q{}", s.screenshot_quality))
                                    .width(120.0)
                                    .show_ui(ui, |ui| {
                                        for &q in QUALITY_OPTIONS {
                                            ui.selectable_value(
                                                &mut s.screenshot_quality,
                                                q,
                                                format!("Q{}", q),
                                            );
                                        }
                                    });
                            }

                            ui.add_space(16.0);

                            ui.horizontal(|ui| {
                                if ui
                                    .add_sized(
                                        [100.0, 32.0],
                                        egui::Button::new(
                                            egui::RichText::new("取消")
                                                .color(egui::Color32::from_rgb(180, 180, 190))
                                                .size(13.0),
                                        )
                                        .fill(
                                            egui::Color32::from_rgba_unmultiplied(
                                                255, 255, 255, 12,
                                            ),
                                        ),
                                    )
                                    .clicked()
                                {
                                    settings_open.store(false, Ordering::SeqCst);
                                    clear_cache_clone.store(true, Ordering::SeqCst);
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }

                                ui.add_space(6.0);

                                if ui
                                    .add_sized(
                                        [130.0, 32.0],
                                        egui::Button::new(
                                            egui::RichText::new("保存并退出")
                                                .color(egui::Color32::BLACK)
                                                .size(13.0)
                                                .strong(),
                                        )
                                        .fill(egui::Color32::from_rgb(80, 200, 120)),
                                    )
                                    .clicked()
                                {
                                    let s = settings.lock();
                                    if let Err(e) = s.save() {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                    drop(s);
                                    settings_open.store(false, Ordering::SeqCst);
                                    clear_cache_clone.store(true, Ordering::SeqCst);
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                            });
                        });

                    if ctx.input(|i| i.viewport().close_requested()) {
                        settings_open.store(false, Ordering::SeqCst);
                        clear_cache_clone.store(true, Ordering::SeqCst);
                    }
                },
            );

            if clear_cache.load(Ordering::SeqCst) {
                self.settings_loaded = None;
            }
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(bg)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| {
                ui.set_min_height(44.0);
                ui.horizontal(|ui| {
                    ui.set_min_height(44.0);
                    ui.spacing_mut().item_spacing.x = 6.0;

                    let drag_rect = ui.min_rect();
                    let drag_response =
                        ui.interact(drag_rect, egui::Id::new("drag"), egui::Sense::click());
                    if drag_response.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::Grab);
                    }
                    if drag_response.is_pointer_button_down_on() {
                        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }

                    let _ = ui.selectable_label(
                        false,
                        egui::RichText::new(if self.is_recording { "⏺" } else { "📷" })
                            .color(if self.is_recording {
                                egui::Color32::from_rgb(255, 100, 100)
                            } else {
                                egui::Color32::from_rgb(100, 220, 120)
                            })
                            .size(16.0),
                    );

                    egui::ComboBox::from_id_salt("mon")
                        .selected_text({
                            if self.selected_monitor < self.monitors.len() {
                                let m = &self.monitors[self.selected_monitor];
                                format!("[{}] {}", m.index, m.name)
                            } else {
                                "Unknown".to_string()
                            }
                        })
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for m in &self.monitors {
                                ui.selectable_value(
                                    &mut self.selected_monitor,
                                    m.index,
                                    format!("[{}] {} ({}x{})", m.index, m.name, m.width, m.height),
                                );
                            }
                        });

                    if self.is_recording {
                        ui.label(
                            egui::RichText::new(self.timer_text())
                                .color(egui::Color32::WHITE)
                                .size(14.0)
                                .strong(),
                        );
                    }

                    if ui
                        .add_sized(
                            [60.0, 28.0],
                            egui::Button::new(
                                egui::RichText::new(if self.screenshot_loading {
                                    "..."
                                } else {
                                    "截图"
                                })
                                .color(egui::Color32::WHITE)
                                .size(12.0),
                            )
                            .fill(egui::Color32::from_rgb(80, 200, 120)),
                        )
                        .clicked()
                    {
                        self.screenshot();
                    }

                    if ui
                        .add_sized(
                            [56.0, 28.0],
                            egui::Button::new(
                                egui::RichText::new(if self.is_recording {
                                    "停止"
                                } else {
                                    "录制"
                                })
                                .color(egui::Color32::WHITE)
                                .size(12.0),
                            )
                            .fill(if self.is_recording {
                                egui::Color32::from_rgb(220, 70, 70)
                            } else {
                                egui::Color32::from_rgb(200, 50, 50)
                            }),
                        )
                        .clicked()
                    {
                        self.toggle_recording();
                    }

                    let pin_bg = if self.is_pinned {
                        egui::Color32::from_rgb(60, 150, 255)
                    } else {
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 12)
                    };
                    let pin_txt = if self.is_pinned {
                        egui::Color32::from_rgb(15, 30, 60)
                    } else {
                        txt
                    };

                    if ui
                        .add_sized(
                            [28.0, 28.0],
                            egui::Button::new(
                                egui::RichText::new(if self.is_pinned { "📌" } else { "📍" })
                                    .size(14.0)
                                    .color(pin_txt),
                            )
                            .fill(pin_bg),
                        )
                        .clicked()
                    {
                        self.is_pinned = !self.is_pinned;
                        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                            if self.is_pinned {
                                egui::WindowLevel::AlwaysOnTop
                            } else {
                                egui::WindowLevel::Normal
                            },
                        ));
                    }

                    let settings_txt = txt;

                    if ui
                        .add_sized(
                            [28.0, 28.0],
                            egui::Button::new(
                                egui::RichText::new("⚙").size(14.0).color(settings_txt),
                            )
                            .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 12)),
                        )
                        .clicked()
                    {
                        self.settings_open.store(true, Ordering::SeqCst);
                    }

                    if ui
                        .add_sized(
                            [28.0, 28.0],
                            egui::Button::new(
                                egui::RichText::new("❌")
                                    .size(12.0)
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(220, 70, 70)),
                        )
                        .clicked()
                    {
                        self.quit_flag.store(true, Ordering::SeqCst);
                        self.stop_recording();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });

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
