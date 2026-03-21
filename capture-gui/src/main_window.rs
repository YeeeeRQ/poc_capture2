use capture_core::MonitorInfo;
use eframe::egui;

use crate::app::CaptureApp;

pub fn show(
    ctx: &egui::Context,
    app: &mut CaptureApp,
    bg: egui::Color32,
    txt: egui::Color32,
    is_recording: bool,
    timer_text: String,
    monitors: Vec<MonitorInfo>,
    selected_monitor: usize,
) {
    let mut selected = selected_monitor;

    egui::CentralPanel::default()
        .frame(egui::Frame {
            fill: egui::Color32::TRANSPARENT,
            inner_margin: 0.0.into(),
            outer_margin: 0.0.into(),
            ..Default::default()
        })
        .show(ctx, |ui| {
            let rect = ui.max_rect();

            ui.painter().rect(
                rect,
                0.0,
                bg,
                egui::Stroke::new(0.0, egui::Color32::TRANSPARENT),
                egui::StrokeKind::Inside,
            );

            ui.allocate_ui(rect.size(), |ui| {
                let avail_h = ui.available_height();
                let toolbar_h = 44.0_f32;
                let top_space = ((avail_h - toolbar_h) / 2.0).max(0.0);

                ui.add_space(top_space);
                ui.set_min_height(toolbar_h);

                ui.vertical_centered(|ui| {
                    ui.set_min_height(toolbar_h);
                    ui.horizontal(|ui| {
                        ui.set_min_height(toolbar_h);
                        ui.spacing_mut().item_spacing.x = 6.0;

                        let record_icon = ui.selectable_label(
                            false,
                            egui::RichText::new(if is_recording { "⏺" } else { "📷" })
                                .color(if is_recording {
                                    egui::Color32::from_rgb(255, 100, 100)
                                } else {
                                    egui::Color32::from_rgb(100, 220, 120)
                                })
                                .size(16.0),
                        );

                        egui::ComboBox::from_id_salt("mon")
                            .selected_text({
                                if selected < monitors.len() {
                                    let m = &monitors[selected];
                                    format!("[{}] {}", m.index, m.name)
                                } else {
                                    "Unknown".to_string()
                                }
                            })
                            .width(160.0)
                            .show_ui(ui, |ui| {
                                for m in &monitors {
                                    ui.selectable_value(
                                        &mut selected,
                                        m.index,
                                        format!(
                                            "[{}] {} ({}x{})",
                                            m.index, m.name, m.width, m.height
                                        ),
                                    );
                                }
                            });

                        if selected != selected_monitor {
                            if let Some(tray_state) = crate::tray::get_tray_state() {
                                tray_state
                                    .selected_monitor
                                    .store(selected, std::sync::atomic::Ordering::SeqCst);
                            }
                        }

                        ui.label(
                            egui::RichText::new(timer_text)
                                .color(egui::Color32::WHITE)
                                .size(14.0)
                                .strong(),
                        );

                        let screenshot_btn = ui.add_sized(
                            [60.0, 28.0],
                            egui::Button::new(
                                egui::RichText::new(if app.screenshot_loading {
                                    "..."
                                } else {
                                    "截图"
                                })
                                .color(egui::Color32::WHITE)
                                .size(12.0),
                            )
                            .fill(egui::Color32::from_rgb(80, 200, 120)),
                        );
                        if screenshot_btn.clicked() {
                            log::info!("[MainWindow] Screenshot clicked");
                            if let Some(tray_state) = crate::tray::get_tray_state() {
                                tray_state
                                    .screenshot_flag
                                    .store(true, std::sync::atomic::Ordering::SeqCst);
                            }
                        }

                        let record_btn = ui.add_sized(
                            [56.0, 28.0],
                            egui::Button::new(
                                egui::RichText::new(if is_recording { "停止" } else { "录制" })
                                    .color(egui::Color32::WHITE)
                                    .size(12.0),
                            )
                            .fill(if is_recording {
                                egui::Color32::from_rgb(220, 70, 70)
                            } else {
                                egui::Color32::from_rgb(200, 50, 50)
                            }),
                        );
                        if record_btn.clicked() {
                            log::info!(
                                "[MainWindow] Record clicked, is_recording={}",
                                is_recording
                            );
                            if let Some(tray_state) = crate::tray::get_tray_state() {
                                tray_state
                                    .recording_toggle_flag
                                    .store(true, std::sync::atomic::Ordering::SeqCst);
                            }
                        }

                        let pin_bg = if app.is_pinned {
                            egui::Color32::from_rgb(60, 150, 255)
                        } else {
                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 12)
                        };
                        let pin_txt = if app.is_pinned {
                            egui::Color32::from_rgb(15, 30, 60)
                        } else {
                            txt
                        };

                        let pin_btn = ui.add_sized(
                            [28.0, 28.0],
                            egui::Button::new(egui::RichText::new("📌").size(14.0).color(pin_txt))
                                .fill(pin_bg),
                        );
                        if pin_btn.clicked() {
                            let new_pinned = !app.is_pinned;
                            log::info!(
                                "[MainWindow] Pin clicked, is_pinned: {} -> {}",
                                app.is_pinned,
                                new_pinned
                            );
                            app.is_pinned = new_pinned;
                            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                                if app.is_pinned {
                                    egui::WindowLevel::AlwaysOnTop
                                } else {
                                    egui::WindowLevel::Normal
                                },
                            ));
                        }

                        let settings_txt = txt;
                        let settings_btn = ui.add_sized(
                            [28.0, 28.0],
                            egui::Button::new(
                                egui::RichText::new("⚙").size(14.0).color(settings_txt),
                            )
                            .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 12)),
                        );
                        if settings_btn.clicked() {
                            log::info!("[MainWindow] Settings clicked");
                            app.settings_open
                                .store(true, std::sync::atomic::Ordering::SeqCst);
                        }

                        let close_btn = ui.add_sized(
                            [28.0, 28.0],
                            egui::Button::new(
                                egui::RichText::new("❌")
                                    .size(12.0)
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(220, 70, 70)),
                        );
                        if close_btn.clicked() {
                            log::info!(
                                "[MainWindow] Close clicked, saving window state and hiding"
                            );
                            if let Some(tray_state) = crate::tray::get_tray_state() {
                                let hwnd =
                                    tray_state.hwnd.load(std::sync::atomic::Ordering::SeqCst);
                                if hwnd != 0 {
                                    if let Some(state) =
                                        crate::windows_window::save_window_state(hwnd)
                                    {
                                        *tray_state.window_state.lock() = state;
                                    }
                                    crate::windows_window::hide_window(hwnd);
                                }
                            }
                        }

                        let drag_rect = record_icon.rect.expand(4.0);
                        let drag_resp =
                            ui.interact(drag_rect, egui::Id::new("drag"), egui::Sense::click());
                        if drag_resp.hovered() {
                            ctx.set_cursor_icon(egui::CursorIcon::Grab);
                        }
                        if drag_resp.is_pointer_button_down_on() {
                            log::info!("[MainWindow] Drag started");
                            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                    });
                });
            });
        });
}
