//! Core orchestrator for the broadcaster.
//!
//! This crate coordinates capture, audio, encoding, and transport
//! subsystems to provide a unified streaming engine.

mod metrics;
#[cfg(windows)]
mod orchestrator;
#[cfg(windows)]
mod state;

pub use metrics::MetricsCollector;
#[cfg(windows)]
pub use orchestrator::Engine;
#[cfg(windows)]
pub use state::{InitializedResources, ResourceManager};

use broadcaster_ipc::{EngineCommand, EngineEvent};
use crossbeam_channel::{Receiver, Sender};

/// Create an engine instance with IPC channels.
#[cfg(windows)]
pub fn create_engine(command_rx: Receiver<EngineCommand>, event_tx: Sender<EngineEvent>) -> Engine {
    Engine::new(command_rx, event_tx)
}

/// Stub for non-Windows platforms.
#[cfg(not(windows))]
pub fn create_engine(_command_rx: Receiver<EngineCommand>, _event_tx: Sender<EngineEvent>) -> ! {
    panic!("Broadcaster engine is only supported on Windows")
}
