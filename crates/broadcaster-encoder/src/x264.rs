//! x264 software video encoder.

use bytes::Bytes;
use tracing::{debug, instrument, trace};

use crate::error::EncoderError;
use crate::{
    EncodedVideoPacket, EncoderResult, FrameType, H264Profile, VideoEncoder, VideoEncoderConfig,
};

/// x264 software encoder wrapper.
pub struct X264Encoder {
    encoder: x264::Encoder,
    config: VideoEncoderConfig,
    frame_count: u64,
    keyframe_interval: u64,
    /// Cached SPS/PPS header data.
    headers: Bytes,
}

impl X264Encoder {
    /// Create a new x264 encoder.
    #[instrument(name = "x264_new", skip_all)]
    pub fn new(config: VideoEncoderConfig) -> EncoderResult<Self> {
        debug!(
            width = config.width,
            height = config.height,
            fps = config.fps,
            bitrate_kbps = config.bitrate_kbps,
            "Initializing x264 encoder"
        );

        let keyframe_interval = (config.fps * config.keyframe_interval_secs) as u64;

        // Build x264 encoder using the Setup builder
        let mut setup = x264::Setup::preset(
            x264::Preset::Veryfast,
            x264::Tune::Zerolatency,
            false, // fast_decode
            true,  // zero_latency
        )
        .fps(config.fps, 1)
        .bitrate(config.bitrate_kbps as i32)
        .max_keyframe_interval(keyframe_interval as i32)
        .scenecut_threshold(0); // Disable scenecut for predictable keyframes

        // Apply H.264 profile
        setup = match config.profile {
            H264Profile::Baseline => setup.baseline(),
            H264Profile::Main => setup.main(),
            H264Profile::High => setup.high(),
        };

        // Build encoder with NV12 colorspace
        let encoder = setup
            .build(
                x264::Colorspace::NV12,
                config.width as i32,
                config.height as i32,
            )
            .map_err(|e| EncoderError::Initialization(format!("x264 setup failed: {}", e)))?;

        // Get SPS/PPS headers
        let headers = encoder.headers().map_or_else(
            |_| Bytes::new(),
            |h| Bytes::from(h.entirety().to_vec()),
        );

        debug!(header_size = headers.len(), "x264 encoder initialized");

        Ok(Self {
            encoder,
            config,
            frame_count: 0,
            keyframe_interval,
            headers,
        })
    }

}

impl VideoEncoder for X264Encoder {
    #[instrument(name = "x264_encode", skip(self, frame))]
    fn encode(&mut self, frame: &[u8], pts_100ns: u64) -> EncoderResult<Option<EncodedVideoPacket>> {
        let expected_size = (self.config.width * self.config.height * 3 / 2) as usize;
        if frame.len() != expected_size {
            return Err(EncoderError::InvalidInput(format!(
                "Expected {} bytes ({}x{} NV12), got {}",
                expected_size, self.config.width, self.config.height, frame.len()
            )));
        }

        trace!(frame = self.frame_count, pts = pts_100ns, "Encoding frame");

        // NV12 format: Y plane followed by interleaved UV plane
        let y_size = (self.config.width * self.config.height) as usize;
        let uv_size = y_size / 2;

        let y_plane = &frame[..y_size];
        let uv_plane = &frame[y_size..y_size + uv_size];

        // Create x264 Image from NV12 planes
        // NV12: Y plane (stride = width), UV plane interleaved (stride = width)
        let y_stride = self.config.width as i32;
        let uv_stride = self.config.width as i32;

        let image = x264::Image::new(
            x264::Colorspace::NV12,
            self.config.width as i32,
            self.config.height as i32,
            &[
                x264::Plane {
                    data: y_plane,
                    stride: y_stride,
                },
                x264::Plane {
                    data: uv_plane,
                    stride: uv_stride,
                },
            ],
        );

        // Convert 100ns units to encoder timebase (fps)
        // pts_100ns is in 100-nanosecond units
        let pts = (pts_100ns * self.config.fps as u64) / 10_000_000;

        // Encode the frame
        let (data, picture) = self
            .encoder
            .encode(pts as i64, image)
            .map_err(|e| EncoderError::Encoding(format!("x264 encode failed: {}", e)))?;

        // If no data was produced, the frame is being buffered
        if data.len() == 0 {
            self.frame_count += 1;
            return Ok(None);
        }

        // Get all NAL units as a single byte slice
        let nal_data = data.entirety().to_vec();

        // Use picture's keyframe method for reliable keyframe detection
        let is_keyframe = picture.keyframe();

        // Determine frame type from picture info or keyframe detection
        let frame_type = if is_keyframe {
            FrameType::I
        } else if self.frame_count == 0 {
            // First frame should be a keyframe
            FrameType::I
        } else {
            FrameType::P
        };

        // Get DTS from picture (convert back to 100ns units)
        let dts_100ns = (picture.dts() as u64 * 10_000_000) / self.config.fps as u64;

        let packet = EncodedVideoPacket {
            data: Bytes::from(nal_data),
            pts_100ns,
            dts_100ns,
            is_keyframe,
            frame_type,
        };

        self.frame_count += 1;

        Ok(Some(packet))
    }

    fn flush(&mut self) -> EncoderResult<Vec<EncodedVideoPacket>> {
        debug!("Flushing x264 encoder");

        let mut packets = Vec::new();
        let flush = self.encoder.flush();

        for result in flush {
            match result {
                Ok((data, picture)) => {
                    if data.len() > 0 {
                        let nal_data = data.entirety().to_vec();
                        let is_keyframe = picture.keyframe();

                        let pts_100ns =
                            (picture.pts() as u64 * 10_000_000) / self.config.fps as u64;
                        let dts_100ns =
                            (picture.dts() as u64 * 10_000_000) / self.config.fps as u64;

                        packets.push(EncodedVideoPacket {
                            data: Bytes::from(nal_data),
                            pts_100ns,
                            dts_100ns,
                            is_keyframe,
                            frame_type: if is_keyframe { FrameType::I } else { FrameType::P },
                        });
                    }
                }
                Err(e) => {
                    debug!("Flush iteration ended: {}", e);
                    break;
                }
            }
        }

        Ok(packets)
    }

    fn is_hardware_accelerated(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "x264"
    }
}

impl Drop for X264Encoder {
    fn drop(&mut self) {
        // Clean up x264 resources
        debug!("Closing x264 encoder");
    }
}
