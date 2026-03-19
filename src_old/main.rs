use anyhow::Result;
use clap::Parser;
use log::LevelFilter;

mod capture;
mod cli;
mod encoder;

use cli::Cli;

fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .format_timestamp_millis()
        .init();

    let cli = Cli::parse();

    match &cli.command {
        cli::Commands::Screenshot(cmd) => {
            capture::screenshot::take_screenshot(cmd)?;
        }
        #[cfg(feature = "record")]
        cli::Commands::Record(cmd) => {
            capture::recorder::start_recording(cmd)?;
        }
        #[cfg(not(feature = "record"))]
        cli::Commands::Record(_) => {
            anyhow::bail!(
                "Recording feature is not enabled. Build with `cargo build --features record`"
            );
        }
        cli::Commands::ListMonitors => {
            capture::screenshot::list_monitors()?;
        }
    }

    Ok(())
}
