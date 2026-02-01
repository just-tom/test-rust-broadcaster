//! Events sent from the engine to the UI.

use serde::{Deserialize, Serialize};

use crate::state::EngineState;
use crate::types::{AudioDevice, CaptureSource, StreamMetrics, WarningType};

/// Events that the engine can send to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineEvent {
    /// Engine state has changed.
    StateChanged {
        /// Previous state.
        previous: Box<EngineState>,

        /// Current state.
        current: Box<EngineState>,
    },

    /// Updated stream metrics.
    Metrics(StreamMetrics),

    /// Performance warning detected.
    PerformanceWarning(WarningType),

    /// Error occurred.
    Error {
        /// Whether the error is recoverable.
        recoverable: bool,

        /// Error message.
        message: String,
    },

    /// List of available capture sources.
    CaptureSources(Vec<CaptureSource>),

    /// List of available audio devices.
    AudioDevices(Vec<AudioDevice>),

    /// Engine is ready.
    Ready,

    /// Engine has shut down.
    Shutdown,
}
