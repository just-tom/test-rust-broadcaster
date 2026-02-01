//! Captured frame types.

use bytes::Bytes;
use std::time::Instant;

/// Timestamp for a captured frame.
#[derive(Debug, Clone, Copy)]
pub struct CaptureTimestamp {
    /// Monotonic timestamp when the frame was captured.
    pub capture_time: Instant,

    /// Frame presentation timestamp in 100ns units (for AV sync).
    pub pts_100ns: u64,
}

impl CaptureTimestamp {
    /// Create a new capture timestamp.
    pub fn now(start_time: Instant) -> Self {
        let capture_time = Instant::now();
        let elapsed = capture_time.duration_since(start_time);
        let pts_100ns = elapsed.as_nanos() as u64 / 100;

        Self {
            capture_time,
            pts_100ns,
        }
    }

    /// Get the presentation timestamp in milliseconds.
    pub fn pts_ms(&self) -> u64 {
        self.pts_100ns / 10_000
    }
}

/// A captured video frame.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    /// NV12 pixel data.
    pub data: Bytes,

    /// Frame width in pixels.
    pub width: u32,

    /// Frame height in pixels.
    pub height: u32,

    /// Capture timestamp.
    pub timestamp: CaptureTimestamp,

    /// Monotonically increasing sequence number.
    pub sequence: u64,
}

impl CapturedFrame {
    /// Create a new captured frame.
    pub fn new(
        data: Bytes,
        width: u32,
        height: u32,
        timestamp: CaptureTimestamp,
        sequence: u64,
    ) -> Self {
        Self {
            data,
            width,
            height,
            timestamp,
            sequence,
        }
    }

    /// Calculate expected NV12 buffer size for given dimensions.
    pub fn nv12_buffer_size(width: u32, height: u32) -> usize {
        // NV12: Y plane (width * height) + UV plane (width * height / 2)
        let y_size = (width * height) as usize;
        let uv_size = y_size / 2;
        y_size + uv_size
    }

    /// Validate that the frame data matches expected dimensions.
    pub fn is_valid(&self) -> bool {
        let expected_size = Self::nv12_buffer_size(self.width, self.height);
        self.data.len() == expected_size
    }
}
