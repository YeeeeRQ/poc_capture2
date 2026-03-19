pub mod encoder;
pub mod recorder;
pub mod screenshot;

pub use encoder::FfmpegEncoder;
pub use recorder::{start_recording, RecordSettings, RecorderHandle, RecorderState};
pub use screenshot::{list_monitors, take_screenshot, MonitorInfo, ScreenshotSettings};
