//! Error types for the transport module.

use thiserror::Error;

/// Errors that can occur during transport operations.
#[derive(Debug, Error)]
pub enum TransportError {
    /// Connection failed.
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Connection error (general).
    #[error("Connection error: {0}")]
    Connection(String),

    /// Connection lost.
    #[error("Connection lost: {0}")]
    ConnectionLost(String),

    /// Authentication failed.
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// Invalid RTMP URL.
    #[error("Invalid RTMP URL: {0}")]
    InvalidUrl(String),

    /// Send failed.
    #[error("Send failed: {0}")]
    SendFailed(String),

    /// Send error.
    #[error("Send error: {0}")]
    Send(String),

    /// Reconnect exhausted.
    #[error("Reconnect attempts exhausted after {0} attempts")]
    ReconnectExhausted(u32),

    /// Not connected.
    #[error("Not connected")]
    NotConnected,

    /// Already connected.
    #[error("Already connected")]
    AlreadyConnected,

    /// Channel disconnected.
    #[error("Channel disconnected")]
    ChannelDisconnected,

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// RTMP protocol error.
    #[error("RTMP protocol error: {0}")]
    Protocol(String),
}
