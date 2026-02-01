//! AAC audio encoder.

use bytes::Bytes;
use tracing::{debug, instrument, trace};

use crate::error::EncoderError;
use crate::{AudioEncoder, AudioEncoderConfig, EncodedAudioPacket, EncoderResult};

/// AAC-LC audio encoder using fdk-aac.
pub struct AacEncoder {
    encoder: fdk_aac::enc::Encoder,
    config: AudioEncoderConfig,
    samples_per_frame: usize,
    sample_buffer: Vec<f32>,
    frame_count: u64,
    /// Output buffer for encoded data.
    output_buffer: Vec<u8>,
}

impl AacEncoder {
    /// Create a new AAC encoder.
    #[instrument(name = "aac_new", skip_all)]
    pub fn new(config: AudioEncoderConfig) -> EncoderResult<Self> {
        debug!(
            sample_rate = config.sample_rate,
            channels = config.channels,
            bitrate_kbps = config.bitrate_kbps,
            "Initializing AAC encoder"
        );

        // AAC frame size is 1024 samples per channel
        let samples_per_frame = 1024 * config.channels as usize;

        // Configure channel mode
        let channel_mode = if config.channels == 1 {
            fdk_aac::enc::ChannelMode::Mono
        } else {
            fdk_aac::enc::ChannelMode::Stereo
        };

        // Create encoder parameters
        let params = fdk_aac::enc::EncoderParams {
            bit_rate: fdk_aac::enc::BitRate::Cbr(config.bitrate_kbps * 1000),
            sample_rate: config.sample_rate,
            transport: fdk_aac::enc::Transport::Raw, // Raw AAC for RTMP
            channels: channel_mode,
        };

        // Create the encoder
        let encoder = fdk_aac::enc::Encoder::new(params)
            .map_err(|e| EncoderError::Initialization(format!("fdk-aac init failed: {:?}", e)))?;

        // Get encoder info for buffer sizing
        let info = encoder
            .info()
            .map_err(|e| EncoderError::Initialization(format!("fdk-aac info failed: {:?}", e)))?;

        debug!(
            max_out_buf_bytes = info.maxOutBufBytes,
            frame_length = info.frameLength,
            "AAC encoder initialized"
        );

        // Allocate output buffer based on encoder info
        let output_buffer = vec![0u8; info.maxOutBufBytes as usize];

        Ok(Self {
            encoder,
            config,
            samples_per_frame,
            sample_buffer: Vec::with_capacity(samples_per_frame * 2),
            frame_count: 0,
            output_buffer,
        })
    }

    /// Convert f32 samples to i16 for encoding.
    fn f32_to_i16(samples: &[f32]) -> Vec<i16> {
        samples
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect()
    }
}

impl AudioEncoder for AacEncoder {
    #[instrument(name = "aac_encode", skip(self, samples))]
    fn encode(
        &mut self,
        samples: &[f32],
        pts_100ns: u64,
    ) -> EncoderResult<Option<EncodedAudioPacket>> {
        // Add samples to buffer
        self.sample_buffer.extend_from_slice(samples);

        // Check if we have enough samples for a frame
        if self.sample_buffer.len() < self.samples_per_frame {
            return Ok(None);
        }

        trace!(
            buffer_size = self.sample_buffer.len(),
            frame = self.frame_count,
            "Encoding AAC frame"
        );

        // Take one frame worth of samples
        let frame_samples: Vec<f32> = self.sample_buffer.drain(..self.samples_per_frame).collect();

        // Convert f32 to i16 for encoding
        let pcm_i16 = Self::f32_to_i16(&frame_samples);

        // Encode the frame
        let encode_info = self
            .encoder
            .encode(&pcm_i16, &mut self.output_buffer)
            .map_err(|e| EncoderError::Encoding(format!("AAC encode failed: {:?}", e)))?;

        // If no data was produced, the encoder is still buffering
        if encode_info.output_size == 0 {
            self.frame_count += 1;
            return Ok(None);
        }

        // Extract the encoded data
        let aac_data = self.output_buffer[..encode_info.output_size].to_vec();

        let packet = EncodedAudioPacket {
            data: Bytes::from(aac_data),
            pts_100ns,
        };

        self.frame_count += 1;

        Ok(Some(packet))
    }

    fn flush(&mut self) -> EncoderResult<Vec<EncodedAudioPacket>> {
        debug!("Flushing AAC encoder");

        let mut packets = Vec::new();

        // If there are remaining samples, pad to a full frame and encode
        if !self.sample_buffer.is_empty() {
            // Pad with silence to reach a full frame
            let padding_needed = self.samples_per_frame - self.sample_buffer.len();
            self.sample_buffer
                .extend(std::iter::repeat_n(0.0f32, padding_needed));

            let pcm_i16 = Self::f32_to_i16(&self.sample_buffer);

            if let Ok(encode_info) = self.encoder.encode(&pcm_i16, &mut self.output_buffer) {
                if encode_info.output_size > 0 {
                    let aac_data = self.output_buffer[..encode_info.output_size].to_vec();
                    // Use frame count for final PTS calculation
                    let pts_100ns =
                        (self.frame_count * 1024 * 10_000_000) / self.config.sample_rate as u64;
                    packets.push(EncodedAudioPacket {
                        data: Bytes::from(aac_data),
                        pts_100ns,
                    });
                }
            }

            self.sample_buffer.clear();
        }

        Ok(packets)
    }

    fn name(&self) -> &'static str {
        "AAC-LC"
    }
}

impl Drop for AacEncoder {
    fn drop(&mut self) {
        debug!("Closing AAC encoder");
    }
}
