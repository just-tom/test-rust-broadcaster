//! Window enumeration for capture.

use tracing::{debug, instrument};
use windows::Graphics::Capture::GraphicsCaptureItem;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible,
};

use broadcaster_ipc::{CaptureSource, CaptureSourceType};

use crate::error::CaptureError;
use crate::CaptureResult;

/// Window information for capture.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    /// Window handle.
    pub handle: isize,

    /// Window title.
    pub title: String,

    /// Process ID.
    pub process_id: u32,

    /// Window dimensions.
    pub width: u32,
    pub height: u32,
}

impl WindowInfo {
    /// Convert to a capture source for IPC.
    pub fn to_capture_source(&self) -> CaptureSource {
        CaptureSource {
            id: format!("window:{}", self.handle),
            name: self.title.clone(),
            source_type: CaptureSourceType::Window,
            width: self.width,
            height: self.height,
        }
    }

    /// Create a GraphicsCaptureItem for this window.
    pub fn create_capture_item(&self) -> CaptureResult<GraphicsCaptureItem> {
        let interop: IGraphicsCaptureItemInterop =
            windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;

        let hwnd = HWND(self.handle as *mut _);
        let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(hwnd)? };

        Ok(item)
    }
}

/// Enumerate all visible windows suitable for capture.
#[instrument(name = "enumerate_windows")]
pub fn enumerate_windows() -> CaptureResult<Vec<WindowInfo>> {
    let mut windows: Vec<WindowInfo> = Vec::new();

    unsafe {
        EnumWindows(
            Some(enum_window_callback),
            LPARAM(&mut windows as *mut Vec<WindowInfo> as isize),
        )
        .ok()
        .map_err(|_| CaptureError::WindowsApi {
            message: "Failed to enumerate windows".to_string(),
            source: None,
        })?;
    }

    debug!(count = windows.len(), "Enumerated windows");
    Ok(windows)
}

unsafe extern "system" fn enum_window_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = &mut *(lparam.0 as *mut Vec<WindowInfo>);

    // Skip invisible windows
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL::from(true);
    }

    // Get window title
    let title_length = GetWindowTextLengthW(hwnd);
    if title_length == 0 {
        return BOOL::from(true);
    }

    let mut title_buffer: Vec<u16> = vec![0; (title_length + 1) as usize];
    let actual_length = GetWindowTextW(hwnd, &mut title_buffer);
    if actual_length == 0 {
        return BOOL::from(true);
    }

    let title = String::from_utf16_lossy(&title_buffer[..actual_length as usize]);

    // Skip empty titles
    if title.trim().is_empty() {
        return BOOL::from(true);
    }

    // Get window rect
    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_err() {
        return BOOL::from(true);
    }

    let width = (rect.right - rect.left).max(0) as u32;
    let height = (rect.bottom - rect.top).max(0) as u32;

    // Skip very small windows
    if width < 100 || height < 100 {
        return BOOL::from(true);
    }

    // Get process ID
    let mut process_id: u32 = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut process_id));

    let info = WindowInfo {
        handle: hwnd.0 as isize,
        title,
        process_id,
        width,
        height,
    };

    windows.push(info);
    BOOL::from(true)
}

/// Find a window by its capture source ID.
pub fn find_window_by_id(id: &str) -> CaptureResult<WindowInfo> {
    let handle: isize = id
        .strip_prefix("window:")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| CaptureError::SourceNotFound(id.to_string()))?;

    let windows = enumerate_windows()?;
    windows
        .into_iter()
        .find(|w| w.handle == handle)
        .ok_or_else(|| CaptureError::SourceNotFound(id.to_string()))
}
