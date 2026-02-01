//! Core orchestrator for the broadcaster.
//!
//! This crate coordinates capture, audio, encoding, and transport
//! subsystems to provide a unified streaming engine.

mod metrics;
mod orchestrator;
mod state;

pub use metrics::MetricsCollector;
pub use orchestrator::Engine;
pub use state::{InitializedResources, ResourceManager};

use broadcaster_ipc::{EngineCommand, EngineEvent};
use crossbeam_channel::{Receiver, Sender};

/// Create an engine instance with IPC channels.
pub fn create_engine(
    command_rx: Receiver<EngineCommand>,
    event_tx: Sender<EngineEvent>,
) -> Engine {
    Engine::new(command_rx, event_tx)
}
