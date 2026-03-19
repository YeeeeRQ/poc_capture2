use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::Result;
use eframe::egui;
use eframe::run_native;

mod app;

use crate::app::CaptureApp;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let quit_flag = Arc::new(AtomicBool::new(false));
    let quit_for_app = Arc::clone(&quit_flag);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(false)
            .with_inner_size([420.0, 48.0])
            .with_min_inner_size([380.0, 44.0])
            .with_resizable(false)
            .with_always_on_top()
            .with_titlebar_shown(false),
        ..Default::default()
    };

    run_native(
        "Capture",
        options,
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();

            fonts.font_data.insert(
                "noto_sans_sc".into(),
                egui::FontData::from_owned(include_bytes!("../fonts/NotoSansSC.ttf").to_vec()),
            );
            fonts.font_data.insert(
                "noto_color_emoji".into(),
                egui::FontData::from_owned(include_bytes!("../fonts/NotoColorEmoji.ttf").to_vec()),
            );

            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "noto_sans_sc".into());

            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("noto_color_emoji".into());

            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "noto_sans_sc".into());

            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("noto_color_emoji".into());

            cc.egui_ctx.set_fonts(fonts);

            Ok(Box::new(CaptureApp::new(quit_for_app.clone())))
        }),
    )
    .map_err(|e| anyhow::anyhow!("GUI error: {}", e))?;

    Ok(())
}
