//! Engine state machine types.

use serde::{Deserialize, Serialize};

use crate::types::{StreamConfig, StreamMetrics};

/// The current state of the broadcast engine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum EngineState {
    /// Engine is idle, not streaming.
    #[default]
    Idle,

    /// Engine is starting up.
    Starting {
        /// Current startup phase.
        phase: StartupPhase,
    },

    /// Engine is live and streaming.
    Live {
        /// Active stream configuration.
        config: StreamConfig,

        /// Current stream metrics.
        metrics: StreamMetrics,
    },

    /// Engine is stopping.
    Stopping {
        /// Reason for stopping.
        reason: StopReason,

        /// Current shutdown phase.
        phase: ShutdownPhase,
    },

    /// Engine encountered a fatal error.
    Error {
        /// Error message.
        message: String,

        /// Whether recovery is possible.
        recoverable: bool,
    },
}

impl EngineState {
    /// Returns true if the engine is in the Idle state.
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    /// Returns true if the engine is currently live.
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Live { .. })
    }

    /// Returns true if the engine is starting.
    pub fn is_starting(&self) -> bool {
        matches!(self, Self::Starting { .. })
    }

    /// Returns true if the engine is stopping.
    pub fn is_stopping(&self) -> bool {
        matches!(self, Self::Stopping { .. })
    }

    /// Returns true if the engine is in an error state.
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    /// Returns a simple string representation of the state.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Starting { .. } => "Starting",
            Self::Live { .. } => "Live",
            Self::Stopping { .. } => "Stopping",
            Self::Error { .. } => "Error",
        }
    }
}

/// Startup phases for the engine, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartupPhase {
    /// Initializing capture subsystem.
    InitCapture,

    /// Initializing audio subsystem.
    InitAudio,

    /// Initializing video/audio encoders.
    InitEncoder,

    /// Connecting to RTMP server.
    ConnectRtmp,

    /// Starting transmission.
    StartTransmission,
}

impl StartupPhase {
    /// Returns the next phase, if any.
    pub fn next(self) -> Option<Self> {
        match self {
            Self::InitCapture => Some(Self::InitAudio),
            Self::InitAudio => Some(Self::InitEncoder),
            Self::InitEncoder => Some(Self::ConnectRtmp),
            Self::ConnectRtmp => Some(Self::StartTransmission),
            Self::StartTransmission => None,
        }
    }

    /// Returns the previous phase, if any (for rollback).
    pub fn previous(self) -> Option<Self> {
        match self {
            Self::InitCapture => None,
            Self::InitAudio => Some(Self::InitCapture),
            Self::InitEncoder => Some(Self::InitAudio),
            Self::ConnectRtmp => Some(Self::InitEncoder),
            Self::StartTransmission => Some(Self::ConnectRtmp),
        }
    }

    /// Returns the display name for this phase.
    pub fn name(self) -> &'static str {
        match self {
            Self::InitCapture => "Initializing capture",
            Self::InitAudio => "Initializing audio",
            Self::InitEncoder => "Initializing encoder",
            Self::ConnectRtmp => "Connecting to server",
            Self::StartTransmission => "Starting stream",
        }
    }
}

/// Shutdown phases for the engine, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShutdownPhase {
    /// Stopping transmission.
    StopTransmission,

    /// Disconnecting from RTMP server.
    DisconnectRtmp,

    /// Shutting down encoders.
    ShutdownEncoder,

    /// Shutting down audio.
    ShutdownAudio,

    /// Shutting down capture.
    ShutdownCapture,
}

impl ShutdownPhase {
    /// Returns the next phase, if any.
    pub fn next(self) -> Option<Self> {
        match self {
            Self::StopTransmission => Some(Self::DisconnectRtmp),
            Self::DisconnectRtmp => Some(Self::ShutdownEncoder),
            Self::ShutdownEncoder => Some(Self::ShutdownAudio),
            Self::ShutdownAudio => Some(Self::ShutdownCapture),
            Self::ShutdownCapture => None,
        }
    }

    /// Returns the display name for this phase.
    pub fn name(self) -> &'static str {
        match self {
            Self::StopTransmission => "Stopping stream",
            Self::DisconnectRtmp => "Disconnecting",
            Self::ShutdownEncoder => "Stopping encoder",
            Self::ShutdownAudio => "Stopping audio",
            Self::ShutdownCapture => "Stopping capture",
        }
    }
}

/// Reason for stopping the stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StopReason {
    /// User requested stop.
    UserRequested,

    /// Network connection lost.
    NetworkLost,

    /// Encoder error.
    EncoderError { message: String },

    /// Capture error.
    CaptureError { message: String },

    /// Fatal error occurred.
    FatalError { message: String },
}

impl StopReason {
    /// Returns a display message for this reason.
    pub fn message(&self) -> String {
        match self {
            Self::UserRequested => "Stream stopped by user".to_string(),
            Self::NetworkLost => "Network connection lost".to_string(),
            Self::EncoderError { message } => format!("Encoder error: {message}"),
            Self::CaptureError { message } => format!("Capture error: {message}"),
            Self::FatalError { message } => format!("Fatal error: {message}"),
        }
    }
}
