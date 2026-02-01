//! Error types for the encoder module.

use thiserror::Error;

/// Errors that can occur during encoding operations.
#[derive(Debug, Error)]
pub enum EncoderError {
    /// NVENC not available.
    #[error("NVENC not available: {0}")]
    NvencNotAvailable(String),

    /// NVENC initialization failed.
    #[error("NVENC initialization failed: {0}")]
    NvencInitFailed(String),

    /// x264 initialization failed.
    #[error("x264 initialization failed: {0}")]
    X264InitFailed(String),

    /// AAC encoder initialization failed.
    #[error("AAC encoder initialization failed: {0}")]
    AacInitFailed(String),

    /// General initialization error.
    #[error("Initialization failed: {0}")]
    Initialization(String),

    /// General encoding error.
    #[error("Encoding error: {0}")]
    Encoding(String),

    /// Encoding error (legacy).
    #[error("Encoding error: {0}")]
    EncodingError(String),

    /// Invalid input data.
    #[error("Invalid input data: {0}")]
    InvalidInput(String),

    /// Encoder overload.
    #[error("Encoder overload: queue depth {0}")]
    Overload(usize),

    /// Encoder not initialized.
    #[error("Encoder not initialized")]
    NotInitialized,
}
