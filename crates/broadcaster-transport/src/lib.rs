//! RTMP streaming client.
//!
//! This crate provides RTMP transport functionality for streaming
//! encoded video and audio to servers.

mod connection;
mod error;
mod rtmp;

pub use connection::{ConnectionState, ReconnectPolicy};
pub use error::TransportError;
pub use rtmp::RtmpClient;

/// Channel capacity for outgoing packets.
pub const PACKET_CHANNEL_CAPACITY: usize = 30;

/// Result type for transport operations.
pub type TransportResult<T> = Result<T, TransportError>;

/// Maximum reconnection attempts.
pub const MAX_RECONNECT_ATTEMPTS: u32 = 3;

/// Base reconnect delay in milliseconds.
pub const BASE_RECONNECT_DELAY_MS: u64 = 1000;
