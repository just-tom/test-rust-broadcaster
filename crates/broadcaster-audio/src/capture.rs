//! Audio capture using WASAPI.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use bytes::Bytes;
use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;
use tracing::{debug, info, trace, warn, instrument};
use windows::Win32::Media::Audio::{
    IAudioCaptureClient, IAudioClient, IMMDevice, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_LOOPBACK,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use broadcaster_ipc::AudioDeviceType;

use crate::device::{find_device_by_id, get_default_device};
use crate::error::AudioError;
use crate::{AudioResult, AUDIO_CHANNEL_CAPACITY, CHANNELS, SAMPLES_PER_CHUNK, SAMPLE_RATE};

/// An audio chunk captured from a device.
#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// Audio samples as f32 interleaved stereo.
    pub data: Bytes,

    /// Timestamp when this chunk was captured.
    pub timestamp: Instant,

    /// Monotonically increasing sequence number.
    pub sequence: u64,

    /// Source identifier.
    pub source: AudioDeviceType,
}

impl AudioChunk {
    /// Get the number of samples in this chunk.
    pub fn sample_count(&self) -> usize {
        self.data.len() / (std::mem::size_of::<f32>() * CHANNELS as usize)
    }

    /// Get samples as f32 slice.
    pub fn as_f32_slice(&self) -> &[f32] {
        unsafe {
            std::slice::from_raw_parts(
                self.data.as_ptr() as *const f32,
                self.data.len() / std::mem::size_of::<f32>(),
            )
        }
    }
}

/// Audio capture session for a single device.
pub struct AudioCaptureSession {
    device_id: Option<String>,
    device_type: AudioDeviceType,
    is_loopback: bool,
    capture_thread: Mutex<Option<JoinHandle<()>>>,
    should_stop: Arc<AtomicBool>,
    is_active: AtomicBool,
    chunk_receiver: Mutex<Option<Receiver<AudioChunk>>>,
}

impl AudioCaptureSession {
    /// Create a new capture session for the given device.
    pub fn new(device_id: Option<String>, device_type: AudioDeviceType) -> Self {
        let is_loopback = device_type == AudioDeviceType::Output;

        Self {
            device_id,
            device_type,
            is_loopback,
            capture_thread: Mutex::new(None),
            should_stop: Arc::new(AtomicBool::new(false)),
            is_active: AtomicBool::new(false),
            chunk_receiver: Mutex::new(None),
        }
    }

    /// Create a loopback capture session for system audio.
    pub fn new_loopback() -> Self {
        Self::new(None, AudioDeviceType::Output)
    }

    /// Create a microphone capture session.
    pub fn new_microphone(device_id: Option<String>) -> Self {
        Self::new(device_id, AudioDeviceType::Input)
    }

    /// Start capturing audio.
    #[instrument(name = "audio_capture_start", skip(self))]
    pub fn start(&mut self) -> AudioResult<Receiver<AudioChunk>> {
        if self.is_active.load(Ordering::SeqCst) {
            return Err(AudioError::AlreadyStarted);
        }

        info!(
            device_type = ?self.device_type,
            is_loopback = self.is_loopback,
            "Starting audio capture"
        );

        let (sender, receiver): (Sender<AudioChunk>, Receiver<AudioChunk>) =
            crossbeam_channel::bounded(AUDIO_CHANNEL_CAPACITY);

        let should_stop = Arc::clone(&self.should_stop);
        should_stop.store(false, Ordering::SeqCst);

        let device_id = self.device_id.clone();
        let device_type = self.device_type.clone();
        let is_loopback = self.is_loopback;

        let handle = thread::spawn(move || {
            if let Err(e) = capture_thread(device_id, device_type, is_loopback, sender, should_stop)
            {
                warn!("Audio capture thread error: {}", e);
            }
        });

        *self.capture_thread.lock() = Some(handle);
        *self.chunk_receiver.lock() = Some(receiver.clone());
        self.is_active.store(true, Ordering::SeqCst);

        Ok(receiver)
    }

