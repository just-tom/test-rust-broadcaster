//! Windows Graphics Capture for screen/window capture.
//!
//! This crate provides functionality to capture screens and windows
//! using the Windows Graphics Capture API.

mod error;
#[cfg(windows)]
mod frame;
#[cfg(windows)]
mod wgc;

pub use error::CaptureError;
#[cfg(windows)]
pub use frame::{CaptureTimestamp, CapturedFrame};
#[cfg(windows)]
pub use wgc::monitor::enumerate_monitors;
#[cfg(windows)]
pub use wgc::session::CaptureSession;
#[cfg(windows)]
pub use wgc::window::enumerate_windows;

#[cfg(windows)]
use crossbeam_channel::Receiver;

/// Channel capacity for captured frames.
pub const FRAME_CHANNEL_CAPACITY: usize = 3;

/// Result type for capture operations.
pub type CaptureResult<T> = Result<T, CaptureError>;

/// Trait for capture sources.
#[cfg(windows)]
pub trait CaptureSource {
    /// Start capturing frames.
    fn start(&mut self) -> CaptureResult<Receiver<CapturedFrame>>;

    /// Stop capturing.
    fn stop(&mut self) -> CaptureResult<()>;

    /// Check if capture is active.
    fn is_active(&self) -> bool;

    /// Get the source dimensions.
    fn dimensions(&self) -> (u32, u32);
}
