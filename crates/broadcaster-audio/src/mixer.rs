//! Audio mixing functionality.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use bytes::Bytes;
use crossbeam_channel::{Receiver, Sender};
use parking_lot::RwLock;
use tracing::{debug, info, instrument, trace, warn};

use crate::capture::AudioChunk;
use crate::error::AudioError;
use crate::{AudioResult, AUDIO_CHANNEL_CAPACITY, CHANNELS, SAMPLES_PER_CHUNK, SAMPLE_RATE};

/// Input source for the mixer.
pub struct MixerInput {
    /// Receiver for audio chunks from this source.
    pub receiver: Receiver<AudioChunk>,

    /// Volume multiplier (0.0 - 1.0).
    pub volume: Arc<RwLock<f32>>,

    /// Whether this source is muted.
    pub muted: Arc<AtomicBool>,
}

impl MixerInput {
    /// Create a new mixer input.
    pub fn new(receiver: Receiver<AudioChunk>) -> Self {
        Self {
            receiver,
            volume: Arc::new(RwLock::new(1.0)),
            muted: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Set the volume for this input.
    pub fn set_volume(&self, volume: f32) {
        *self.volume.write() = volume.clamp(0.0, 1.0);
    }

    /// Set the muted state for this input.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::SeqCst);
    }

    /// Get the current volume.
    pub fn volume(&self) -> f32 {
        *self.volume.read()
    }

    /// Check if this input is muted.
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::SeqCst)
    }
}

/// Mixed audio output chunk.
#[derive(Debug, Clone)]
pub struct MixedAudioChunk {
    /// Interleaved f32 stereo samples.
    pub data: Bytes,

    /// Presentation timestamp in 100ns units.
    pub pts_100ns: u64,

    /// Sequence number.
    pub sequence: u64,
}

/// Audio mixer that combines multiple input sources.
pub struct AudioMixer {
    mix_thread: Option<JoinHandle<()>>,
    should_stop: Arc<AtomicBool>,
    output_receiver: Option<Receiver<MixedAudioChunk>>,
    mic_volume: Arc<RwLock<f32>>,
    system_volume: Arc<RwLock<f32>>,
    mic_muted: Arc<AtomicBool>,
    system_muted: Arc<AtomicBool>,
}

