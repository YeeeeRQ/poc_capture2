#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::Result;
use chrono::Local;
use eframe::egui;

mod app;
mod main_window;
mod settings;
mod settings_window;
mod tray;
#[cfg(windows)]
mod windows_window;
mod worker;

use crate::app::CaptureApp;
use crate::worker::{spawn_worker, WorkerState};
use settings::Settings;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    if let Some(idx) = env::args().position(|arg| arg == "--dxgi-diagnostic") {
        let output_file = env::args().nth(idx + 1);
        let options = capture_core::DxgiDiagnosticOptions {
            verbose: true,
            output_file: output_file.map(PathBuf::from),
            exit_after: true,
        };
        capture_core::run_diagnostics(&options);
        return Ok(());
    }

    let session_folder = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(format!("capture_{}", Local::now().format("%Y%m%d_%H%M%S")));

    if let Err(e) = std::fs::create_dir_all(&session_folder) {
        log::error!("Failed to create session folder: {}", e);
    } else {
        log::info!("Session folder: {}", session_folder.display());
    }

    let app_settings = Settings::load();
    let monitors = capture_core::list_monitors().unwrap_or_default();

    let quit_flag = Arc::new(AtomicBool::new(false));

    let worker_state = WorkerState::new(session_folder.clone(), app_settings.clone());
    spawn_worker(worker_state);

    tray::setup_tray(
        Arc::clone(&quit_flag),
        session_folder.clone(),
        monitors.clone(),
    );

    let primary = capture_core::get_primary_monitor_rect()
        .map(|r| (r.x, r.y, r.width, r.height))
        .unwrap_or((0, 0, 1920, 1080));

    let main_w = 475.0_f32;
    let main_h = 48.0_f32;
    let margin = 88.0_f32;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size([main_w, main_h])
            .with_min_inner_size([400.0, 44.0])
            .with_resizable(true)
            .with_always_on_top()
            .with_titlebar_shown(false)
            .with_position(egui::Pos2::new(
                (primary.2 as f32 - main_w) / 2.0,
                primary.3 as f32 - main_h - margin,
            )),
        ..Default::default()
    };

    let quit_for_app = Arc::clone(&quit_flag);

    eframe::run_native(
        "Capture",
        native_options,
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();

            fonts.font_data.insert(
                "noto_sans_sc".into(),
                egui::FontData::from_owned(include_bytes!("../fonts/NotoSansSC.ttf").to_vec())
                    .into(),
            );
            fonts.font_data.insert(
                "noto_color_emoji".into(),
                egui::FontData::from_owned(include_bytes!("../fonts/NotoColorEmoji.ttf").to_vec())
                    .into(),
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
