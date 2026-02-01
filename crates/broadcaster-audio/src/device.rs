//! Audio device enumeration.

use tracing::{debug, instrument};
use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

use broadcaster_ipc::{AudioDevice, AudioDeviceType};

use crate::error::AudioError;
use crate::AudioResult;

/// Initialize COM for the current thread if not already initialized.
fn ensure_com_initialized() -> AudioResult<()> {
    unsafe {
        // CoInitializeEx returns S_OK if successful, S_FALSE if already initialized
        // Both are acceptable outcomes
        let result = CoInitializeEx(None, COINIT_MULTITHREADED);
        if result.is_err() && result != windows::Win32::Foundation::S_FALSE {
            return Err(AudioError::WindowsApi {
                message: "Failed to initialize COM".to_string(),
                source: None,
            });
        }
    }
    Ok(())
}

/// Enumerate all audio devices of a specific type.
#[instrument(name = "enumerate_audio_devices")]
pub fn enumerate_audio_devices() -> AudioResult<Vec<AudioDevice>> {
    ensure_com_initialized()?;

    let mut devices = Vec::new();

    // Enumerate input devices (microphones)
    let input_devices = enumerate_devices_by_type(AudioDeviceType::Input)?;
    devices.extend(input_devices);

    // Enumerate output devices (for loopback capture)
    let output_devices = enumerate_devices_by_type(AudioDeviceType::Output)?;
    devices.extend(output_devices);

    debug!(count = devices.len(), "Enumerated audio devices");
    Ok(devices)
}

fn enumerate_devices_by_type(device_type: AudioDeviceType) -> AudioResult<Vec<AudioDevice>> {
    let data_flow = match device_type {
        AudioDeviceType::Input => eCapture,
        AudioDeviceType::Output => eRender,
    };

    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };

    // Get default device ID for this type
    let default_id = unsafe {
        enumerator
            .GetDefaultAudioEndpoint(data_flow, eConsole)
            .ok()
            .and_then(|device| device.GetId().ok())
            .map(|id| id.to_string().unwrap_or_default())
    };

    // Enumerate all devices
    let collection = unsafe { enumerator.EnumAudioEndpoints(data_flow, DEVICE_STATE_ACTIVE)? };

    let count = unsafe { collection.GetCount()? };
    let mut devices = Vec::with_capacity(count as usize);

    for i in 0..count {
        let device: IMMDevice = unsafe { collection.Item(i)? };

        if let Ok(audio_device) = device_info(&device, device_type.clone(), &default_id) {
            devices.push(audio_device);
        }
    }

    Ok(devices)
}

fn device_info(
    device: &IMMDevice,
    device_type: AudioDeviceType,
    default_id: &Option<String>,
) -> AudioResult<AudioDevice> {
    let id = unsafe {
        let id_ptr = device.GetId()?;
        id_ptr.to_string().map_err(|_| AudioError::WindowsApi {
            message: "Failed to get device ID".to_string(),
            source: None,
        })?
    };

    let name = get_device_name(device).unwrap_or_else(|_| "Unknown Device".to_string());

    let is_default = default_id.as_ref().map(|d| d == &id).unwrap_or(false);

    Ok(AudioDevice {
        id,
        name,
        device_type,
        is_default,
    })
}

fn get_device_name(device: &IMMDevice) -> AudioResult<String> {
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;

    unsafe {
        let store: IPropertyStore =
            device.OpenPropertyStore(windows::Win32::System::Com::STGM_READ)?;

        // PKEY_Device_FriendlyName
        let key = windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY {
            fmtid: windows::core::GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
            pid: 14,
        };

        let prop = store.GetValue(&key)?;

        // Try to extract string from PROPVARIANT
        // The value is a VT_LPWSTR which we can read via to_string on the PROPVARIANT
        let name = prop.to_string();

        if name.is_empty() {
            Ok("Unknown".to_string())
        } else {
            Ok(name)
        }
    }
}

/// Find an audio device by its ID.
pub fn find_device_by_id(id: &str) -> AudioResult<IMMDevice> {
    ensure_com_initialized()?;

    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };

    let id_wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        enumerator
            .GetDevice(PCWSTR(id_wide.as_ptr()))
            .map_err(|_| AudioError::DeviceNotFound(id.to_string()))
    }
}

/// Get the default audio device for a type.
pub fn get_default_device(device_type: AudioDeviceType) -> AudioResult<IMMDevice> {
    ensure_com_initialized()?;

    let data_flow = match device_type {
        AudioDeviceType::Input => eCapture,
        AudioDeviceType::Output => eRender,
    };

    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };

    unsafe {
        enumerator
            .GetDefaultAudioEndpoint(data_flow, eConsole)
            .map_err(|e| AudioError::WindowsApi {
                message: "No default audio device".to_string(),
                source: Some(e),
            })
    }
}
