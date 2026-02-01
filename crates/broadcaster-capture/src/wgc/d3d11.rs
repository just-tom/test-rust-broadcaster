//! Direct3D 11 device management for capture.

use tracing::{debug, instrument};
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;

use crate::error::CaptureError;
use crate::CaptureResult;

/// Direct3D 11 device wrapper for capture operations.
pub struct D3D11Device {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
}

impl D3D11Device {
    /// Create a new D3D11 device for capture.
    #[instrument(name = "d3d11_create_device")]
    pub fn new() -> CaptureResult<Self> {
        let mut device = None;
        let mut context = None;

        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
        }

        let device = device.ok_or_else(|| CaptureError::WindowsApi {
            message: "Failed to create D3D11 device".to_string(),
            source: None,
        })?;

        let context = context.ok_or_else(|| CaptureError::WindowsApi {
            message: "Failed to get D3D11 device context".to_string(),
            source: None,
        })?;

        debug!("Created D3D11 device for capture");
        Ok(Self { device, context })
    }

    /// Get the D3D11 device.
    pub fn device(&self) -> &ID3D11Device {
        &self.device
    }

    /// Get the device context.
    pub fn context(&self) -> &ID3D11DeviceContext {
        &self.context
    }

    /// Get the DXGI device interface.
    pub fn dxgi_device(&self) -> CaptureResult<IDXGIDevice> {
        let dxgi: IDXGIDevice = self.device.cast()?;
        Ok(dxgi)
    }
}

impl Clone for D3D11Device {
    fn clone(&self) -> Self {
        Self {
            device: self.device.clone(),
            context: self.context.clone(),
        }
    }
}
