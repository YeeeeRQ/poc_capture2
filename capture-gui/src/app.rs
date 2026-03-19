use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use parking_lot::Mutex;

use capture_core::{
    list_monitors, take_screenshot, RecordSettings, RecorderHandle, ScreenshotSettings,
};

pub struct CaptureApp {
    monitors: Vec<(usize, u32, u32)>,
    selected_monitor: usize,
    is_recording: bool,
    record_start: Option<Instant>,
    fps: u32,
    handle: Mutex<Option<RecorderHandle>>,
    join_handle:
        Mutex<Option<std::thread::JoinHandle<std::result::Result<PathBuf, anyhow::Error>>>>,
    quit_flag: Arc<AtomicBool>,
}

impl CaptureApp {
    pub fn new(quit_flag: Arc<AtomicBool>) -> Self {
        let monitors = list_monitors()
            .map(|v| {
                v.into_iter()
                    .map(|m| (m.index, m.width, m.height))
                    .collect()
            })
            .unwrap_or_default();

        Self {
            monitors,
            selected_monitor: 0,
            is_recording: false,
            record_start: None,
            fps: 30,
            handle: Mutex::new(None),
            join_handle: Mutex::new(None),
            quit_flag,
        }
    }

    fn screenshot(&self) {
        if self.selected_monitor >= self.monitors.len() {
            return;
        }
        let settings = ScreenshotSettings {
            monitor_index: self.selected_monitor,
            output_path: None,
            format: "png".to_string(),
            quality: 90,
        };
        match take_screenshot(&settings) {
            Ok(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                log::info!("Screenshot saved: {}", name);
            }
            Err(e) => {
                log::error!("Screenshot failed: {}", e);
            }
        }
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
            fps: self.fps,
            duration_secs: None,
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

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(bg)
                    .inner_margin(egui::Margin::same(0.0)),
            )
            .show(ctx, |ui| {
                ui.set_min_height(44.0);
                ui.horizontal(|ui| {
                    ui.set_min_height(44.0);
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.add_space(10.0);

                    ui.label(
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
                                let (_, w, h) = &self.monitors[self.selected_monitor];
                                format!("{w}x{h}")
                            } else {
                                "Unknown".to_string()
                            }
                        })
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for (i, (_, w, h)) in self.monitors.iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.selected_monitor,
                                    i,
                                    format!("{w}x{h}"),
                                );
                            }
                        });

                    if ui
                        .add_sized(
                            [60.0, 28.0],
                            egui::Button::new(
                                egui::RichText::new("截图")
                                    .color(egui::Color32::BLACK)
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

                    if self.is_recording {
                        ui.label(
                            egui::RichText::new(self.timer_text())
                                .color(egui::Color32::WHITE)
                                .size(14.0)
                                .strong(),
                        );
                    } else {
                        ui.add_sized(
                            [88.0, 24.0],
                            egui::DragValue::new(&mut self.fps)
                                .range(1..=120)
                                .speed(1.0)
                                .suffix(" fps"),
                        );
                    }

                    if ui
                        .add_sized(
                            [28.0, 28.0],
                            egui::Button::new(egui::RichText::new("✕").size(14.0).color(txt)),
                        )
                        .clicked()
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    }

                    ui.add_space(10.0);
                });
            });

        if self.is_recording {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}
