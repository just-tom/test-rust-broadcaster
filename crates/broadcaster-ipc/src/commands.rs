//! Commands sent from the UI to the engine.

use serde::{Deserialize, Serialize};

use crate::types::StreamConfig;

/// Commands that the UI can send to the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineCommand {
    /// Start streaming with the given configuration.
    Start { config: StreamConfig },

    /// Stop the current stream.
    Stop,

    /// Set microphone volume (0.0 - 1.0).
    SetMicVolume(f32),

    /// Set system audio volume (0.0 - 1.0).
    SetSystemVolume(f32),

    /// Mute or unmute the microphone.
    SetMicMuted(bool),

    /// Mute or unmute system audio.
    SetSystemMuted(bool),

    /// Request the list of available capture sources.
    GetCaptureSources,

    /// Request the list of available audio devices.
    GetAudioDevices,

    /// Request current engine state.
    GetState,

    /// Shutdown the engine completely.
    Shutdown,
}
