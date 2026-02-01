//! Error types for the capture module.

use thiserror::Error;

/// Errors that can occur during capture operations.
#[derive(Debug, Error)]
pub enum CaptureError {
    /// Windows API error.
    #[error("Windows API error: {message}")]
    WindowsApi {
        message: String,
        #[cfg(windows)]
        #[source]
        source: Option<windows::core::Error>,
    },

    /// Capture source not found.
    #[error("Capture source not found: {0}")]
    SourceNotFound(String),

    /// Capture already started.
    #[error("Capture already started")]
    AlreadyStarted,

    /// Capture not started.
    #[error("Capture not started")]
    NotStarted,

    /// Frame conversion error.
    #[error("Frame conversion error: {0}")]
    FrameConversion(String),

    /// Device lost during capture.
    #[error("Capture device lost")]
    DeviceLost,

    /// Graphics capture not supported on this system.
    #[error("Windows Graphics Capture not supported")]
    NotSupported,

    /// Permission denied for capture.
    #[error("Permission denied for capture")]
    PermissionDenied,

    /// Channel send error.
    #[error("Failed to send frame: channel disconnected")]
    ChannelDisconnected,
}

#[cfg(windows)]
impl From<windows::core::Error> for CaptureError {
    fn from(err: windows::core::Error) -> Self {
        Self::WindowsApi {
            message: err.message().to_string(),
            source: Some(err),
        }
    }
}
