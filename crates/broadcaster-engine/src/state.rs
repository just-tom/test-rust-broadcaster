//! Resource management and initialization tracking.

use std::sync::Arc;

use crossbeam_channel::Receiver;
use parking_lot::Mutex;
use tracing::{debug, info, instrument, warn};

use broadcaster_audio::{AudioCaptureSession, AudioMixer, MixedAudioChunk};
use broadcaster_capture::{CaptureSession, CaptureSource, CapturedFrame};
use broadcaster_encoder::{
    create_audio_encoder, create_video_encoder, AudioEncoder, AudioEncoderConfig,
    VideoEncoder, VideoEncoderConfig,
};
use broadcaster_ipc::{StartupPhase, StreamConfig};
use broadcaster_transport::RtmpClient;

/// Resources that have been initialized during startup.
#[derive(Default)]
pub struct InitializedResources {
    /// Capture session.
    pub capture: Option<CaptureSession>,

    /// Audio capture sessions.
    pub mic_capture: Option<AudioCaptureSession>,
    pub system_capture: Option<AudioCaptureSession>,

    /// Audio mixer.
    pub mixer: Option<AudioMixer>,

    /// Video encoder.
    pub video_encoder: Option<Box<dyn VideoEncoder>>,

    /// Audio encoder.
    pub audio_encoder: Option<Box<dyn AudioEncoder>>,

    /// RTMP client.
    pub rtmp_client: Option<RtmpClient>,

    /// Frame receiver from capture.
    pub frame_rx: Option<Receiver<CapturedFrame>>,

    /// Mixed audio receiver.
    pub audio_rx: Option<Receiver<MixedAudioChunk>>,
}

impl InitializedResources {
    /// Create empty resources.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Manages resource initialization and cleanup.
pub struct ResourceManager {
    resources: Mutex<InitializedResources>,
    current_phase: Mutex<Option<StartupPhase>>,
}

impl ResourceManager {
    /// Create a new resource manager.
    pub fn new() -> Self {
        Self {
            resources: Mutex::new(InitializedResources::new()),
            current_phase: Mutex::new(None),
        }
    }

    /// Initialize resources up to and including the specified phase.
    #[instrument(name = "init_resources", skip(self, config))]
    pub fn initialize(
        &self,
        config: &StreamConfig,
        target_phase: StartupPhase,
    ) -> Result<(), String> {
        let mut phase = StartupPhase::InitCapture;

        loop {
            *self.current_phase.lock() = Some(phase);
            self.init_phase(config, phase)?;

            if phase == target_phase {
                break;
            }

            phase = phase.next().ok_or("No more phases")?;
        }

        Ok(())
    }

    /// Initialize a single phase.
    fn init_phase(&self, config: &StreamConfig, phase: StartupPhase) -> Result<(), String> {
        info!("Initializing phase: {:?}", phase);

        match phase {
            StartupPhase::InitCapture => self.init_capture(config),
            StartupPhase::InitAudio => self.init_audio(config),
            StartupPhase::InitEncoder => self.init_encoder(config),
            StartupPhase::ConnectRtmp => self.init_rtmp(config),
            StartupPhase::StartTransmission => self.start_transmission(),
        }
    }

    fn init_capture(&self, config: &StreamConfig) -> Result<(), String> {
        let mut session = CaptureSession::new(&config.capture_source)
            .map_err(|e| format!("Capture init failed: {}", e))?;

        let frame_rx = session
            .start()
            .map_err(|e| format!("Capture start failed: {}", e))?;

        let mut resources = self.resources.lock();
        resources.capture = Some(session);
        resources.frame_rx = Some(frame_rx);

        debug!("Capture initialized");
        Ok(())
    }

    fn init_audio(&self, config: &StreamConfig) -> Result<(), String> {
        let mut resources = self.resources.lock();

        // Initialize system audio (loopback)
        let mut system_capture = AudioCaptureSession::new_loopback();
        let system_rx = system_capture
            .start()
            .map_err(|e| format!("System audio init failed: {}", e))?;
        resources.system_capture = Some(system_capture);

        // Initialize microphone if specified
        let mic_rx = if let Some(ref mic_id) = config.mic_device {
            let mut mic_capture = AudioCaptureSession::new_microphone(Some(mic_id.clone()));
            let rx = mic_capture
                .start()
                .map_err(|e| format!("Mic init failed: {}", e))?;
            resources.mic_capture = Some(mic_capture);
            Some(rx)
        } else {
            None
        };

        // Initialize mixer
        let mut mixer = AudioMixer::new();
        mixer.set_mic_volume(config.mic_volume);
        mixer.set_system_volume(config.system_volume);

        let audio_rx = mixer
            .start(mic_rx, Some(system_rx))
            .map_err(|e| format!("Mixer init failed: {}", e))?;

        resources.mixer = Some(mixer);
        resources.audio_rx = Some(audio_rx);

        debug!("Audio initialized");
        Ok(())
    }

