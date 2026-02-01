//! Monitor enumeration for capture.

use tracing::{debug, instrument};
use windows::Graphics::Capture::GraphicsCaptureItem;
use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

use broadcaster_ipc::{CaptureSource, CaptureSourceType};

use crate::error::CaptureError;
use crate::CaptureResult;

/// Monitor information for capture.
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    /// Monitor handle.
    pub handle: isize,

    /// Monitor name/device path.
    pub name: String,

    /// Monitor bounds.
    pub bounds: (i32, i32, i32, i32), // left, top, right, bottom

    /// Whether this is the primary monitor.
    pub is_primary: bool,
}

impl MonitorInfo {
    /// Get the width of the monitor.
    pub fn width(&self) -> u32 {
        (self.bounds.2 - self.bounds.0) as u32
    }

    /// Get the height of the monitor.
    pub fn height(&self) -> u32 {
        (self.bounds.3 - self.bounds.1) as u32
    }

    /// Convert to a capture source for IPC.
    pub fn to_capture_source(&self) -> CaptureSource {
        CaptureSource {
            id: format!("monitor:{}", self.handle),
            name: if self.is_primary {
                format!("{} (Primary)", self.name)
            } else {
                self.name.clone()
            },
            source_type: CaptureSourceType::Monitor,
            width: self.width(),
            height: self.height(),
        }
    }

    /// Create a GraphicsCaptureItem for this monitor.
    pub fn create_capture_item(&self) -> CaptureResult<GraphicsCaptureItem> {
        let interop: IGraphicsCaptureItemInterop =
            windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;

        let hmonitor = HMONITOR(self.handle as *mut _);
        let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(hmonitor)? };

        Ok(item)
    }
}

/// Enumerate all available monitors.
#[instrument(name = "enumerate_monitors")]
pub fn enumerate_monitors() -> CaptureResult<Vec<MonitorInfo>> {
    let mut monitors: Vec<MonitorInfo> = Vec::new();

    unsafe {
        EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(enum_monitor_callback),
            LPARAM(&mut monitors as *mut Vec<MonitorInfo> as isize),
        )
        .ok()
        .map_err(|_| CaptureError::WindowsApi {
            message: "Failed to enumerate monitors".to_string(),
            source: None,
        })?;
    }

    debug!(count = monitors.len(), "Enumerated monitors");
    Ok(monitors)
}

unsafe extern "system" fn enum_monitor_callback(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let monitors = &mut *(lparam.0 as *mut Vec<MonitorInfo>);

    let mut monitor_info = MONITORINFOEXW::default();
    monitor_info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmonitor, &mut monitor_info.monitorInfo).as_bool() {
        let name = String::from_utf16_lossy(
            &monitor_info.szDevice[..monitor_info
                .szDevice
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(monitor_info.szDevice.len())],
        );

        let info = MonitorInfo {
            handle: hmonitor.0 as isize,
            name,
            bounds: (
                monitor_info.monitorInfo.rcMonitor.left,
                monitor_info.monitorInfo.rcMonitor.top,
                monitor_info.monitorInfo.rcMonitor.right,
                monitor_info.monitorInfo.rcMonitor.bottom,
            ),
            is_primary: (monitor_info.monitorInfo.dwFlags & 1) != 0, // MONITORINFOF_PRIMARY
        };

        monitors.push(info);
    }

    BOOL::from(true)
}

/// Find a monitor by its capture source ID.
pub fn find_monitor_by_id(id: &str) -> CaptureResult<MonitorInfo> {
    let handle: isize = id
        .strip_prefix("monitor:")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| CaptureError::SourceNotFound(id.to_string()))?;

    let monitors = enumerate_monitors()?;
    monitors
        .into_iter()
        .find(|m| m.handle == handle)
        .ok_or_else(|| CaptureError::SourceNotFound(id.to_string()))
}
