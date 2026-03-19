use anyhow::Result;
use clap::{Parser, Subcommand};
use log::LevelFilter;
use std::path::PathBuf;

use capture_core::{
    list_monitors, take_screenshot, RecordSettings, RecorderHandle, ScreenshotSettings,
};

#[derive(Parser, Debug)]
#[command(
    name = "capture",
    version,
    about = "Windows screen capture tool: screenshot & recording"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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
}

#[derive(Parser, Debug, Default)]
struct RecordArgs {
    #[arg(short, long, default_value = "0")]
    monitor: usize,
    #[arg(short, long)]
    output: Option<String>,
    #[arg(short = 'f', long, default_value = "30")]
    fps: u32,
    #[arg(long)]
    duration: Option<u64>,
    #[arg(long, default_value = "medium", value_parser = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"])]
    preset: String,
}

fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .format_timestamp_millis()
        .init();

    let cli = Cli::parse();

    match cli.command {
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
                fps: args.fps,
                duration_secs: args.duration,
                preset: args.preset,
            };

            println!(
                "Recording monitor [{}] {}x{} @ {} fps...",
                args.monitor, monitors[args.monitor].width, monitors[args.monitor].height, args.fps
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
