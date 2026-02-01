//! Metrics collection and reporting.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use tracing::debug;

use broadcaster_ipc::{StreamMetrics, WarningType};

/// Collects and reports stream metrics.
pub struct MetricsCollector {
    start_time: RwLock<Option<Instant>>,
    frame_count: AtomicU64,
    capture_drops: AtomicU64,
    encode_drops: AtomicU64,
    network_drops: AtomicU64,
    bytes_sent: AtomicU64,
    last_report_time: RwLock<Instant>,
    last_frame_count: AtomicU64,
    target_fps: f32,
    target_bitrate_kbps: u32,
    encoder_load: RwLock<f32>,
    buffer_fullness: RwLock<f32>,
}

impl MetricsCollector {
    /// Create a new metrics collector.
    pub fn new(target_fps: f32, target_bitrate_kbps: u32) -> Self {
        Self {
            start_time: RwLock::new(None),
            frame_count: AtomicU64::new(0),
            capture_drops: AtomicU64::new(0),
            encode_drops: AtomicU64::new(0),
            network_drops: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            last_report_time: RwLock::new(Instant::now()),
            last_frame_count: AtomicU64::new(0),
            target_fps,
            target_bitrate_kbps,
            encoder_load: RwLock::new(0.0),
            buffer_fullness: RwLock::new(0.0),
        }
    }

    /// Start metrics collection.
    pub fn start(&self) {
        *self.start_time.write() = Some(Instant::now());
        *self.last_report_time.write() = Instant::now();
    }

    /// Stop metrics collection.
    pub fn stop(&self) {
        *self.start_time.write() = None;
    }

    /// Record a frame being processed.
    pub fn record_frame(&self) {
        self.frame_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a capture drop.
    pub fn record_capture_drop(&self) {
        self.capture_drops.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an encode drop.
    pub fn record_encode_drop(&self) {
        self.encode_drops.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a network drop.
    pub fn record_network_drop(&self) {
        self.network_drops.fetch_add(1, Ordering::Relaxed);
    }

    /// Record bytes sent.
    pub fn record_bytes_sent(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Update encoder load percentage.
    pub fn update_encoder_load(&self, load: f32) {
        *self.encoder_load.write() = load.clamp(0.0, 100.0);
    }

    /// Update buffer fullness percentage.
    pub fn update_buffer_fullness(&self, fullness: f32) {
        *self.buffer_fullness.write() = fullness.clamp(0.0, 100.0);
    }

    /// Get current metrics snapshot.
    pub fn snapshot(&self) -> StreamMetrics {
        let now = Instant::now();

        // Calculate FPS
        let last_time = *self.last_report_time.read();
        let elapsed = now.duration_since(last_time);
        let current_frames = self.frame_count.load(Ordering::Relaxed);
        let last_frames = self.last_frame_count.load(Ordering::Relaxed);

        let fps = if elapsed.as_secs_f32() > 0.0 {
            (current_frames - last_frames) as f32 / elapsed.as_secs_f32()
        } else {
            0.0
        };

        // Calculate bitrate
        let bytes = self.bytes_sent.load(Ordering::Relaxed);
        let bitrate_kbps = if let Some(start) = *self.start_time.read() {
            let total_elapsed = now.duration_since(start).as_secs_f32();
            if total_elapsed > 0.0 {
                ((bytes * 8) as f32 / total_elapsed / 1000.0) as u32
            } else {
                0
            }
        } else {
            0
        };

        // Calculate uptime
        let uptime_seconds = self
            .start_time
            .read()
            .map(|s| now.duration_since(s).as_secs())
            .unwrap_or(0);

        let capture_drops = self.capture_drops.load(Ordering::Relaxed);
        let encode_drops = self.encode_drops.load(Ordering::Relaxed);
        let network_drops = self.network_drops.load(Ordering::Relaxed);

        StreamMetrics {
            fps,
            target_fps: self.target_fps,
            bitrate_kbps,
            target_bitrate_kbps: self.target_bitrate_kbps,
            dropped_frames: capture_drops + encode_drops + network_drops,
            capture_drops,
            encode_drops,
            network_drops,
            encoder_load_percent: *self.encoder_load.read(),
            buffer_fullness_percent: *self.buffer_fullness.read(),
            uptime_seconds,
        }
    }

    /// Check for warnings based on current metrics.
    pub fn check_warnings(&self) -> Vec<WarningType> {
        let mut warnings = Vec::new();

        let encoder_load = *self.encoder_load.read();
        if encoder_load > 90.0 {
            warnings.push(WarningType::EncoderOverload {
                load_percent: encoder_load,
            });
        }

        let buffer_fullness = *self.buffer_fullness.read();
        if buffer_fullness > 80.0 {
            warnings.push(WarningType::NetworkCongestion {
                buffer_percent: buffer_fullness,
            });
        }

        warnings
    }

    /// Update last report time for FPS calculation.
    pub fn mark_reported(&self) {
        *self.last_report_time.write() = Instant::now();
        self.last_frame_count.store(
            self.frame_count.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new(60.0, 6000)
    }
}
