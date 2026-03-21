use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use eframe::egui;
use parking_lot::RwLock;

use crate::settings::{Settings, FORMAT_OPTIONS, QUALITY_OPTIONS};

static SETTINGS_VIEWPORT_ID: std::sync::LazyLock<egui::ViewportId> =
    std::sync::LazyLock::new(|| egui::ViewportId::from_hash_of("settings_viewport"));

static SETTINGS_ROUNDED: std::sync::LazyLock<Arc<AtomicBool>> =
    std::sync::LazyLock::new(|| Arc::new(AtomicBool::new(false)));

pub fn show(
    ctx: &egui::Context,
    settings_open: &Arc<AtomicBool>,
    settings_loaded: &Option<Settings>,
    main_viewport_rect: &Option<egui::Rect>,
    shared_settings: &Arc<RwLock<Settings>>,
) {
    if !settings_open.load(Ordering::SeqCst) {
        return;
    }

    let settings_loaded = match settings_loaded {
        Some(s) => s,
        None => return,
    };

    let settings_h = 340.0_f32;
    let settings_w = 260.0_f32;
    let gap = 10.0_f32;

    let settings_pos = if let Some(rect) = main_viewport_rect {
        let center_x = rect.center().x;
        egui::Pos2::new(center_x - settings_w / 2.0, rect.min.y - settings_h - gap)
    } else {
        egui::Pos2::new(100.0, 100.0)
    };

    let settings_open = Arc::clone(settings_open);
    let local_settings = Arc::new(parking_lot::Mutex::new(settings_loaded.clone()));
    let local_settings_for_sync = Arc::clone(&local_settings);
    let shared_settings = Arc::clone(shared_settings);

    ctx.show_viewport_deferred(
        *SETTINGS_VIEWPORT_ID,
        egui::ViewportBuilder::default()
            .with_title("设置")
            .with_inner_size([settings_w, settings_h])
            .with_position(settings_pos)
            .with_decorations(false)
            .with_resizable(false),
        move |ctx, _class| {
            let bg = egui::Color32::from_rgb(30, 30, 42);
            let label_col = egui::Color32::from_rgb(180, 180, 190);
            let header_col = egui::Color32::from_rgb(100, 220, 120);
            let separator_col = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20);
            let settings = Arc::clone(&local_settings_for_sync);

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::default()
                        .fill(bg)
                        .inner_margin(egui::Margin::same(16)),
                )
                .show(ctx, |ui| {
                    ui.set_min_height(320.0);

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

                    ui.label(
                        egui::RichText::new("设置")
                            .color(egui::Color32::WHITE)
                            .size(18.0)
                            .strong(),
                    );

                    ui.add_space(16.0);

                    ui.label(
                        egui::RichText::new("📷 截图设置")
                            .color(header_col)
                            .size(14.0)
                            .strong(),
                    );

                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("格式").color(label_col).size(13.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let mut s = settings.lock();
                            let mut current_fmt = s.screenshot_format.clone();
                            egui::ComboBox::from_id_salt("fmt")
                                .selected_text(current_fmt.to_uppercase())
                                .width(100.0)
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
                        });
                    });

                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("品质").color(label_col).size(13.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let mut s = settings.lock();
                            egui::ComboBox::from_id_salt("q")
                                .selected_text(format!("Q{}", s.screenshot_quality))
                                .width(100.0)
                                .show_ui(ui, |ui| {
                                    for &q in QUALITY_OPTIONS {
                                        ui.selectable_value(
                                            &mut s.screenshot_quality,
                                            q,
                                            format!("Q{}", q),
                                        );
                                    }
                                });
                        });
                    });

                    ui.add_space(16.0);

                    let line_rect = ui.available_rect_before_wrap();
                    ui.painter().line_segment(
                        [line_rect.left_top(), line_rect.right_top()],
                        (1.0, separator_col),
                    );

                    ui.add_space(16.0);

                    ui.label(
                        egui::RichText::new("🎬 视频设置")
                            .color(header_col)
                            .size(14.0)
                            .strong(),
                    );

                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("帧率").color(label_col).size(13.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                                .width(100.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut s.fps, 15u32, "15 fps");
                                    ui.selectable_value(&mut s.fps, 30u32, "30 fps");
                                    ui.selectable_value(&mut s.fps, 60u32, "60 fps");
                                });
                        });
                    });

                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        let mut s = settings.lock();
                        ui.label(egui::RichText::new("边框").color(label_col).size(13.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut s.draw_border, "");
                        });
                    });

                    ui.add_space(12.0);

                    let line_rect2 = ui.available_rect_before_wrap();
                    ui.painter().line_segment(
                        [line_rect2.left_top(), line_rect2.right_top()],
                        (1.0, separator_col),
                    );

                    ui.add_space(12.0);

                    ui.with_layout(
                        egui::Layout::top_down_justified(egui::Align::Center),
                        |ui| {
                            ui.horizontal(|ui| {
                                if ui
                                    .add_sized(
                                        [106.0, 36.0],
                                        egui::Button::new(
                                            egui::RichText::new("取消")
                                                .color(egui::Color32::from_rgb(180, 180, 190))
                                                .size(14.0),
                                        )
                                        .stroke(
                                            egui::Stroke::new(
                                                1.0,
                                                egui::Color32::from_rgba_unmultiplied(
                                                    255, 255, 255, 40,
                                                ),
                                            ),
                                        ),
                                    )
                                    .clicked()
                                {
                                    log::info!("[Settings] Cancel clicked");
                                    SETTINGS_ROUNDED.store(false, Ordering::SeqCst);
                                    settings_open.store(false, Ordering::SeqCst);
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }

                                ui.add_space(8.0);

                                if ui
                                    .add_sized(
                                        [106.0, 36.0],
                                        egui::Button::new(
                                            egui::RichText::new("保存")
                                                .color(egui::Color32::WHITE)
                                                .size(14.0)
                                                .strong(),
                                        )
                                        .fill(egui::Color32::from_rgb(80, 200, 120)),
                                    )
                                    .clicked()
                                {
                                    log::info!("[Settings] Save clicked");
                                    let s = settings.lock();
                                    if let Err(e) = s.save() {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                    *shared_settings.write() = s.clone();
                                    drop(s);
                                    SETTINGS_ROUNDED.store(false, Ordering::SeqCst);
                                    settings_open.store(false, Ordering::SeqCst);
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                            });
                        },
                    );
                });

            if ctx.input(|i| i.viewport().close_requested()) {
                log::info!("[Settings] Viewport close requested");
                settings_open.store(false, Ordering::SeqCst);
                SETTINGS_ROUNDED.store(false, Ordering::SeqCst);
            }
        },
    );
}
