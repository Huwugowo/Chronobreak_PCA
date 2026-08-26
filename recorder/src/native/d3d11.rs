use std::marker::PhantomData;

use anyhow::{Context, Result, bail};
use windows::Graphics::Capture::Direct3D11CaptureFrame;
use windows::Graphics::DirectX::Direct3D11::{IDirect3DDevice, IDirect3DSurface};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11CreateDevice, ID3D11Device, ID3D11Texture2D, ID3D11VideoContext1,
    ID3D11VideoDevice,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_NOT_FOUND, IDXGIAdapter1, IDXGIDevice, IDXGIFactory1,
};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::core::Interface;

use super::winrt::WinRtMtaGuard;

/// One explicitly selected D3D11 device for native capture, conversion and
/// NVENC registration. The adapter identity is checked against the existing
/// CaptureTarget instead of guessed from GPU names or vendor IDs.
pub struct NativeD3d11Device {
    adapter_index: u32,
    adapter_luid: u64,
    adapter_name: String,
    device: ID3D11Device,
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext1,
    winrt_device: IDirect3DDevice,
    feature_level: D3D_FEATURE_LEVEL,
}

impl NativeD3d11Device {
    pub(crate) fn for_luid(_winrt: &WinRtMtaGuard, expected_luid: u64) -> Result<Self> {
        // SAFETY: CreateDXGIFactory1 initializes and returns an owned COM
        // interface; `windows` manages its reference count.
        let factory: IDXGIFactory1 =
            unsafe { CreateDXGIFactory1() }.context("could not create native DXGI factory")?;
        let (adapter_index, adapter) = adapter_by_luid(&factory, expected_luid)?;
        // SAFETY: `adapter` is a live interface returned by this factory.
        let description =
            unsafe { adapter.GetDesc1() }.context("could not query native DXGI adapter")?;
        let actual_luid = luid_value(
            description.AdapterLuid.LowPart,
            description.AdapterLuid.HighPart,
        );
        debug_assert_eq!(actual_luid, expected_luid);

        let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
        let mut device = None;
        let mut context = None;
        let mut feature_level = D3D_FEATURE_LEVEL::default();
        // SAFETY: all out-pointers refer to initialized local `Option`s, the
        // feature-level slice remains live for the call, and UNKNOWN requires
        // the non-null hardware adapter supplied above. No WARP fallback is
        // requested.
        unsafe {
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut feature_level),
                Some(&mut context),
            )
        }
        .context("could not create the same-adapter native D3D11 device")?;
        if feature_level.0 < D3D_FEATURE_LEVEL_11_0.0 {
            bail!(
                "native recorder requires D3D feature level 11.0 or newer, got 0x{:x}",
                feature_level.0
            );
        }
        let device = device.context("D3D11CreateDevice returned no device")?;
        let context = context.context("D3D11CreateDevice returned no immediate context")?;
        let video_device: ID3D11VideoDevice = device
            .cast()
            .context("same-adapter D3D11 device has no video-device support")?;
        let video_context: ID3D11VideoContext1 = context
            .cast()
            .context("same-adapter D3D11 context has no color-space-aware video support")?;
        let dxgi_device: IDXGIDevice = device
            .cast()
            .context("could not cast native D3D11 device to IDXGIDevice")?;
        // SAFETY: `dxgi_device` is a live D3D11-backed IDXGIDevice and the
        // returned inspectable is immediately retained by a WinRT interface.
        let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device) }
            .context("could not wrap native D3D11 device for Windows Graphics Capture")?;
        let winrt_device: IDirect3DDevice = inspectable
            .cast()
            .context("could not cast native WGC device wrapper")?;

        Ok(Self {
            adapter_index,
            adapter_luid: actual_luid,
            adapter_name: wide_string(&description.Description),
            device,
            video_device,
            video_context,
            winrt_device,
            feature_level,
        })
    }

    pub fn adapter_index(&self) -> u32 {
        self.adapter_index
    }

    pub fn adapter_luid(&self) -> u64 {
        self.adapter_luid
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    pub fn feature_level(&self) -> D3D_FEATURE_LEVEL {
        self.feature_level
    }

    pub fn device(&self) -> &ID3D11Device {
        &self.device
    }

    pub(crate) fn video_device(&self) -> &ID3D11VideoDevice {
        &self.video_device
    }

    pub(crate) fn video_context(&self) -> &ID3D11VideoContext1 {
        &self.video_context
    }

    pub(crate) fn winrt_device(&self) -> &IDirect3DDevice {
        &self.winrt_device
    }
}

/// GPU-resident view of one WGC source texture. The private lifetime marker and
/// non-cloneable API prevent safe code from detaching the texture from the WGC
/// frame that owns the recyclable pool surface.
pub(super) struct NativeSourceTexture<'frame> {
    texture: ID3D11Texture2D,
    desc: D3D11_TEXTURE2D_DESC,
    _frame: PhantomData<&'frame Direct3D11CaptureFrame>,
}

impl<'frame> NativeSourceTexture<'frame> {
    pub(super) fn from_frame(frame: &'frame Direct3D11CaptureFrame) -> Result<Self> {
        let surface: IDirect3DSurface = frame
            .Surface()
            .context("native WGC frame has no Direct3D surface")?;
        let access: IDirect3DDxgiInterfaceAccess = surface
            .cast()
            .context("WGC surface does not expose DXGI interface access")?;
        // SAFETY: the WinRT surface is live for this call and the requested
        // interface is ID3D11Texture2D. The returned COM reference is retained
        // only inside a value lifetime-bound to `frame`.
        let texture: ID3D11Texture2D =
            unsafe { access.GetInterface() }.context("could not access WGC D3D11 texture")?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: `desc` is a valid writable descriptor and `texture` is live.
        unsafe { texture.GetDesc(&mut desc) };
        Ok(Self {
            texture,
            desc,
            _frame: PhantomData,
        })
    }

    pub(super) fn texture(&self) -> &ID3D11Texture2D {
        &self.texture
    }

    pub(super) fn desc(&self) -> D3D11_TEXTURE2D_DESC {
        self.desc
    }
}

fn adapter_by_luid(factory: &IDXGIFactory1, expected_luid: u64) -> Result<(u32, IDXGIAdapter1)> {
    let mut index = 0_u32;
    loop {
        // SAFETY: `factory` is live and DXGI validates the enumeration index.
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(error).context("could not enumerate native DXGI adapters"),
        };
        // SAFETY: `adapter` is a live interface returned by `factory`.
        let description =
            unsafe { adapter.GetDesc1() }.context("could not query native DXGI adapter")?;
        let luid = luid_value(
            description.AdapterLuid.LowPart,
            description.AdapterLuid.HighPart,
        );
        if luid == expected_luid {
            return Ok((index, adapter));
        }
        index = index
            .checked_add(1)
            .context("DXGI adapter enumeration index overflowed")?;
    }
    bail!("no DXGI adapter matches target LUID {expected_luid:016x}")
}

fn luid_value(low: u32, high: i32) -> u64 {
    ((high as u32 as u64) << 32) | u64::from(low)
}

fn wide_string(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combines_signed_luid_halves_without_losing_bits() {
        assert_eq!(luid_value(0x89ab_cdef, -2), 0xffff_fffe_89ab_cdef);
    }
}