    fn init_encoder(&self, config: &StreamConfig) -> Result<(), String> {
        let mut resources = self.resources.lock();

        // Get dimensions from capture
        let (width, height) = if let Some(ref capture) = resources.capture {
            capture.dimensions()
        } else {
            (1920, 1080)
        };

        // Create video encoder
        let video_config = VideoEncoderConfig {
            width,
            height,
            fps: 60,
            bitrate_kbps: config.video_bitrate_kbps,
            keyframe_interval_secs: 2,
            ..Default::default()
        };

        let video_encoder = create_video_encoder(video_config)
            .map_err(|e| format!("Video encoder init failed: {}", e))?;

        // Create audio encoder
        let audio_config = AudioEncoderConfig {
            bitrate_kbps: config.audio_bitrate_kbps,
            ..Default::default()
        };

        let audio_encoder = create_audio_encoder(audio_config)
            .map_err(|e| format!("Audio encoder init failed: {}", e))?;

        resources.video_encoder = Some(video_encoder);
        resources.audio_encoder = Some(audio_encoder);

        debug!("Encoders initialized");
        Ok(())
    }

    fn init_rtmp(&self, config: &StreamConfig) -> Result<(), String> {
        let full_url = if config.rtmp_url.ends_with('/') {
            format!("{}{}", config.rtmp_url, config.stream_key)
        } else {
            format!("{}/{}", config.rtmp_url, config.stream_key)
        };

        let mut client = RtmpClient::new(config.rtmp_url.clone(), config.stream_key.clone())
            .map_err(|e| format!("RTMP client init failed: {}", e))?;

        client
            .connect()
            .map_err(|e| format!("RTMP connect failed: {}", e))?;

        self.resources.lock().rtmp_client = Some(client);

        debug!("RTMP connected");
        Ok(())
    }

    fn start_transmission(&self) -> Result<(), String> {
        // Transmission is started by the orchestrator's main loop
        debug!("Transmission ready");
        Ok(())
    }

    /// Rollback resources from the current phase backwards.
    #[instrument(name = "rollback_resources", skip(self))]
    pub fn rollback(&self) {
        let current = *self.current_phase.lock();

        if let Some(mut phase) = current {
            loop {
                info!("Rolling back phase: {:?}", phase);
                self.rollback_phase(phase);

                match phase.previous() {
                    Some(prev) => phase = prev,
                    None => break,
                }
            }
        }

        *self.current_phase.lock() = None;
    }

    fn rollback_phase(&self, phase: StartupPhase) {
        let mut resources = self.resources.lock();

        match phase {
            StartupPhase::StartTransmission => {
                // Nothing to rollback
            }
            StartupPhase::ConnectRtmp => {
                if let Some(mut client) = resources.rtmp_client.take() {
                    let _ = client.disconnect();
                }
            }
            StartupPhase::InitEncoder => {
                resources.video_encoder = None;
                resources.audio_encoder = None;
            }
            StartupPhase::InitAudio => {
                if let Some(mut mixer) = resources.mixer.take() {
                    let _ = mixer.stop();
                }
                if let Some(mut capture) = resources.mic_capture.take() {
                    let _ = capture.stop();
                }
                if let Some(mut capture) = resources.system_capture.take() {
                    let _ = capture.stop();
                }
                resources.audio_rx = None;
            }
            StartupPhase::InitCapture => {
                if let Some(mut capture) = resources.capture.take() {
                    let _ = capture.stop();
                }
                resources.frame_rx = None;
            }
        }
    }

    /// Shutdown all resources cleanly.
    #[instrument(name = "shutdown_resources", skip(self))]
    pub fn shutdown(&self) {
        info!("Shutting down all resources");
        self.rollback();
    }

    /// Get a reference to the resources (for the main loop).
    pub fn resources(&self) -> &Mutex<InitializedResources> {
        &self.resources
    }
}

impl Default for ResourceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ResourceManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}
