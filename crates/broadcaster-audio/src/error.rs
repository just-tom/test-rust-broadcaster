//! Error types for the audio module.

use thiserror::Error;

/// Errors that can occur during audio operations.
#[derive(Debug, Error)]
pub enum AudioError {
    /// Windows API error.
    #[error("Windows API error: {message}")]
    WindowsApi {
        message: String,
        #[cfg(windows)]
        #[source]
        source: Option<windows::core::Error>,
    },

    /// Audio device not found.
    #[error("Audio device not found: {0}")]
    DeviceNotFound(String),

    /// Audio format not supported.
    #[error("Audio format not supported: {0}")]
    FormatNotSupported(String),

    /// Capture already started.
    #[error("Audio capture already started")]
    AlreadyStarted,

    /// Capture not started.
    #[error("Audio capture not started")]
    NotStarted,

    /// Device lost during capture.
    #[error("Audio device lost")]
    DeviceLost,

    /// Channel send error.
    #[error("Failed to send audio: channel disconnected")]
    ChannelDisconnected,

    /// Mixer error.
    #[error("Mixer error: {0}")]
    MixerError(String),
}

#[cfg(windows)]
impl From<windows::core::Error> for AudioError {
    fn from(err: windows::core::Error) -> Self {
        Self::WindowsApi {
            message: err.message().to_string_lossy(),
            source: Some(err),
        }
    }
}
