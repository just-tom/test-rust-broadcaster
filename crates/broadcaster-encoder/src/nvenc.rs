//! NVENC hardware video encoder.

use bytes::Bytes;
use tracing::{debug, info, instrument, warn};

use crate::error::EncoderError;
use crate::{
    EncodedVideoPacket, EncoderResult, FrameType, H264Profile, VideoEncoder, VideoEncoderConfig,
};

// Conditional compilation for NVENC support
#[cfg(all(windows, feature = "nvenc"))]
mod nvenc_impl {
    use super::*;
    use nvidia_video_codec_sdk::safe::api::ENCODE_API;
    use nvidia_video_codec_sdk::safe::encoder::{Buffer, Encoder, Session};
    use std::sync::OnceLock;

    static NVENC_AVAILABLE: OnceLock<bool> = OnceLock::new();

    /// Check if NVENC is available on this system.
    pub fn check_nvenc_available() -> bool {
        *NVENC_AVAILABLE.get_or_init(|| {
            // Try to initialize the NVENC API
            match ENCODE_API.lock() {
                Ok(_) => {
                    info!("NVENC API available");
                    true
                }
                Err(e) => {
                    debug!("NVENC not available: {:?}", e);
                    false
                }
            }
        })
    }
}

#[cfg(not(all(windows, feature = "nvenc")))]
mod nvenc_impl {
    use super::*;

    /// NVENC is not available on non-Windows or without the nvenc feature.
    pub fn check_nvenc_available() -> bool {
        debug!("NVENC support not compiled in (requires Windows + nvenc feature)");
        false
    }
}

/// NVENC hardware encoder wrapper.
///
/// This encoder uses NVIDIA's hardware video encoding capabilities when available.
/// Falls back gracefully if NVENC is not supported on the system.
pub struct NvencEncoder {
    config: VideoEncoderConfig,
    initialized: bool,
    frame_count: u64,
    keyframe_interval: u64,
    // In the future with full NVENC support:
    // #[cfg(all(windows, feature = "nvenc"))]
    // session: Option<Session>,
    // #[cfg(all(windows, feature = "nvenc"))]
    // buffers: Vec<Buffer>,
}

impl NvencEncoder {
    /// Create a new NVENC encoder.
    #[instrument(name = "nvenc_new", skip_all)]
    pub fn new(config: VideoEncoderConfig) -> EncoderResult<Self> {
        // Check for NVIDIA GPU and NVENC support
        if !Self::check_nvenc_available() {
            return Err(EncoderError::NvencNotAvailable(
                "No NVIDIA GPU with NVENC support detected".to_string(),
            ));
        }

        let keyframe_interval = (config.fps * config.keyframe_interval_secs) as u64;

        debug!(
            width = config.width,
            height = config.height,
            fps = config.fps,
            bitrate_kbps = config.bitrate_kbps,
            "Initializing NVENC encoder"
        );

        // Full NVENC implementation would:
        // 1. Create CUDA context
        // 2. Initialize Encoder with the CUDA device
        // 3. Configure encoder settings (profile, bitrate, etc.)
        // 4. Start a Session
        // 5. Allocate input Buffers and output Bitstreams

        Ok(Self {
            config,
            initialized: true,
            frame_count: 0,
            keyframe_interval,
        })
    }

    /// Check if NVENC is available on this system.
    pub fn check_nvenc_available() -> bool {
        nvenc_impl::check_nvenc_available()
    }

    /// Check if NVENC support is compiled into this build.
    pub fn is_compiled_with_nvenc() -> bool {
        cfg!(all(windows, feature = "nvenc"))
    }
}

impl VideoEncoder for NvencEncoder {
    #[instrument(name = "nvenc_encode", skip(self, frame))]
    fn encode(
        &mut self,
        frame: &[u8],
        pts_100ns: u64,
    ) -> EncoderResult<Option<EncodedVideoPacket>> {
        if !self.initialized {
            return Err(EncoderError::NotInitialized);
        }

        let expected_size = (self.config.width * self.config.height * 3 / 2) as usize;
        if frame.len() != expected_size {
            return Err(EncoderError::InvalidInput(format!(
                "Expected {} bytes, got {}",
                expected_size,
                frame.len()
            )));
        }

        let is_keyframe = self.frame_count % self.keyframe_interval == 0;
        let frame_type = if is_keyframe {
            FrameType::I
        } else {
            FrameType::P
        };

        // In a real implementation, this would:
        // 1. Copy frame data to GPU input buffer
        // 2. Submit encode task
        // 3. Wait for completion
        // 4. Retrieve encoded bitstream

        // Placeholder: return a dummy packet
        let packet = EncodedVideoPacket {
            data: Bytes::from(vec![0u8; 1024]), // Placeholder
            pts_100ns,
            dts_100ns: pts_100ns,
            is_keyframe,
            frame_type,
        };

        self.frame_count += 1;

        Ok(Some(packet))
    }

    fn flush(&mut self) -> EncoderResult<Vec<EncodedVideoPacket>> {
        // Flush any buffered frames
        Ok(Vec::new())
    }

    fn is_hardware_accelerated(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "NVENC"
    }
}

impl Drop for NvencEncoder {
    fn drop(&mut self) {
        // Clean up NVENC resources
        self.initialized = false;
    }
}
