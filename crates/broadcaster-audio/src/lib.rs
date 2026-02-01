//! WASAPI audio capture and mixing.
//!
//! This crate provides functionality to capture audio from microphones
//! and system audio (loopback) using WASAPI.

#[cfg(windows)]
mod capture;
#[cfg(windows)]
mod device;
mod error;
#[cfg(windows)]
mod mixer;

#[cfg(windows)]
pub use capture::{AudioCaptureSession, AudioChunk};
#[cfg(windows)]
pub use device::{enumerate_audio_devices, find_device_by_id};
pub use error::AudioError;
#[cfg(windows)]
pub use mixer::{AudioMixer, MixedAudioChunk, MixerInput};

/// Channel capacity for audio chunks.
pub const AUDIO_CHANNEL_CAPACITY: usize = 8;

/// Result type for audio operations.
pub type AudioResult<T> = Result<T, AudioError>;

/// Audio sample rate in Hz.
pub const SAMPLE_RATE: u32 = 48000;

/// Number of audio channels.
pub const CHANNELS: u16 = 2;

/// Samples per audio chunk (10ms at 48kHz).
pub const SAMPLES_PER_CHUNK: usize = 480;
