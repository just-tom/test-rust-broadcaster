//! Frame pool management for WGC capture.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use crossbeam_channel::Sender;
use parking_lot::Mutex;
use tracing::{debug, trace, warn};
use windows::core::Interface;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ, D3D11_MAP_READ,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;

use crate::error::CaptureError;
use crate::frame::{CaptureTimestamp, CapturedFrame};
use crate::CaptureResult;

/// Manages the capture frame pool and frame processing.
pub struct FramePoolManager {
    frame_pool: Direct3D11CaptureFramePool,
    d3d_device: ID3D11Device,
    context: ID3D11DeviceContext,
    staging_texture: Mutex<Option<ID3D11Texture2D>>,
    frame_sender: Sender<CapturedFrame>,
    sequence: AtomicU64,
    start_time: Instant,
    is_active: AtomicBool,
    width: u32,
    height: u32,
}

impl FramePoolManager {
    /// Create a new frame pool manager.
    pub fn new(
        item: &GraphicsCaptureItem,
        d3d_device: ID3D11Device,
        context: ID3D11DeviceContext,
        direct3d_device: &IDirect3DDevice,
        frame_sender: Sender<CapturedFrame>,
    ) -> CaptureResult<Arc<Self>> {
        let size = item.Size()?;
        let width = size.Width as u32;
        let height = size.Height as u32;

        debug!(width, height, "Creating frame pool");

        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            direct3d_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2, // Number of frames in pool
            size,
        )?;

        let manager = Arc::new(Self {
            frame_pool,
            d3d_device,
            context,
            staging_texture: Mutex::new(None),
            frame_sender,
            sequence: AtomicU64::new(0),
            start_time: Instant::now(),
            is_active: AtomicBool::new(false),
            width,
            height,
        });

        // Set up frame arrived callback
        let manager_clone = Arc::clone(&manager);
        manager.frame_pool.FrameArrived(&TypedEventHandler::new(
            move |pool: &Option<Direct3D11CaptureFramePool>, _| {
                if let Some(pool) = pool {
                    if let Err(e) = manager_clone.on_frame_arrived(pool) {
                        warn!("Frame processing error: {}", e);
                    }
                }
                Ok(())
            },
        ))?;

        Ok(manager)
    }

    /// Get the frame pool for creating a capture session.
    pub fn frame_pool(&self) -> &Direct3D11CaptureFramePool {
        &self.frame_pool
    }

    /// Set the active state.
    pub fn set_active(&self, active: bool) {
        self.is_active.store(active, Ordering::SeqCst);
    }

    /// Get the actual capture dimensions from WGC.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Handle a frame arrival.
    fn on_frame_arrived(&self, pool: &Direct3D11CaptureFramePool) -> CaptureResult<()> {
        if !self.is_active.load(Ordering::SeqCst) {
            return Ok(());
        }

        let frame = pool.TryGetNextFrame()?;
        let surface = frame.Surface()?;

        // Get timestamp
        let timestamp = CaptureTimestamp::now(self.start_time);
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);

        trace!(sequence, "Processing captured frame");

        // Convert to NV12 and send
        let data = self.convert_to_nv12(&surface)?;
        let captured_frame = CapturedFrame::new(data, self.width, self.height, timestamp, sequence);

        // Try to send, drop if channel is full (backpressure)
        match self.frame_sender.try_send(captured_frame) {
            Ok(()) => {
                debug!(
                    "Captured frame #{}: {}x{}",
                    sequence, self.width, self.height
                );
            }
            Err(crossbeam_channel::TrySendError::Full(_)) => {
                debug!("Frame channel full, dropping frame #{}", sequence);
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                return Err(CaptureError::ChannelDisconnected);
            }
        }

        Ok(())
    }

    /// Convert a captured surface to NV12 format.
    fn convert_to_nv12(
        &self,
        surface: &windows::Graphics::DirectX::Direct3D11::IDirect3DSurface,
    ) -> CaptureResult<Bytes> {
        // Get the D3D11 texture from the surface
        let access: windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess =
            surface.cast()?;
        let texture: ID3D11Texture2D = unsafe { access.GetInterface()? };

        // Ensure we have a staging texture
        let mut staging_lock = self.staging_texture.lock();
        let staging = if let Some(ref staging) = *staging_lock {
            staging.clone()
        } else {
            let staging = self.create_staging_texture()?;
            *staging_lock = Some(staging.clone());
            staging
        };
        drop(staging_lock);

        // Copy to staging texture
        unsafe {
            self.context.CopyResource(&staging, &texture);
        }

        // Map the staging texture for CPU read
        let mapped = unsafe {
            let mut mapped = std::mem::zeroed();
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            mapped
        };

        // Read BGRA data and convert to NV12
        let bgra_data = unsafe {
            std::slice::from_raw_parts(
                mapped.pData as *const u8,
                (mapped.RowPitch * self.height) as usize,
            )
        };

        let nv12_data = self.bgra_to_nv12(bgra_data, mapped.RowPitch as usize);

        unsafe {
            self.context.Unmap(&staging, 0);
        }

        Ok(Bytes::from(nv12_data))
    }

    /// Create a staging texture for CPU readback.
    fn create_staging_texture(&self) -> CaptureResult<ID3D11Texture2D> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: self.width,
            Height: self.height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: Default::default(),
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: Default::default(),
        };

        let mut texture = None;
        unsafe {
            self.d3d_device
                .CreateTexture2D(&desc, None, Some(&mut texture))?;
        }

        texture.ok_or_else(|| CaptureError::WindowsApi {
            message: "Failed to create staging texture".to_string(),
            source: None,
        })
    }

    /// Convert BGRA to NV12 format.
    fn bgra_to_nv12(&self, bgra: &[u8], row_pitch: usize) -> Vec<u8> {
        let w = self.width as usize;
        let h = self.height as usize;

        // NV12: Y plane followed by interleaved UV plane
        let y_size = w * h;
        let uv_size = y_size / 2;
        let mut nv12 = vec![0u8; y_size + uv_size];

        // Y plane
        for y in 0..h {
            for x in 0..w {
                let src_offset = y * row_pitch + x * 4;
                let b = bgra[src_offset] as f32;
                let g = bgra[src_offset + 1] as f32;
                let r = bgra[src_offset + 2] as f32;

                // BT.601 conversion
                let y_val = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
                nv12[y * w + x] = y_val;
            }
        }

        // UV plane (subsampled 2x2)
        let uv_offset = y_size;
        for y in (0..h).step_by(2) {
            for x in (0..w).step_by(2) {
                let src_offset = y * row_pitch + x * 4;
                let b = bgra[src_offset] as f32;
                let g = bgra[src_offset + 1] as f32;
                let r = bgra[src_offset + 2] as f32;

                // BT.601 conversion
                let u = ((-0.169 * r - 0.331 * g + 0.500 * b) + 128.0).clamp(0.0, 255.0) as u8;
                let v = ((0.500 * r - 0.419 * g - 0.081 * b) + 128.0).clamp(0.0, 255.0) as u8;

                let uv_idx = uv_offset + (y / 2) * w + x;
                nv12[uv_idx] = u;
                nv12[uv_idx + 1] = v;
            }
        }

        nv12
    }

    /// Recreate the frame pool with a new size.
    #[allow(dead_code)]
    pub fn recreate(&self, new_size: SizeInt32, device: &IDirect3DDevice) -> CaptureResult<()> {
        self.frame_pool.Recreate(
            device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            new_size,
        )?;
        Ok(())
    }
}
