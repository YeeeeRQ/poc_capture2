pub mod capture_adapter;
pub mod encoder;
pub mod recorder;
pub mod screenshot;

pub use capture_adapter::enumerate_monitors;
pub use capture_adapter::RecordSettings;
pub use capture_adapter::ScreenshotRequest;
pub use encoder::FfmpegEncoder;
pub use recorder::RecorderHandle;
pub use screenshot::{
    get_primary_monitor_rect, list_monitors, take_screenshot, MonitorInfo, MonitorRect,
    ScreenshotSettings,
};
