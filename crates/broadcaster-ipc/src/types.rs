//! Common types used across IPC messages.

use serde::{Deserialize, Serialize};

/// Configuration for starting a stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// RTMP server URL (e.g., "rtmp://live.twitch.tv/app").
    pub rtmp_url: String,

    /// Stream key for authentication.
    pub stream_key: String,

    /// Capture source identifier.
    pub capture_source: String,

    /// Microphone device identifier (None for no mic).
    pub mic_device: Option<String>,

    /// Initial microphone volume (0.0 - 1.0).
    pub mic_volume: f32,

    /// Initial system audio volume (0.0 - 1.0).
    pub system_volume: f32,

    /// Video bitrate in kbps (default: 6000).
    pub video_bitrate_kbps: u32,

    /// Audio bitrate in kbps (default: 128).
    pub audio_bitrate_kbps: u32,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            rtmp_url: String::new(),
            stream_key: String::new(),
            capture_source: String::new(),
            mic_device: None,
            mic_volume: 1.0,
            system_volume: 1.0,
            video_bitrate_kbps: 6000,
            audio_bitrate_kbps: 128,
        }
    }
}

/// Real-time stream metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamMetrics {
    /// Current video frames per second.
    pub fps: f32,

    /// Target video frames per second.
    pub target_fps: f32,

    /// Current bitrate in kbps.
    pub bitrate_kbps: u32,

    /// Target bitrate in kbps.
    pub target_bitrate_kbps: u32,

    /// Total dropped frames since stream start.
    pub dropped_frames: u64,

    /// Frames dropped at capture stage.
    pub capture_drops: u64,

    /// Frames dropped at encode stage.
    pub encode_drops: u64,

    /// Frames dropped at network stage.
    pub network_drops: u64,

    /// Encoder load percentage (0-100).
    pub encoder_load_percent: f32,

    /// Network buffer fullness percentage (0-100).
    pub buffer_fullness_percent: f32,

    /// Stream uptime in seconds.
    pub uptime_seconds: u64,
}

/// Types of performance warnings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WarningType {
    /// Encoder is falling behind.
    EncoderOverload { load_percent: f32 },

    /// Network buffer is filling up.
    NetworkCongestion { buffer_percent: f32 },

    /// Frames are being dropped at capture.
    CaptureDrops { count: u64 },

    /// High CPU usage detected.
    HighCpuUsage { percent: f32 },

    /// Low available memory.
    LowMemory { available_mb: u64 },
}

/// A capture source (monitor or window).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSource {
    /// Unique identifier for this source.
    pub id: String,

    /// Display name for the UI.
    pub name: String,

    /// Type of capture source.
    pub source_type: CaptureSourceType,

    /// Width in pixels.
    pub width: u32,

    /// Height in pixels.
    pub height: u32,
}

/// Type of capture source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CaptureSourceType {
    /// A monitor/display.
    Monitor,

    /// An application window.
    Window,
}

/// An audio device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    /// Unique identifier for this device.
    pub id: String,

    /// Display name for the UI.
    pub name: String,

    /// Type of audio device.
    pub device_type: AudioDeviceType,

    /// Whether this is the default device.
    pub is_default: bool,
}

/// Type of audio device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AudioDeviceType {
    /// Input device (microphone).
    Input,

    /// Output device (for loopback capture).
    Output,
}
