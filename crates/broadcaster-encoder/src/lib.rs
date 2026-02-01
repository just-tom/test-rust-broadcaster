//! Video (NVENC/x264) and audio (AAC) encoding.
//!
//! This crate provides hardware-accelerated video encoding via NVENC
//! with x264 software fallback, plus AAC audio encoding.

#[cfg(windows)]
mod aac;
mod error;
#[cfg(windows)]
mod nvenc;
#[cfg(windows)]
mod x264;

#[cfg(windows)]
pub use aac::AacEncoder;
pub use error::EncoderError;
#[cfg(windows)]
pub use nvenc::NvencEncoder;
#[cfg(windows)]
pub use x264::X264Encoder;

use bytes::Bytes;

/// Channel capacity for encoded packets.
pub const ENCODED_CHANNEL_CAPACITY: usize = 8;

/// Result type for encoder operations.
pub type EncoderResult<T> = Result<T, EncoderError>;

/// Video encoding configuration.
#[derive(Debug, Clone)]
pub struct VideoEncoderConfig {
    /// Width in pixels.
    pub width: u32,

    /// Height in pixels.
    pub height: u32,

    /// Target frames per second.
    pub fps: u32,

    /// Target bitrate in kbps.
    pub bitrate_kbps: u32,

    /// Keyframe interval in seconds.
    pub keyframe_interval_secs: u32,

    /// H.264 profile.
    pub profile: H264Profile,
}

impl Default for VideoEncoderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 60,
            bitrate_kbps: 6000,
            keyframe_interval_secs: 2,
            profile: H264Profile::High,
        }
    }
}

/// H.264 profile levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H264Profile {
    Baseline,
    Main,
    High,
}

/// Audio encoding configuration.
#[derive(Debug, Clone)]
pub struct AudioEncoderConfig {
    /// Sample rate in Hz.
    pub sample_rate: u32,

    /// Number of channels.
    pub channels: u16,

    /// Target bitrate in kbps.
    pub bitrate_kbps: u32,
}

impl Default for AudioEncoderConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            bitrate_kbps: 128,
        }
    }
}

/// An encoded video packet.
#[derive(Debug, Clone)]
pub struct EncodedVideoPacket {
    /// Encoded NAL data.
    pub data: Bytes,

    /// Presentation timestamp in 100ns units.
    pub pts_100ns: u64,

    /// Decode timestamp in 100ns units.
    pub dts_100ns: u64,

    /// Whether this is a keyframe.
    pub is_keyframe: bool,

    /// Frame type for priority ordering.
    pub frame_type: FrameType,
}

/// Video frame type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FrameType {
    /// I-frame (keyframe) - highest priority.
    I = 0,

    /// P-frame - medium priority.
    P = 1,

    /// B-frame - lowest priority.
    B = 2,
}

/// An encoded audio packet.
#[derive(Debug, Clone)]
pub struct EncodedAudioPacket {
    /// Encoded AAC data.
    pub data: Bytes,

    /// Presentation timestamp in 100ns units.
    pub pts_100ns: u64,
}

/// Trait for video encoders.
pub trait VideoEncoder: Send {
    /// Encode a frame in NV12 format.
    fn encode(&mut self, frame: &[u8], pts_100ns: u64)
        -> EncoderResult<Option<EncodedVideoPacket>>;

    /// Flush any remaining frames.
    fn flush(&mut self) -> EncoderResult<Vec<EncodedVideoPacket>>;

    /// Check if the encoder supports hardware acceleration.
    fn is_hardware_accelerated(&self) -> bool;

    /// Get encoder name for diagnostics.
    fn name(&self) -> &'static str;
}

/// Trait for audio encoders.
pub trait AudioEncoder: Send {
    /// Encode audio samples.
    fn encode(
        &mut self,
        samples: &[f32],
        pts_100ns: u64,
    ) -> EncoderResult<Option<EncodedAudioPacket>>;

    /// Flush any remaining samples.
    fn flush(&mut self) -> EncoderResult<Vec<EncodedAudioPacket>>;

    /// Get encoder name for diagnostics.
    fn name(&self) -> &'static str;
}

/// Create a video encoder, preferring NVENC with x264 fallback.
#[cfg(windows)]
pub fn create_video_encoder(config: VideoEncoderConfig) -> EncoderResult<Box<dyn VideoEncoder>> {
    // Try NVENC first
    match NvencEncoder::new(config.clone()) {
        Ok(encoder) => {
            tracing::info!("Using NVENC hardware encoder");
            Ok(Box::new(encoder))
        }
        Err(e) => {
            tracing::warn!("NVENC not available: {}, falling back to x264", e);
            let encoder = X264Encoder::new(config)?;
            tracing::info!("Using x264 software encoder");
            Ok(Box::new(encoder))
        }
    }
}

/// Create a video encoder (stub for non-Windows platforms).
#[cfg(not(windows))]
pub fn create_video_encoder(_config: VideoEncoderConfig) -> EncoderResult<Box<dyn VideoEncoder>> {
    Err(EncoderError::NotSupported(
        "Video encoding is only supported on Windows".into(),
    ))
}

/// Create an audio encoder.
#[cfg(windows)]
pub fn create_audio_encoder(config: AudioEncoderConfig) -> EncoderResult<Box<dyn AudioEncoder>> {
    let encoder = AacEncoder::new(config)?;
    Ok(Box::new(encoder))
}

/// Create an audio encoder (stub for non-Windows platforms).
#[cfg(not(windows))]
pub fn create_audio_encoder(_config: AudioEncoderConfig) -> EncoderResult<Box<dyn AudioEncoder>> {
    Err(EncoderError::NotSupported(
        "Audio encoding is only supported on Windows".into(),
    ))
}
