//! Broadcaster Tauri application library.

mod commands;

#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use std::thread;

use parking_lot::Mutex;
#[cfg(windows)]
use tauri::Manager;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(windows)]
use broadcaster_engine::Engine;
use broadcaster_ipc::{command_channel, event_channel, EngineCommand, EngineEvent};
use crossbeam_channel::{Receiver, Sender};

/// Application state shared with Tauri commands.
pub struct AppState {
    pub command_tx: Sender<EngineCommand>,
    pub event_rx: Mutex<Receiver<EngineEvent>>,
}

/// Initialize logging.
fn init_logging() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "broadcaster=debug,broadcaster_engine=debug,broadcaster_capture=debug,broadcaster_audio=debug,broadcaster_encoder=debug,broadcaster_transport=debug".into()
        }))
        .with(tracing_subscriber::fmt::layer())
        .init();
}

#[cfg(windows)]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    info!("Broadcaster starting");

    // Create IPC channels
    let (command_tx, command_rx) = command_channel();
    let (event_tx, event_rx) = event_channel();

    // Start engine in background thread
    thread::spawn(move || {
        let mut engine = Engine::new(command_rx, event_tx);
        engine.run();
    });

    // Create app state
    let state = AppState {
        command_tx,
        event_rx: Mutex::new(event_rx),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::start_stream,
            commands::stop_stream,
            commands::set_mic_volume,
            commands::set_system_volume,
            commands::set_mic_muted,
            commands::set_system_muted,
            commands::get_capture_sources,
            commands::get_audio_devices,
            commands::get_state,
            commands::poll_events,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(not(windows))]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    info!("Broadcaster is only supported on Windows");

    // Create dummy IPC channels for compilation
    let (command_tx, _command_rx) = command_channel();
    let (_event_tx, event_rx) = event_channel();

    // Create app state
    let state = AppState {
        command_tx,
        event_rx: Mutex::new(event_rx),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::start_stream,
            commands::stop_stream,
            commands::set_mic_volume,
            commands::set_system_volume,
            commands::set_mic_muted,
            commands::set_system_muted,
            commands::get_capture_sources,
            commands::get_audio_devices,
            commands::get_state,
            commands::poll_events,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
