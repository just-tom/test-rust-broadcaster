//! RTMP streaming client.
//!
//! This crate provides RTMP transport functionality for streaming
//! encoded video and audio to servers.

mod connection;
mod error;
mod nal;
mod rtmp;

pub use connection::{ConnectionState, ReconnectPolicy};
pub use error::TransportError;
pub use nal::{
    build_avc_decoder_config, build_flv_video_tag, extract_sps_pps, filter_parameter_sets,
    nals_to_avcc, parse_annex_b, NalUnit, NalUnitType,
};
pub use rtmp::{RtmpClient, RtmpPacket};

/// Channel capacity for outgoing packets.
pub const PACKET_CHANNEL_CAPACITY: usize = 300;

/// Result type for transport operations.
pub type TransportResult<T> = Result<T, TransportError>;

/// Maximum reconnection attempts.
pub const MAX_RECONNECT_ATTEMPTS: u32 = 3;

/// Base reconnect delay in milliseconds.
pub const BASE_RECONNECT_DELAY_MS: u64 = 1000;
