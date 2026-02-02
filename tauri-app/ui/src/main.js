import { invoke } from '@tauri-apps/api/core';

// Application state
let state = {
  isLive: false,
  isStarting: false,
  captureSources: [],
  audioDevices: [],
  micMuted: false,
  systemMuted: false,
};

// DOM Elements
const elements = {
  status: document.getElementById('status'),
  captureSource: document.getElementById('capture-source'),
  refreshSources: document.getElementById('refresh-sources'),
  rtmpUrl: document.getElementById('rtmp-url'),
  streamKey: document.getElementById('stream-key'),
  micDevice: document.getElementById('mic-device'),
  micVolume: document.getElementById('mic-volume'),
  micVolumeValue: document.getElementById('mic-volume-value'),
  micMute: document.getElementById('mic-mute'),
  systemVolume: document.getElementById('system-volume'),
  systemVolumeValue: document.getElementById('system-volume-value'),
  systemMute: document.getElementById('system-mute'),
  streamBtn: document.getElementById('stream-btn'),
  metrics: document.getElementById('metrics'),
  metricFps: document.getElementById('metric-fps'),
  metricBitrate: document.getElementById('metric-bitrate'),
  metricDropped: document.getElementById('metric-dropped'),
  metricUptime: document.getElementById('metric-uptime'),
  encoderInfo: document.getElementById('encoder-info'),
};

// Initialize the application
async function init() {
  setupEventListeners();
  await refreshSources();
  await refreshAudioDevices();
  startEventPolling();
}

// Setup event listeners
function setupEventListeners() {
  elements.refreshSources.addEventListener('click', refreshSources);
  elements.streamBtn.addEventListener('click', toggleStream);

  elements.micVolume.addEventListener('input', handleMicVolumeChange);
  elements.systemVolume.addEventListener('input', handleSystemVolumeChange);
  elements.micMute.addEventListener('click', toggleMicMute);
  elements.systemMute.addEventListener('click', toggleSystemMute);
}

// Refresh capture sources
async function refreshSources() {
  try {
    console.log('Requesting capture sources...');
    await invoke('get_capture_sources');
    await invoke('get_audio_devices');
    // Sources will arrive via event polling
  } catch (e) {
    console.error('Failed to request sources:', e);
  }
}

// Refresh audio devices
async function refreshAudioDevices() {
  try {
    await invoke('get_audio_devices');
    // Devices will arrive via event polling
  } catch (e) {
    console.error('Failed to request audio devices:', e);
  }
}

// Toggle stream on/off
async function toggleStream() {
  if (state.isLive || state.isStarting) {
    await stopStream();
  } else {
    await startStream();
  }
}

// Start streaming
async function startStream() {
  const captureSource = elements.captureSource.value;
  const rtmpUrl = elements.rtmpUrl.value.trim();
  const streamKey = elements.streamKey.value.trim();

  if (!captureSource) {
    alert('Please select a capture source');
    return;
  }

  if (!rtmpUrl) {
    alert('Please enter an RTMP URL');
    return;
  }

  if (!streamKey) {
    alert('Please enter a stream key');
    return;
  }

  const config = {
    rtmp_url: rtmpUrl,
    stream_key: streamKey,
    capture_source: captureSource,
    mic_device: elements.micDevice.value || null,
    mic_volume: parseInt(elements.micVolume.value) / 100,
    system_volume: parseInt(elements.systemVolume.value) / 100,
    video_bitrate_kbps: 6000,
    audio_bitrate_kbps: 128,
  };

  try {
    await invoke('start_stream', { config });
    state.isStarting = true;
    updateUI();
  } catch (e) {
    console.error('Failed to start stream:', e);
    alert('Failed to start stream: ' + e);
  }
}

// Stop streaming
async function stopStream() {
  try {
    await invoke('stop_stream');
  } catch (e) {
    console.error('Failed to stop stream:', e);
  }
}

// Handle mic volume change
async function handleMicVolumeChange() {
  const volume = parseInt(elements.micVolume.value) / 100;
  elements.micVolumeValue.textContent = `${elements.micVolume.value}%`;

  try {
    await invoke('set_mic_volume', { volume });
  } catch (e) {
    console.error('Failed to set mic volume:', e);
  }
}

// Handle system volume change
async function handleSystemVolumeChange() {
  const volume = parseInt(elements.systemVolume.value) / 100;
  elements.systemVolumeValue.textContent = `${elements.systemVolume.value}%`;

  try {
    await invoke('set_system_volume', { volume });
  } catch (e) {
    console.error('Failed to set system volume:', e);
  }
}

// Toggle mic mute
async function toggleMicMute() {
  state.micMuted = !state.micMuted;
  elements.micMute.classList.toggle('muted', state.micMuted);
  elements.micMute.innerHTML = state.micMuted ? '&#x1F507;' : '&#x1F50A;';

  try {
    await invoke('set_mic_muted', { muted: state.micMuted });
  } catch (e) {
    console.error('Failed to toggle mic mute:', e);
  }
}