    /// Stop capturing audio.
    #[instrument(name = "audio_capture_stop", skip(self))]
    pub fn stop(&mut self) -> AudioResult<()> {
        if !self.is_active.load(Ordering::SeqCst) {
            return Ok(());
        }

        info!("Stopping audio capture");

        self.should_stop.store(true, Ordering::SeqCst);

        if let Some(handle) = self.capture_thread.lock().take() {
            let _ = handle.join();
        }

        *self.chunk_receiver.lock() = None;
        self.is_active.store(false, Ordering::SeqCst);

        info!("Audio capture stopped");
        Ok(())
    }

    /// Check if capture is active.
    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::SeqCst)
    }
}

impl Drop for AudioCaptureSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn capture_thread(
    device_id: Option<String>,
    device_type: AudioDeviceType,
    is_loopback: bool,
    sender: Sender<AudioChunk>,
    should_stop: Arc<AtomicBool>,
) -> AudioResult<()> {
    // Initialize COM for this thread
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    // Get the device
    let device: IMMDevice = if let Some(ref id) = device_id {
        find_device_by_id(id)?
    } else {
        get_default_device(device_type.clone())?
    };

    // Activate audio client
    let audio_client: IAudioClient = unsafe {
        device.Activate(CLSCTX_ALL, None)?
    };

    // Get the mix format
    let mix_format = unsafe {
        audio_client.GetMixFormat()?
    };

    // Initialize the audio client
    let stream_flags = if is_loopback {
        AUDCLNT_STREAMFLAGS_LOOPBACK
    } else {
        Default::default()
    };

    unsafe {
        audio_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            stream_flags,
            10_000_000, // 1 second buffer
            0,
            mix_format,
            None,
        )?;
    }

    // Get capture client
    let capture_client: IAudioCaptureClient = unsafe {
        audio_client.GetService()?
    };

    // Start the audio client
    unsafe {
        audio_client.Start()?;
    }

    debug!("Audio capture started, entering capture loop");

    let sequence = AtomicU64::new(0);
    let start_time = Instant::now();

    // Capture loop
    while !should_stop.load(Ordering::SeqCst) {
        // Get available frames
        let mut packet_length = 0u32;
        unsafe {
            if capture_client.GetNextPacketSize(&mut packet_length).is_err() {
                break;
            }
        }

        if packet_length == 0 {
            thread::sleep(Duration::from_millis(5));
            continue;
        }

        // Get buffer
        let mut data_ptr = std::ptr::null_mut();
        let mut num_frames = 0u32;
        let mut flags = 0u32;

        unsafe {
            if capture_client
                .GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None)
                .is_err()
            {
                break;
            }
        }

        if num_frames > 0 {
            // Convert to f32 and create chunk
            let sample_count = num_frames as usize * CHANNELS as usize;
            let data = unsafe {
                std::slice::from_raw_parts(data_ptr as *const f32, sample_count)
            };

            let bytes = Bytes::copy_from_slice(unsafe {
                std::slice::from_raw_parts(
                    data.as_ptr() as *const u8,
                    sample_count * std::mem::size_of::<f32>(),
                )
            });

            let chunk = AudioChunk {
                data: bytes,
                timestamp: Instant::now(),
                sequence: sequence.fetch_add(1, Ordering::SeqCst),
                source: device_type.clone(),
            };

            // Try to send, don't block if full
            match sender.try_send(chunk) {
                Ok(()) => {}
                Err(crossbeam_channel::TrySendError::Full(_)) => {
                    trace!("Audio channel full, dropping chunk");
                }
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                    break;
                }
            }
        }

        // Release buffer
        unsafe {
            let _ = capture_client.ReleaseBuffer(num_frames);
        }
    }

    // Stop and cleanup
    unsafe {
        let _ = audio_client.Stop();
    }

    debug!("Audio capture thread exiting");
    Ok(())
}

use windows::Win32::System::Com::CLSCTX_ALL;
