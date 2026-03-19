use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::Result;
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
        Box::new(move |_cc| Ok(Box::new(CaptureApp::new(quit_for_app.clone())))),
    )
    .map_err(|e| anyhow::anyhow!("GUI error: {}", e))?;

    Ok(())
}
