use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[derive(Debug)]
pub struct PerfStats {
    pub frames_captured: AtomicU64,
    pub frames_encoded: AtomicU64,
    pub last_alloc_ms: AtomicU64,
    pub last_convert_ms: AtomicU64,
    pub last_send_ms: AtomicU64,
    pub last_snapshot_ms: AtomicU64,
    pub last_encode_ms: AtomicU64,
    pub total_alloc_bytes: AtomicU64,
    pub total_copy_bytes: AtomicU64,
    pub start_time: Instant,
    pub enabled: AtomicBool,
    accum_alloc_ms: AtomicU64,
    accum_convert_ms: AtomicU64,
    accum_send_ms: AtomicU64,
    accum_snapshot_ms: AtomicU64,
    max_alloc_ms: AtomicU64,
    max_convert_ms: AtomicU64,
    max_send_ms: AtomicU64,
    max_snapshot_ms: AtomicU64,
}

impl PerfStats {
    pub fn new() -> Self {
        Self {
            frames_captured: AtomicU64::new(0),
            frames_encoded: AtomicU64::new(0),
            last_alloc_ms: AtomicU64::new(0),
            last_convert_ms: AtomicU64::new(0),
            last_send_ms: AtomicU64::new(0),
            last_snapshot_ms: AtomicU64::new(0),
            last_encode_ms: AtomicU64::new(0),
            total_alloc_bytes: AtomicU64::new(0),
            total_copy_bytes: AtomicU64::new(0),
            start_time: Instant::now(),
            enabled: AtomicBool::new(true),
            accum_alloc_ms: AtomicU64::new(0),
            accum_convert_ms: AtomicU64::new(0),
            accum_send_ms: AtomicU64::new(0),
            accum_snapshot_ms: AtomicU64::new(0),
            max_alloc_ms: AtomicU64::new(0),
            max_convert_ms: AtomicU64::new(0),
            max_send_ms: AtomicU64::new(0),
            max_snapshot_ms: AtomicU64::new(0),
        }
    }

    pub fn record_capture(
        &self,
        alloc_ms: u64,
        convert_ms: u64,
        send_ms: u64,
        snapshot_ms: u64,
        bytes: u64,
    ) {
        self.frames_captured.fetch_add(1, Ordering::Relaxed);
        self.last_alloc_ms.store(alloc_ms, Ordering::Relaxed);
        self.last_convert_ms.store(convert_ms, Ordering::Relaxed);
        self.last_send_ms.store(send_ms, Ordering::Relaxed);
        self.last_snapshot_ms.store(snapshot_ms, Ordering::Relaxed);
        self.total_alloc_bytes
            .fetch_add(bytes * 2, Ordering::Relaxed);
        self.total_copy_bytes.fetch_add(bytes, Ordering::Relaxed);

        self.accum_alloc_ms.fetch_add(alloc_ms, Ordering::Relaxed);
        self.accum_convert_ms
            .fetch_add(convert_ms, Ordering::Relaxed);
        self.accum_send_ms.fetch_add(send_ms, Ordering::Relaxed);
        self.accum_snapshot_ms
            .fetch_add(snapshot_ms, Ordering::Relaxed);

        update_max(&self.max_alloc_ms, alloc_ms);
        update_max(&self.max_convert_ms, convert_ms);
        update_max(&self.max_send_ms, send_ms);
        update_max(&self.max_snapshot_ms, snapshot_ms);
    }

    pub fn record_encode(&self, encode_ms: u64) {
        self.frames_encoded.fetch_add(1, Ordering::Relaxed);
        self.last_encode_ms.store(encode_ms, Ordering::Relaxed);
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }

    pub fn capture_fps(&self) -> f64 {
        let elapsed = self.elapsed_secs();
        self.frames_captured.load(Ordering::Relaxed) as f64 / elapsed.max(0.001)
    }

    pub fn encode_fps(&self) -> f64 {
        let elapsed = self.elapsed_secs();
        self.frames_encoded.load(Ordering::Relaxed) as f64 / elapsed.max(0.001)
    }

    pub fn throughput_mbps(&self) -> f64 {
        let elapsed = self.elapsed_secs();
        self.total_alloc_bytes.load(Ordering::Relaxed) as f64 / elapsed / 1_048_576.0
    }

    pub fn total_allocated_mb(&self) -> u64 {
        self.total_alloc_bytes.load(Ordering::Relaxed) / 1024 / 1024
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        self.frames_captured.store(0, Ordering::Relaxed);
        self.frames_encoded.store(0, Ordering::Relaxed);
        self.last_alloc_ms.store(0, Ordering::Relaxed);
        self.last_convert_ms.store(0, Ordering::Relaxed);
        self.last_send_ms.store(0, Ordering::Relaxed);
        self.last_snapshot_ms.store(0, Ordering::Relaxed);
        self.last_encode_ms.store(0, Ordering::Relaxed);
        self.total_alloc_bytes.store(0, Ordering::Relaxed);
        self.total_copy_bytes.store(0, Ordering::Relaxed);
        self.accum_alloc_ms.store(0, Ordering::Relaxed);
        self.accum_convert_ms.store(0, Ordering::Relaxed);
        self.accum_send_ms.store(0, Ordering::Relaxed);
        self.accum_snapshot_ms.store(0, Ordering::Relaxed);
        self.max_alloc_ms.store(0, Ordering::Relaxed);
        self.max_convert_ms.store(0, Ordering::Relaxed);
        self.max_send_ms.store(0, Ordering::Relaxed);
        self.max_snapshot_ms.store(0, Ordering::Relaxed);
    }

    pub fn avg_alloc_ms(&self) -> f64 {
        let frames = self.frames_captured.load(Ordering::Relaxed);
        if frames == 0 {
            return 0.0;
        }
        self.accum_alloc_ms.load(Ordering::Relaxed) as f64 / frames as f64
    }

    pub fn avg_convert_ms(&self) -> f64 {
        let frames = self.frames_captured.load(Ordering::Relaxed);
        if frames == 0 {
            return 0.0;
        }
        self.accum_convert_ms.load(Ordering::Relaxed) as f64 / frames as f64
    }

    pub fn avg_send_ms(&self) -> f64 {
        let frames = self.frames_captured.load(Ordering::Relaxed);
        if frames == 0 {
            return 0.0;
        }
        self.accum_send_ms.load(Ordering::Relaxed) as f64 / frames as f64
    }

    pub fn avg_snapshot_ms(&self) -> f64 {
        let frames = self.frames_captured.load(Ordering::Relaxed);
        if frames == 0 {
            return 0.0;
        }
        self.accum_snapshot_ms.load(Ordering::Relaxed) as f64 / frames as f64
    }

    pub fn max_alloc_ms(&self) -> u64 {
        self.max_alloc_ms.load(Ordering::Relaxed)
    }

    pub fn max_convert_ms(&self) -> u64 {
        self.max_convert_ms.load(Ordering::Relaxed)
    }

    pub fn max_send_ms(&self) -> u64 {
        self.max_send_ms.load(Ordering::Relaxed)
    }

    pub fn max_snapshot_ms(&self) -> u64 {
        self.max_snapshot_ms.load(Ordering::Relaxed)
    }
}

fn update_max(max_val: &AtomicU64, new_val: u64) {
    let current = max_val.load(Ordering::Relaxed);
    if new_val > current {
        max_val.store(new_val, Ordering::Relaxed);
    }
}

impl Default for PerfStats {
    fn default() -> Self {
        Self::new()
    }
}
