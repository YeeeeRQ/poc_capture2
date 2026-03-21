use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;

use crate::PerfStats;

pub struct PerfSampler {
    stats: Arc<PerfStats>,
    output_path: PathBuf,
    file: Arc<Mutex<Option<File>>>,
    last_sample_time: Arc<Mutex<Instant>>,
    last_frame_count: Arc<Mutex<u64>>,
    session_start: Instant,
}

impl PerfSampler {
    pub fn new(stats: Arc<PerfStats>, output_path: PathBuf) -> anyhow::Result<Self> {
        let file = File::create(&output_path)?;
        let sampler = Self {
            stats,
            output_path,
            file: Arc::new(Mutex::new(Some(file))),
            last_sample_time: Arc::new(Mutex::new(Instant::now())),
            last_frame_count: Arc::new(Mutex::new(0)),
            session_start: Instant::now(),
        };
        sampler.write_header()?;
        Ok(sampler)
    }

    fn write_header(&self) -> anyhow::Result<()> {
        if let Some(ref file) = *self.file.lock() {
            let mut f = file;
            writeln!(f, "# session_start={}", chrono::Utc::now().to_rfc3339())?;
            writeln!(f, "# target_fps={}", 30)?;
            writeln!(
                f,
                "elapsed_s,capture_fps,encode_fps,alloc_ms,convert_ms,send_ms,snapshot_ms,queue_len"
            )?;
        }
        Ok(())
    }

    pub fn sample(&self) -> anyhow::Result<()> {
        let now = Instant::now();
        let elapsed_since_last = {
            let last = *self.last_sample_time.lock();
            now.duration_since(last)
        };

        if elapsed_since_last.as_secs() < 1 {
            return Ok(());
        }

        {
            let mut last = self.last_sample_time.lock();
            *last = now;
        }

        let elapsed_s = now.duration_since(self.session_start).as_secs_f64();
        let capture_fps = self.stats.capture_fps();
        let encode_fps = self.stats.encode_fps();

        let alloc_ms = self.stats.last_alloc_ms.load(Ordering::Relaxed) as f64 / 1000.0;
        let convert_ms = self.stats.last_convert_ms.load(Ordering::Relaxed) as f64 / 1000.0;
        let send_ms = self.stats.last_send_ms.load(Ordering::Relaxed) as f64 / 1000.0;
        let snapshot_ms = self.stats.last_snapshot_ms.load(Ordering::Relaxed) as f64 / 1000.0;

        let frames_captured = self.stats.frames_captured.load(Ordering::Relaxed);
        let last_count = *self.last_frame_count.lock();
        let queue_len = frames_captured.saturating_sub(last_count);
        *self.last_frame_count.lock() = frames_captured;

        if let Some(ref file) = *self.file.lock() {
            let mut f = file;
            writeln!(
                f,
                "{:.1},{:.1},{:.1},{:.3},{:.3},{:.3},{:.3},{}",
                elapsed_s,
                capture_fps,
                encode_fps,
                alloc_ms,
                convert_ms,
                send_ms,
                snapshot_ms,
                queue_len
            )?;
        }

        Ok(())
    }

    pub fn finish(self) -> anyhow::Result<()> {
        if let Some(file) = self.file.lock().take() {
            let mut f = file;
            let summary = format!(
                "# avg_capture_fps={:.1}\n# avg_encode_fps={:.1}\n# total_frames={}\n# avg_alloc_ms={:.3}\n# max_alloc_ms={:.3}\n# avg_convert_ms={:.3}\n# max_convert_ms={:.3}",
                self.stats.capture_fps(),
                self.stats.encode_fps(),
                self.stats.frames_captured.load(Ordering::Relaxed),
                self.stats.avg_alloc_ms(),
                self.stats.max_alloc_ms() as f64 / 1000.0,
                self.stats.avg_convert_ms(),
                self.stats.max_convert_ms() as f64 / 1000.0,
            );
            writeln!(f, "{}", summary)?;
        }
        log::info!("[PerfSampler] CSV saved to: {}", self.output_path.display());
        Ok(())
    }
}

#[derive(Clone)]
pub struct PerfSamplerHandle {
    inner: Arc<Mutex<Option<PerfSampler>>>,
}

impl PerfSamplerHandle {
    pub fn new(sampler: PerfSampler) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(sampler))),
        }
    }

    pub fn sample(&self) -> anyhow::Result<()> {
        if let Some(ref sampler) = *self.inner.lock() {
            sampler.sample()?;
        }
        Ok(())
    }

    pub fn finish(self) -> anyhow::Result<()> {
        if let Some(sampler) = self.inner.lock().take() {
            sampler.finish()?;
        }
        Ok(())
    }
}