impl AudioMixer {
    /// Create a new audio mixer.
    pub fn new() -> Self {
        Self {
            mix_thread: None,
            should_stop: Arc::new(AtomicBool::new(false)),
            output_receiver: None,
            mic_volume: Arc::new(RwLock::new(1.0)),
            system_volume: Arc::new(RwLock::new(1.0)),
            mic_muted: Arc::new(AtomicBool::new(false)),
            system_muted: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start mixing audio from the given inputs.
    #[instrument(name = "mixer_start", skip(self, mic_input, system_input))]
    pub fn start(
        &mut self,
        mic_input: Option<Receiver<AudioChunk>>,
        system_input: Option<Receiver<AudioChunk>>,
    ) -> AudioResult<Receiver<MixedAudioChunk>> {
        info!("Starting audio mixer");

        let (sender, receiver): (Sender<MixedAudioChunk>, Receiver<MixedAudioChunk>) =
            crossbeam_channel::bounded(AUDIO_CHANNEL_CAPACITY);

        let should_stop = Arc::clone(&self.should_stop);
        should_stop.store(false, Ordering::SeqCst);

        let mic_volume = Arc::clone(&self.mic_volume);
        let system_volume = Arc::clone(&self.system_volume);
        let mic_muted = Arc::clone(&self.mic_muted);
        let system_muted = Arc::clone(&self.system_muted);

        let handle = thread::spawn(move || {
            if let Err(e) = mix_thread(
                mic_input,
                system_input,
                sender,
                should_stop,
                mic_volume,
                system_volume,
                mic_muted,
                system_muted,
            ) {
                warn!("Mixer thread error: {}", e);
            }
        });

        self.mix_thread = Some(handle);
        self.output_receiver = Some(receiver.clone());

        Ok(receiver)
    }

    /// Stop the mixer.
    #[instrument(name = "mixer_stop", skip(self))]
    pub fn stop(&mut self) -> AudioResult<()> {
        info!("Stopping audio mixer");

        self.should_stop.store(true, Ordering::SeqCst);

        if let Some(handle) = self.mix_thread.take() {
            let _ = handle.join();
        }

        self.output_receiver = None;
        info!("Audio mixer stopped");
        Ok(())
    }

    /// Set microphone volume.
    pub fn set_mic_volume(&self, volume: f32) {
        *self.mic_volume.write() = volume.clamp(0.0, 1.0);
    }

    /// Set system audio volume.
    pub fn set_system_volume(&self, volume: f32) {
        *self.system_volume.write() = volume.clamp(0.0, 1.0);
    }

    /// Set microphone muted state.
    pub fn set_mic_muted(&self, muted: bool) {
        self.mic_muted.store(muted, Ordering::SeqCst);
    }

    /// Set system audio muted state.
    pub fn set_system_muted(&self, muted: bool) {
        self.system_muted.store(muted, Ordering::SeqCst);
    }
}

impl Default for AudioMixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AudioMixer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn mix_thread(
    mic_input: Option<Receiver<AudioChunk>>,
    system_input: Option<Receiver<AudioChunk>>,
    sender: Sender<MixedAudioChunk>,
    should_stop: Arc<AtomicBool>,
    mic_volume: Arc<RwLock<f32>>,
    system_volume: Arc<RwLock<f32>>,
    mic_muted: Arc<AtomicBool>,
    system_muted: Arc<AtomicBool>,
) -> AudioResult<()> {
    debug!("Mixer thread started");

    let samples_per_chunk = SAMPLES_PER_CHUNK * CHANNELS as usize;
    let mut mix_buffer = vec![0.0f32; samples_per_chunk];
    let mut sequence = 0u64;
    let start_time = std::time::Instant::now();

    // Calculate timing for 10ms chunks
    let chunk_duration = Duration::from_millis(10);
    let mut next_chunk_time = start_time;

    while !should_stop.load(Ordering::SeqCst) {
        // Reset mix buffer
        mix_buffer.fill(0.0);

        // Mix microphone input
        if let Some(ref mic_rx) = mic_input {
            if !mic_muted.load(Ordering::SeqCst) {
                let volume = *mic_volume.read();
                if let Ok(chunk) = mic_rx.try_recv() {
                    let samples = chunk.as_f32_slice();
                    for (i, &sample) in samples.iter().enumerate().take(mix_buffer.len()) {
                        mix_buffer[i] += sample * volume;
                    }
                }
            }
        }

        // Mix system audio input
        if let Some(ref system_rx) = system_input {
            if !system_muted.load(Ordering::SeqCst) {
                let volume = *system_volume.read();
                if let Ok(chunk) = system_rx.try_recv() {
                    let samples = chunk.as_f32_slice();
                    for (i, &sample) in samples.iter().enumerate().take(mix_buffer.len()) {
                        mix_buffer[i] += sample * volume;
                    }
                }
            }
        }

        // Soft clip to prevent harsh clipping
        for sample in mix_buffer.iter_mut() {
            *sample = soft_clip(*sample);
        }

        // Calculate PTS
        let elapsed = next_chunk_time.duration_since(start_time);
        let pts_100ns = elapsed.as_nanos() as u64 / 100;

        // Create output chunk
        let data = Bytes::copy_from_slice(unsafe {
            std::slice::from_raw_parts(
                mix_buffer.as_ptr() as *const u8,
                mix_buffer.len() * std::mem::size_of::<f32>(),
            )
        });

        let output = MixedAudioChunk {
            data,
            pts_100ns,
            sequence,
        };

        // Send output
        match sender.try_send(output) {
            Ok(()) => {}
            Err(crossbeam_channel::TrySendError::Full(_)) => {
                trace!("Mixed audio channel full, dropping chunk");
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                break;
            }
        }

        sequence += 1;
        next_chunk_time += chunk_duration;

        // Sleep until next chunk time
        let now = std::time::Instant::now();
        if next_chunk_time > now {
            thread::sleep(next_chunk_time - now);
        }
    }

    debug!("Mixer thread exiting");
    Ok(())
}

/// Soft clipping function to prevent harsh digital clipping.
fn soft_clip(sample: f32) -> f32 {
    if sample > 1.0 {
        1.0 - (-sample + 1.0).exp() * 0.5
    } else if sample < -1.0 {
        -1.0 + (sample + 1.0).exp() * 0.5
    } else {
        sample
    }
}
