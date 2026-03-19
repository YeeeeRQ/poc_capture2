use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "capture",
    version,
    about = "Windows screen capture tool: screenshot & recording"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Take a screenshot")]
    Screenshot(ScreenshotArgs),

    #[cfg(feature = "record")]
    #[command(about = "Start screen recording (Ctrl+C to stop)")]
    Record(RecordArgs),

    #[cfg(not(feature = "record"))]
    Record {
        #[arg(long, default_value = "")]
        _placeholder: String,
    },

    #[command(about = "List all available monitors")]
    ListMonitors,
}

#[derive(Parser, Debug, Default)]
pub struct ScreenshotArgs {
    #[arg(short, long, default_value = "0", help = "Monitor index (0 = primary)")]
    pub monitor: usize,

    #[arg(short, long, help = "Output file path")]
    pub output: Option<String>,

    #[arg(short, long, default_value = "png", value_parser = ["png", "jpg"], help = "Output format")]
    pub format: String,

    #[arg(short, long, default_value = "90", help = "JPEG quality (0-100)")]
    pub quality: u8,
}

#[derive(Parser, Debug, Default)]
pub struct RecordArgs {
    #[arg(short, long, default_value = "0", help = "Monitor index (0 = primary)")]
    pub monitor: usize,

    #[arg(short, long, help = "Output file path")]
    pub output: Option<String>,

    #[arg(short, long, default_value = "30", help = "Frames per second")]
    pub fps: u32,

    #[arg(long, help = "Maximum recording duration in seconds (optional)")]
    pub duration: Option<u64>,

    #[arg(long, default_value = "medium", value_parser = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"], help = "Encoding speed preset")]
    pub preset: String,
}
