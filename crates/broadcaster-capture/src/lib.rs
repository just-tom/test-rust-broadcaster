//! Windows Graphics Capture for screen/window capture.
//!
//! This crate provides functionality to capture screens and windows
//! using the Windows Graphics Capture API.

mod error;
mod frame;
mod wgc;

pub use error::CaptureError;
pub use frame::{CapturedFrame, CaptureTimestamp};
pub use wgc::monitor::enumerate_monitors;
pub use wgc::session::CaptureSession;
pub use wgc::window::enumerate_windows;

use crossbeam_channel::Receiver;

/// Channel capacity for captured frames.
pub const FRAME_CHANNEL_CAPACITY: usize = 3;

/// Result type for capture operations.
pub type CaptureResult<T> = Result<T, CaptureError>;

/// Trait for capture sources.
pub trait CaptureSource: Send + Sync {
    /// Start capturing frames.
    fn start(&mut self) -> CaptureResult<Receiver<CapturedFrame>>;

    /// Stop capturing.
    fn stop(&mut self) -> CaptureResult<()>;

    /// Check if capture is active.
    fn is_active(&self) -> bool;

    /// Get the source dimensions.
    fn dimensions(&self) -> (u32, u32);
}
