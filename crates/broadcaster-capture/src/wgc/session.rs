//! Capture session management.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;
use tracing::{debug, info, instrument};
use windows::Graphics::Capture::{GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;

use super::d3d11::D3D11Device;
use super::frame_pool::FramePoolManager;
use super::monitor::{find_monitor_by_id, MonitorInfo};
use super::window::{find_window_by_id, WindowInfo};
use crate::error::CaptureError;
use crate::frame::CapturedFrame;
use crate::{CaptureResult, CaptureSource, FRAME_CHANNEL_CAPACITY};

/// A capture session for screen or window capture.
pub struct CaptureSession {
    d3d_device: D3D11Device,
    direct3d_device: IDirect3DDevice,
    frame_pool_manager: Mutex<Option<Arc<FramePoolManager>>>,
    session: Mutex<Option<GraphicsCaptureSession>>,
    frame_receiver: Mutex<Option<Receiver<CapturedFrame>>>,
    is_active: AtomicBool,
    source_id: String,
    width: u32,
    height: u32,
}

impl CaptureSession {
    /// Create a new capture session for the given source.
    #[instrument(name = "capture_session_new", skip_all, fields(source_id = %source_id))]
    pub fn new(source_id: &str) -> CaptureResult<Self> {
        debug!("Creating capture session");

        // Create D3D11 device
        let d3d_device = D3D11Device::new()?;

        // Create Direct3D device for WGC
        let dxgi_device = d3d_device.dxgi_device()?;
        let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device)? };
        let direct3d_device: IDirect3DDevice = inspectable.cast()?;

        // Get source dimensions
        let (width, height) = Self::get_source_dimensions(source_id)?;

        Ok(Self {
            d3d_device,
            direct3d_device,
            frame_pool_manager: Mutex::new(None),
            session: Mutex::new(None),
            frame_receiver: Mutex::new(None),
            is_active: AtomicBool::new(false),
            source_id: source_id.to_string(),
            width,
            height,
        })
    }

    fn get_source_dimensions(source_id: &str) -> CaptureResult<(u32, u32)> {
        if source_id.starts_with("monitor:") {
            let monitor = find_monitor_by_id(source_id)?;
            Ok((monitor.width(), monitor.height()))
        } else if source_id.starts_with("window:") {
            let window = find_window_by_id(source_id)?;
            Ok((window.width, window.height))
        } else {
            Err(CaptureError::SourceNotFound(source_id.to_string()))
        }
    }

    fn create_capture_item(&self) -> CaptureResult<GraphicsCaptureItem> {
        if self.source_id.starts_with("monitor:") {
            let monitor = find_monitor_by_id(&self.source_id)?;
            monitor.create_capture_item()
        } else if self.source_id.starts_with("window:") {
            let window = find_window_by_id(&self.source_id)?;
            window.create_capture_item()
        } else {
            Err(CaptureError::SourceNotFound(self.source_id.clone()))
        }
    }
}

impl CaptureSource for CaptureSession {
    #[instrument(name = "capture_start", skip(self))]
    fn start(&mut self) -> CaptureResult<Receiver<CapturedFrame>> {
        if self.is_active.load(Ordering::SeqCst) {
            return Err(CaptureError::AlreadyStarted);
        }

        info!("Starting capture for source: {}", self.source_id);

        // Create capture item
        let item = self.create_capture_item()?;

        // Create frame channel
        let (sender, receiver): (Sender<CapturedFrame>, Receiver<CapturedFrame>) =
            crossbeam_channel::bounded(FRAME_CHANNEL_CAPACITY);

        // Create frame pool manager
        let frame_pool_manager = FramePoolManager::new(
            &item,
            self.d3d_device.device().clone(),
            self.d3d_device.context().clone(),
            &self.direct3d_device,
            sender,
        )?;

        // Create capture session
        let session = frame_pool_manager
            .frame_pool()
            .CreateCaptureSession(&item)?;

        // Disable cursor capture for cleaner output
        if let Ok(session2) = session.cast::<windows::Graphics::Capture::IGraphicsCaptureSession2>()
        {
            let _ = session2.SetIsCursorCaptureEnabled(false);
        }

        // Store components
        *self.frame_pool_manager.lock() = Some(frame_pool_manager.clone());
        *self.session.lock() = Some(session.clone());
        *self.frame_receiver.lock() = Some(receiver.clone());

        // Start capture
        frame_pool_manager.set_active(true);
        session.StartCapture()?;

        self.is_active.store(true, Ordering::SeqCst);
        info!("Capture started successfully");

        Ok(receiver)
    }

    #[instrument(name = "capture_stop", skip(self))]
    fn stop(&mut self) -> CaptureResult<()> {
        if !self.is_active.load(Ordering::SeqCst) {
            return Ok(());
        }

        info!("Stopping capture");

        // Deactivate frame pool
        if let Some(ref manager) = *self.frame_pool_manager.lock() {
            manager.set_active(false);
        }

        // Close session
        if let Some(session) = self.session.lock().take() {
            session.Close()?;
        }

        // Close frame pool
        if let Some(ref manager) = *self.frame_pool_manager.lock() {
            manager.frame_pool().Close()?;
        }

        *self.frame_pool_manager.lock() = None;
        *self.frame_receiver.lock() = None;

        self.is_active.store(false, Ordering::SeqCst);
        info!("Capture stopped");

        Ok(())
    }

    fn is_active(&self) -> bool {
        self.is_active.load(Ordering::SeqCst)
    }

    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
