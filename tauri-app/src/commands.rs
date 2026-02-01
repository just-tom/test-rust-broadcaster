//! Tauri command handlers.

use tauri::State;
use tracing::{debug, instrument};

use broadcaster_ipc::{EngineCommand, EngineEvent, StreamConfig};

use crate::AppState;

/// Start streaming with the given configuration.
#[tauri::command]
#[instrument(skip(state, config))]
pub async fn start_stream(state: State<'_, AppState>, config: StreamConfig) -> Result<(), String> {
    debug!("start_stream command");
    state
        .command_tx
        .send(EngineCommand::Start { config })
        .map_err(|e| format!("Failed to send command: {}", e))
}

/// Stop the current stream.
#[tauri::command]
#[instrument(skip(state))]
pub async fn stop_stream(state: State<'_, AppState>) -> Result<(), String> {
    debug!("stop_stream command");
    state
        .command_tx
        .send(EngineCommand::Stop)
        .map_err(|e| format!("Failed to send command: {}", e))
}

/// Set microphone volume.
#[tauri::command]
pub async fn set_mic_volume(state: State<'_, AppState>, volume: f32) -> Result<(), String> {
    state
        .command_tx
        .send(EngineCommand::SetMicVolume(volume))
        .map_err(|e| format!("Failed to send command: {}", e))
}

/// Set system audio volume.
#[tauri::command]
pub async fn set_system_volume(state: State<'_, AppState>, volume: f32) -> Result<(), String> {
    state
        .command_tx
        .send(EngineCommand::SetSystemVolume(volume))
        .map_err(|e| format!("Failed to send command: {}", e))
}

/// Set microphone muted state.
#[tauri::command]
pub async fn set_mic_muted(state: State<'_, AppState>, muted: bool) -> Result<(), String> {
    state
        .command_tx
        .send(EngineCommand::SetMicMuted(muted))
        .map_err(|e| format!("Failed to send command: {}", e))
}

/// Set system audio muted state.
#[tauri::command]
pub async fn set_system_muted(state: State<'_, AppState>, muted: bool) -> Result<(), String> {
    state
        .command_tx
        .send(EngineCommand::SetSystemMuted(muted))
        .map_err(|e| format!("Failed to send command: {}", e))
}

/// Request capture sources from the engine.
#[tauri::command]
pub async fn get_capture_sources(state: State<'_, AppState>) -> Result<(), String> {
    state
        .command_tx
        .send(EngineCommand::GetCaptureSources)
        .map_err(|e| format!("Failed to send command: {}", e))
}

/// Request audio devices from the engine.
#[tauri::command]
pub async fn get_audio_devices(state: State<'_, AppState>) -> Result<(), String> {
    state
        .command_tx
        .send(EngineCommand::GetAudioDevices)
        .map_err(|e| format!("Failed to send command: {}", e))
}

/// Request current engine state.
#[tauri::command]
pub async fn get_state(state: State<'_, AppState>) -> Result<(), String> {
    state
        .command_tx
        .send(EngineCommand::GetState)
        .map_err(|e| format!("Failed to send command: {}", e))
}

/// Poll for engine events (non-blocking).
#[tauri::command]
pub async fn poll_events(state: State<'_, AppState>) -> Result<Vec<EngineEvent>, String> {
    let rx = state.event_rx.lock();
    let mut events = Vec::new();

    // Collect all available events without blocking
    loop {
        match rx.try_recv() {
            Ok(event) => events.push(event),
            Err(crossbeam_channel::TryRecvError::Empty) => break,
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                return Err("Event channel disconnected".to_string());
            }
        }
    }

    Ok(events)
}
