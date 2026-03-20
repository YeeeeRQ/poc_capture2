use anyhow::Result;
use clap::{Parser, Subcommand};
use log::LevelFilter;
use std::path::PathBuf;

use capture_core::{
    list_monitors, run_diagnostics, take_screenshot, DxgiDiagnosticOptions, RecordSettings,
    RecorderHandle, ScreenshotSettings,
};

#[derive(Parser, Debug)]
#[command(
    name = "capture",
    version,
    about = "Windows screen capture tool: screenshot & recording"
)]
struct Cli {
    #[arg(long, help = "Run DXGI diagnostic and exit")]
    dxgi_diagnostic: Option<Option<String>>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Take a screenshot")]
    Screenshot(ScreenshotArgs),

    #[command(about = "Start screen recording (Ctrl+C to stop)")]
    Record(RecordArgs),

    #[command(about = "List all available monitors")]
    ListMonitors,
}

#[derive(Parser, Debug, Default)]
struct ScreenshotArgs {
    #[arg(short, long, default_value = "0")]
    monitor: usize,
    #[arg(short, long)]
    output: Option<String>,
    #[arg(short, long, default_value = "png", value_parser = ["png", "jpg"])]
    format: String,
    #[arg(short, long, default_value = "90")]
    quality: u8,
    #[arg(long, default_value = "false")]
    border: bool,
}

#[derive(Parser, Debug, Default)]
struct RecordArgs {
    #[arg(short, long, default_value = "0")]
    monitor: usize,
    #[arg(short, long)]
    output: Option<String>,
    #[arg(long)]
    duration: Option<u64>,
    #[arg(long, default_value = "60")]
    fps: u32,
    #[arg(long, default_value = "medium", value_parser = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"])]
    preset: String,
    #[arg(long, default_value = "false")]
    border: bool,
}

fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .format_timestamp_millis()
        .init();

    let cli = Cli::parse();

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            if cli.dxgi_diagnostic.is_some() {
                let file_path = cli.dxgi_diagnostic.as_ref().unwrap().as_ref();
                let options = DxgiDiagnosticOptions {
                    verbose: true,
                    output_file: file_path.map(PathBuf::from),
                    exit_after: true,
                };
                run_diagnostics(&options);
                return Ok(());
            }
            anyhow::bail!("No subcommand provided. Use --help for usage information.");
        }
    };

    match command {
        Commands::Screenshot(args) => {
            let monitors = list_monitors()?;
            if args.monitor >= monitors.len() {
                anyhow::bail!(
                    "Monitor index {} out of range (found {} monitor(s))",
                    args.monitor,
                    monitors.len()
                );
            }

            let settings = ScreenshotSettings {
                monitor_index: args.monitor,
                output_path: args.output.map(PathBuf::from),
                format: args.format,
                quality: args.quality,
                draw_border: args.border,
            };
            take_screenshot(&settings)?;
        }
        Commands::Record(args) => {
            let monitors = list_monitors()?;
            if args.monitor >= monitors.len() {
                anyhow::bail!(
                    "Monitor index {} out of range (found {} monitor(s))",
                    args.monitor,
                    monitors.len()
                );
            }

            let settings = RecordSettings {
                monitor_index: args.monitor,
                output_path: args.output.map(PathBuf::from),
                duration_secs: args.duration,
                target_fps: args.fps,
                preset: args.preset,
                draw_border: args.border,
            };

            println!(
                "Recording monitor [{}] {}x{}...",
                args.monitor, monitors[args.monitor].width, monitors[args.monitor].height
            );

            let output = start_recording_cli(settings)?;
            println!("Output: {}", output.display());
        }
        Commands::ListMonitors => {
            for m in list_monitors()? {
                println!("  [{}] {} ({}x{})", m.index, m.name, m.width, m.height);
            }
        }
    }

    Ok(())
}

fn start_recording_cli(settings: RecordSettings) -> Result<std::path::PathBuf> {
    let (handle, join_handle) = RecorderHandle::start(settings)?;

    let res = join_handle
        .join()
        .unwrap_or_else(|e| Err(anyhow::anyhow!("Recording thread panicked: {:?}", e)));

    drop(handle);
    res
}
