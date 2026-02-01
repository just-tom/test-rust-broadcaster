//! Tauri application for the broadcaster.
//!
//! This module provides the bridge between the Tauri frontend and the
//! broadcaster engine, handling IPC commands and events.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::sync::Mutex;
use std::thread;

use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::{error, info};

use broadcaster_engine::Engine;
use broadcaster_ipc::{
    AudioDevice, CaptureSource, EngineCommand, EngineEvent, EngineState, StreamConfig,
};

/// Application state shared across Tauri commands.
struct AppState {
    /// Channel to send commands to the engine.
    command_tx: Sender<EngineCommand>,
    /// Channel to receive events from the engine.
    event_rx: Mutex<Receiver<EngineEvent>>,
}

/// Error type for Tauri commands.
#[derive(Debug, Serialize, Deserialize)]
struct CommandError {
    message: String,
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

type CommandResult<T> = Result<T, CommandError>;

/// Get available capture sources (monitors and windows).
#[tauri::command]
fn get_capture_sources(state: State<AppState>) -> CommandResult<Vec<CaptureSource>> {
    state
        .command_tx
        .send(EngineCommand::GetCaptureSources)
        .map_err(|e| CommandError::from(format!("Failed to send command: {}", e)))?;

    // Wait for the response event
    let rx = state.event_rx.lock().unwrap();
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(EngineEvent::CaptureSources(sources)) => return Ok(sources),
            Ok(_) => continue, // Skip other events
            Err(e) => {
                return Err(CommandError::from(format!(
                    "Timeout waiting for capture sources: {}",
                    e
                )))
            }
        }
    }
}

/// Get available audio devices.
#[tauri::command]
fn get_audio_devices(state: State<AppState>) -> CommandResult<Vec<AudioDevice>> {
    state
        .command_tx
        .send(EngineCommand::GetAudioDevices)
        .map_err(|e| CommandError::from(format!("Failed to send command: {}", e)))?;

    // Wait for the response event
    let rx = state.event_rx.lock().unwrap();
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(EngineEvent::AudioDevices(devices)) => return Ok(devices),
            Ok(_) => continue, // Skip other events
            Err(e) => {
                return Err(CommandError::from(format!(
                    "Timeout waiting for audio devices: {}",
                    e
                )))
            }
        }
    }
}

/// Poll for events from the engine (non-blocking).
#[tauri::command]
fn poll_events(state: State<AppState>) -> Vec<EngineEvent> {
    let rx = state.event_rx.lock().unwrap();
    let mut events = Vec::new();

    // Drain all available events
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }

    events
}

/// Start streaming with the given configuration.
#[tauri::command]
fn start_stream(state: State<AppState>, config: StreamConfig) -> CommandResult<()> {
    state
        .command_tx
        .send(EngineCommand::Start { config })
        .map_err(|e| CommandError::from(format!("Failed to send start command: {}", e)))?;
    Ok(())
}

/// Stop the current stream.
#[tauri::command]
fn stop_stream(state: State<AppState>) -> CommandResult<()> {
    state
        .command_tx
        .send(EngineCommand::Stop)
        .map_err(|e| CommandError::from(format!("Failed to send stop command: {}", e)))?;
    Ok(())
}

/// Set microphone volume (0.0 - 1.0).
#[tauri::command]
fn set_mic_volume(state: State<AppState>, volume: f32) -> CommandResult<()> {
    let volume = volume.clamp(0.0, 1.0);
    state
        .command_tx
        .send(EngineCommand::SetMicVolume(volume))
        .map_err(|e| CommandError::from(format!("Failed to send command: {}", e)))?;
    Ok(())
}

/// Set system audio volume (0.0 - 1.0).
#[tauri::command]
fn set_system_volume(state: State<AppState>, volume: f32) -> CommandResult<()> {
    let volume = volume.clamp(0.0, 1.0);
    state
        .command_tx
        .send(EngineCommand::SetSystemVolume(volume))
        .map_err(|e| CommandError::from(format!("Failed to send command: {}", e)))?;
    Ok(())
}

/// Mute or unmute the microphone.
#[tauri::command]
fn set_mic_muted(state: State<AppState>, muted: bool) -> CommandResult<()> {
    state
        .command_tx
        .send(EngineCommand::SetMicMuted(muted))
        .map_err(|e| CommandError::from(format!("Failed to send command: {}", e)))?;
    Ok(())
}

/// Mute or unmute system audio.
#[tauri::command]
fn set_system_muted(state: State<AppState>, muted: bool) -> CommandResult<()> {
    state
        .command_tx
        .send(EngineCommand::SetSystemMuted(muted))
        .map_err(|e| CommandError::from(format!("Failed to send command: {}", e)))?;
    Ok(())
}

/// Get the current engine state.
#[tauri::command]
fn get_state(state: State<AppState>) -> CommandResult<EngineState> {
    state
        .command_tx
        .send(EngineCommand::GetState)
        .map_err(|e| CommandError::from(format!("Failed to send command: {}", e)))?;

    // Wait for the response event
    let rx = state.event_rx.lock().unwrap();
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(EngineEvent::StateChanged { current, .. }) => return Ok(*current),
            Ok(_) => continue, // Skip other events
            Err(e) => {
                return Err(CommandError::from(format!(
                    "Timeout waiting for state: {}",
                    e
                )))
            }
        }
    }
}

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    info!("Starting broadcaster application");

    // Create IPC channels
    let (command_tx, command_rx) = broadcaster_ipc::command_channel();
    let (event_tx, event_rx) = broadcaster_ipc::event_channel();

    // Spawn engine thread
    thread::spawn(move || {
        info!("Engine thread starting");
        let mut engine = Engine::new(command_rx, event_tx);
        engine.run();
        info!("Engine thread stopped");
    });

    // Build and run Tauri application
    tauri::Builder::default()
        .manage(AppState {
            command_tx,
            event_rx: Mutex::new(event_rx),
        })
        .invoke_handler(tauri::generate_handler![
            get_capture_sources,
            get_audio_devices,
            poll_events,
            start_stream,
            stop_stream,
            set_mic_volume,
            set_system_volume,
            set_mic_muted,
            set_system_muted,
            get_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
