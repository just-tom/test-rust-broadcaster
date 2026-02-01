//! Typed UI<->Engine messages for the broadcaster.
//!
//! This crate defines all the message types used for communication between
//! the Tauri UI and the engine core.

mod commands;
mod events;
mod state;
mod types;

pub use commands::EngineCommand;
pub use events::EngineEvent;
pub use state::{EngineState, ShutdownPhase, StartupPhase, StopReason};
pub use types::{
    AudioDevice, AudioDeviceType, CaptureSource, CaptureSourceType, StreamConfig, StreamMetrics,
    WarningType,
};

use crossbeam_channel::{Receiver, Sender};

/// Channel capacity for commands (UI → Engine).
pub const COMMAND_CHANNEL_CAPACITY: usize = 64;

/// Channel capacity for events (Engine → UI).
pub const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Creates a bounded command channel.
pub fn command_channel() -> (Sender<EngineCommand>, Receiver<EngineCommand>) {
    crossbeam_channel::bounded(COMMAND_CHANNEL_CAPACITY)
}

/// Creates a bounded event channel.
pub fn event_channel() -> (Sender<EngineEvent>, Receiver<EngineEvent>) {
    crossbeam_channel::bounded(EVENT_CHANNEL_CAPACITY)
}
