pub mod capture_adapter;
pub mod dxgi_diagnostic;
pub mod encoder;
pub mod perf_sampler;
pub mod perf_stats;
pub mod recorder;
pub mod screenshot;

pub use capture_adapter::enumerate_monitors;
pub use capture_adapter::RecordSettings;
pub use capture_adapter::ScreenshotRequest;
pub use dxgi_diagnostic::{run_diagnostics, DxgiDiagnosticOptions};
pub use encoder::FfmpegEncoder;
pub use perf_sampler::PerfSampler;
pub use perf_stats::PerfStats;
pub use recorder::RecorderHandle;
pub use screenshot::{
    get_primary_monitor_rect, list_monitors, take_screenshot, MonitorInfo, MonitorRect,
    ScreenshotSettings,
};