// Toggle system mute
async function toggleSystemMute() {
  state.systemMuted = !state.systemMuted;
  elements.systemMute.classList.toggle('muted', state.systemMuted);
  elements.systemMute.innerHTML = state.systemMuted ? '&#x1F507;' : '&#x1F50A;';

  try {
    await invoke('set_system_muted', { muted: state.systemMuted });
  } catch (e) {
    console.error('Failed to toggle system mute:', e);
  }
}

// Poll for engine events
async function pollEvents() {
  try {
    const events = await invoke('poll_events');
    for (const event of events) {
      handleEvent(event);
    }
  } catch (e) {
    console.error('Failed to poll events:', e);
  }
}

// Handle engine events
function handleEvent(event) {
  console.log('Event:', event);

  if (event.StateChanged) {
    handleStateChange(event.StateChanged.current);
  } else if (event.Metrics) {
    handleMetrics(event.Metrics);
  } else if (event.CaptureSources) {
    handleCaptureSources(event.CaptureSources);
  } else if (event.AudioDevices) {
    handleAudioDevices(event.AudioDevices);
  } else if (event.PerformanceWarning) {
    handleWarning(event.PerformanceWarning);
  } else if (event.Error) {
    handleError(event.Error);
  }
}

// Handle state changes
function handleStateChange(engineState) {
  if (engineState === 'Idle') {
    state.isLive = false;
    state.isStarting = false;
  } else if (engineState.Starting) {
    state.isLive = false;
    state.isStarting = true;
  } else if (engineState.Live) {
    state.isLive = true;
    state.isStarting = false;
  } else if (engineState.Stopping) {
    state.isLive = false;
    state.isStarting = false;
  } else if (engineState.Error) {
    state.isLive = false;
    state.isStarting = false;
    alert('Error: ' + engineState.Error.message);
  }

  updateUI();
}

// Handle metrics update
function handleMetrics(metrics) {
  elements.metricFps.textContent = `${metrics.fps.toFixed(1)} / ${metrics.target_fps}`;
  elements.metricBitrate.textContent = `${metrics.bitrate_kbps} kbps`;
  elements.metricDropped.textContent = metrics.dropped_frames.toString();
  elements.metricUptime.textContent = formatUptime(metrics.uptime_seconds);
}

// Handle capture sources update
function handleCaptureSources(sources) {
  state.captureSources = sources;

  const currentValue = elements.captureSource.value;
  elements.captureSource.innerHTML = '<option value="">Select a source...</option>';

  for (const source of sources) {
    const option = document.createElement('option');
    option.value = source.id;
    option.textContent = `${source.name} (${source.width}x${source.height})`;
    elements.captureSource.appendChild(option);
  }

  if (currentValue && sources.find(s => s.id === currentValue)) {
    elements.captureSource.value = currentValue;
  }
}

// Handle audio devices update
function handleAudioDevices(devices) {
  state.audioDevices = devices;

  const inputDevices = devices.filter(d => d.device_type === 'Input');

  const currentValue = elements.micDevice.value;
  elements.micDevice.innerHTML = '<option value="">No microphone</option>';

  for (const device of inputDevices) {
    const option = document.createElement('option');
    option.value = device.id;
    option.textContent = device.name + (device.is_default ? ' (Default)' : '');
    elements.micDevice.appendChild(option);
  }

  if (currentValue && inputDevices.find(d => d.id === currentValue)) {
    elements.micDevice.value = currentValue;
  }
}

// Handle performance warning
function handleWarning(warning) {
  console.warn('Performance warning:', warning);
}

// Handle error
function handleError(error) {
  console.error('Engine error:', error);
  if (!error.recoverable) {
    alert('Fatal error: ' + error.message);
  }
}

// Update UI based on state
function updateUI() {
  // Update status indicator
  elements.status.className = 'status';
  if (state.isLive) {
    elements.status.classList.add('live');
    elements.status.textContent = 'Live';
  } else if (state.isStarting) {
    elements.status.classList.add('starting');
    elements.status.textContent = 'Starting...';
  } else {
    elements.status.classList.add('idle');
    elements.status.textContent = 'Idle';
  }

  // Update stream button
  if (state.isLive) {
    elements.streamBtn.textContent = 'Stop Stream';
    elements.streamBtn.classList.add('live');
  } else if (state.isStarting) {
    elements.streamBtn.textContent = 'Cancel';
    elements.streamBtn.classList.remove('live');
  } else {
    elements.streamBtn.textContent = 'Go Live';
    elements.streamBtn.classList.remove('live');
  }
  // Button is NEVER disabled - user can always stop/cancel
  elements.streamBtn.disabled = false;

  // Show/hide metrics
  elements.metrics.style.display = state.isLive ? 'block' : 'none';

  // Disable inputs while live
  const isDisabled = state.isLive || state.isStarting;
  elements.captureSource.disabled = isDisabled;
  elements.rtmpUrl.disabled = isDisabled;
  elements.streamKey.disabled = isDisabled;
  elements.micDevice.disabled = isDisabled;
}

// Format uptime as HH:MM:SS
function formatUptime(seconds) {
  const hrs = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  const secs = seconds % 60;
  return `${hrs.toString().padStart(2, '0')}:${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
}

// Start event polling
function startEventPolling() {
  setInterval(pollEvents, 100);
}

// Initialize when DOM is ready
document.addEventListener('DOMContentLoaded', init);
