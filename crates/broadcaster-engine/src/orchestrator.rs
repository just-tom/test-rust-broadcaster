//! Main engine orchestrator.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use parking_lot::RwLock;
use tracing::{debug, error, info, instrument, warn};

use broadcaster_audio::enumerate_audio_devices;
use broadcaster_capture::{enumerate_monitors, enumerate_windows};
use broadcaster_ipc::{
    AudioDevice, CaptureSource, EngineCommand, EngineEvent, EngineState, ShutdownPhase,
    StartupPhase, StopReason, StreamConfig, StreamMetrics,
};

use crate::metrics::MetricsCollector;
use crate::state::ResourceManager;

/// The main broadcast engine.
pub struct Engine {
    command_rx: Receiver<EngineCommand>,
    event_tx: Sender<EngineEvent>,
    state: Arc<RwLock<EngineState>>,
    resource_manager: Arc<ResourceManager>,
    metrics: Arc<MetricsCollector>,
    engine_thread: Option<JoinHandle<()>>,
    should_stop: Arc<AtomicBool>,
}

impl Engine {
    /// Create a new engine.
    pub fn new(command_rx: Receiver<EngineCommand>, event_tx: Sender<EngineEvent>) -> Self {
        Self {
            command_rx,
            event_tx,
            state: Arc::new(RwLock::new(EngineState::Idle)),
            resource_manager: Arc::new(ResourceManager::new()),
            metrics: Arc::new(MetricsCollector::default()),
            engine_thread: None,
            should_stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Run the engine (blocking).
    #[instrument(name = "engine_run", skip(self))]
    pub fn run(&mut self) {
        info!("Engine starting");
        self.send_event(EngineEvent::Ready);

        loop {
            match self.command_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(command) => {
                    if !self.handle_command(command) {
                        break;
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // Check if we need to send metrics
                    if self.state.read().is_live() {
                        self.emit_metrics();
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    info!("Command channel disconnected, shutting down");
                    break;
                }
            }
        }

        info!("Engine stopped");
    }

    /// Handle a command. Returns false if engine should stop.
    fn handle_command(&mut self, command: EngineCommand) -> bool {
        debug!(?command, "Handling command");

        match command {
            EngineCommand::Start { config } => self.start_stream(config),
            EngineCommand::Stop => self.stop_stream(StopReason::UserRequested),
            EngineCommand::SetMicVolume(volume) => self.set_mic_volume(volume),
            EngineCommand::SetSystemVolume(volume) => self.set_system_volume(volume),
            EngineCommand::SetMicMuted(muted) => self.set_mic_muted(muted),
            EngineCommand::SetSystemMuted(muted) => self.set_system_muted(muted),
            EngineCommand::GetCaptureSources => self.send_capture_sources(),
            EngineCommand::GetAudioDevices => self.send_audio_devices(),
            EngineCommand::GetState => self.send_state(),
            EngineCommand::Shutdown => {
                self.stop_stream(StopReason::UserRequested);
                self.send_event(EngineEvent::Shutdown);
                return false;
            }
        }

        true
    }

    /// Start streaming.
    #[instrument(name = "start_stream", skip(self, config))]
    fn start_stream(&mut self, config: StreamConfig) {
        // Idempotent: ignore if already starting or live
        {
            let state = self.state.read();
            if state.is_starting() || state.is_live() {
                debug!("Already starting or live, ignoring start command");
                return;
            }
        }

        info!("Starting stream");
        self.transition_to(EngineState::Starting {
            phase: StartupPhase::InitCapture,
        });

        // Try to initialize all resources
        match self
            .resource_manager
            .initialize(&config, StartupPhase::StartTransmission)
        {
            Ok(()) => {
                // Start metrics collection
                self.metrics = Arc::new(MetricsCollector::new(60.0, config.video_bitrate_kbps));
                self.metrics.start();

                // Transition to live
                self.transition_to(EngineState::Live {
                    config,
                    metrics: StreamMetrics::default(),
                });

                // Start the streaming loop
                self.start_stream_loop();

                info!("Stream started successfully");
            }
            Err(e) => {
                error!("Stream start failed: {}", e);

                // Rollback any initialized resources
                self.resource_manager.rollback();

                self.transition_to(EngineState::Error {
                    message: e,
                    recoverable: true,
                });
            }
        }
    }

    /// Start the main streaming loop in a separate thread.
    fn start_stream_loop(&mut self) {
        let resources = Arc::clone(&self.resource_manager);
        let metrics = Arc::clone(&self.metrics);
        let state = Arc::clone(&self.state);
        let should_stop = Arc::clone(&self.should_stop);

        should_stop.store(false, Ordering::SeqCst);

        let handle = thread::spawn(move || {
            stream_loop(resources, metrics, state, should_stop);
        });

        self.engine_thread = Some(handle);
    }

    /// Stop streaming.
    #[instrument(name = "stop_stream", skip(self))]
    fn stop_stream(&mut self, reason: StopReason) {
        // Idempotent: ignore if already idle or stopping
        {
            let state = self.state.read();
            if state.is_idle() || state.is_stopping() {
                debug!("Already idle or stopping, ignoring stop command");
                return;
            }
        }

        info!(?reason, "Stopping stream");

        // Signal stream loop to stop
        self.should_stop.store(true, Ordering::SeqCst);

        // Wait for stream thread to finish
        if let Some(handle) = self.engine_thread.take() {
            let _ = handle.join();
        }

        self.transition_to(EngineState::Stopping {
            reason: reason.clone(),
            phase: ShutdownPhase::StopTransmission,
        });

        // Stop metrics
        self.metrics.stop();

        // Shutdown resources
        self.resource_manager.shutdown();

        self.transition_to(EngineState::Idle);
        info!("Stream stopped");
    }

    fn set_mic_volume(&self, volume: f32) {
        let resources = self.resource_manager.resources().lock();
        if let Some(ref mixer) = resources.mixer {
            mixer.set_mic_volume(volume);
        }
    }

    fn set_system_volume(&self, volume: f32) {
        let resources = self.resource_manager.resources().lock();
        if let Some(ref mixer) = resources.mixer {
            mixer.set_system_volume(volume);
        }
    }

    fn set_mic_muted(&self, muted: bool) {
        let resources = self.resource_manager.resources().lock();
        if let Some(ref mixer) = resources.mixer {
            mixer.set_mic_muted(muted);
        }
    }

    fn set_system_muted(&self, muted: bool) {
        let resources = self.resource_manager.resources().lock();
        if let Some(ref mixer) = resources.mixer {
            mixer.set_system_muted(muted);
        }
    }

    fn send_capture_sources(&self) {
        let mut sources = Vec::new();

        // Enumerate monitors
        if let Ok(monitors) = enumerate_monitors() {
            for monitor in monitors {
                sources.push(monitor.to_capture_source());
            }
        }

        // Enumerate windows
        if let Ok(windows) = enumerate_windows() {
            for window in windows {
                sources.push(window.to_capture_source());
            }
        }

        self.send_event(EngineEvent::CaptureSources(sources));
    }

    fn send_audio_devices(&self) {
        let devices = enumerate_audio_devices().unwrap_or_default();
        self.send_event(EngineEvent::AudioDevices(devices));
    }

    fn send_state(&self) {
        let state = self.state.read().clone();
        self.send_event(EngineEvent::StateChanged {
            previous: Box::new(state.clone()),
            current: Box::new(state),
        });
    }

    fn emit_metrics(&self) {
        let metrics = self.metrics.snapshot();
        self.send_event(EngineEvent::Metrics(metrics));

        // Check for warnings
        for warning in self.metrics.check_warnings() {
            self.send_event(EngineEvent::PerformanceWarning(warning));
        }

        self.metrics.mark_reported();
    }

    fn transition_to(&self, new_state: EngineState) {
        let previous = {
            let mut state = self.state.write();
            let prev = state.clone();
            *state = new_state.clone();
            prev
        };

        debug!(
            previous = %previous.name(),
            current = %new_state.name(),
            "State transition"
        );

        self.send_event(EngineEvent::StateChanged {
            previous: Box::new(previous),
            current: Box::new(new_state),
        });
    }

    fn send_event(&self, event: EngineEvent) {
        if let Err(e) = self.event_tx.try_send(event) {
            warn!("Failed to send event: {}", e);
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.engine_thread.take() {
            let _ = handle.join();
        }
    }
}

/// Main streaming loop.
fn stream_loop(
    resources: Arc<ResourceManager>,
    metrics: Arc<MetricsCollector>,
    state: Arc<RwLock<EngineState>>,
    should_stop: Arc<AtomicBool>,
) {
    debug!("Stream loop starting");

    let frame_interval = Duration::from_nanos(1_000_000_000 / 60); // 60 FPS
    let mut last_frame_time = Instant::now();

    while !should_stop.load(Ordering::SeqCst) {
        let now = Instant::now();

        // Process video frame
        {
            let mut res = resources.resources().lock();

            // Get frame from capture
            if let Some(ref frame_rx) = res.frame_rx {
                match frame_rx.try_recv() {
                    Ok(frame) => {
                        // Encode frame
                        if let Some(ref mut encoder) = res.video_encoder {
                            match encoder.encode(&frame.data, frame.timestamp.pts_100ns) {
                                Ok(Some(packet)) => {
                                    // Send to RTMP
                                    metrics.record_frame();
                                    metrics.record_bytes_sent(packet.data.len() as u64);
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    warn!("Encode error: {}", e);
                                    metrics.record_encode_drop();
                                }
                            }
                        }
                    }
                    Err(crossbeam_channel::TryRecvError::Empty) => {}
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        warn!("Frame channel disconnected");
                        break;
                    }
                }
            }

            // Process audio
            if let Some(ref audio_rx) = res.audio_rx {
                while let Ok(chunk) = audio_rx.try_recv() {
                    if let Some(ref mut encoder) = res.audio_encoder {
                        let samples = unsafe {
                            std::slice::from_raw_parts(
                                chunk.data.as_ptr() as *const f32,
                                chunk.data.len() / std::mem::size_of::<f32>(),
                            )
                        };

                        match encoder.encode(samples, chunk.pts_100ns) {
                            Ok(Some(packet)) => {
                                metrics.record_bytes_sent(packet.data.len() as u64);
                            }
                            Ok(None) => {}
                            Err(e) => {
                                warn!("Audio encode error: {}", e);
                            }
                        }
                    }
                }
            }
        }

        // Rate limiting
        let elapsed = now.elapsed();
        if elapsed < frame_interval {
            thread::sleep(frame_interval - elapsed);
        }

        last_frame_time = now;
    }

    debug!("Stream loop stopped");
}
