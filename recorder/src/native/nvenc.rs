use std::ffi::c_void;
use std::ptr::NonNull;

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows::core::{GUID, Interface, PCWSTR};

use super::source::NativeWgcSource;

pub(super) const NVENC_API_MAJOR_VERSION: u32 = 12;
pub(super) const NVENC_API_MINOR_VERSION: u32 = 2;
pub(super) const NVENC_API_VERSION: u32 = NVENC_API_MAJOR_VERSION | (NVENC_API_MINOR_VERSION << 24);
const NVENC_STRUCT_VERSION_BASE: u32 = 0x7000_0000;
const NVENC_FUNCTION_LIST_VERSION: u32 = nvenc_struct_version(2);
const NVENC_OPEN_SESSION_VERSION: u32 = nvenc_struct_version(1);
const NVENC_CAPS_PARAM_VERSION: u32 = nvenc_struct_version(1);
const NVENC_SUCCESS: i32 = 0;
const NVENC_DEVICE_TYPE_DIRECTX: i32 = 0;
const NVENC_MAX_CODEC_GUIDS: u32 = 64;
const NVENC_CAPS_WIDTH_MAX: i32 = 16;
const NVENC_CAPS_HEIGHT_MAX: i32 = 17;
const NVENC_CAPS_ASYNC_ENCODE_SUPPORT: i32 = 30;
const NVENC_CAPS_MB_NUM_MAX: i32 = 31;
const NVENC_CAPS_MB_PER_SEC_MAX: i32 = 32;
const REQUIRED_WIDTH: u32 = 1920;
const REQUIRED_HEIGHT: u32 = 1080;
const REQUIRED_MACROBLOCKS_PER_FRAME: u32 = 120 * 68;
const REQUIRED_MACROBLOCKS_PER_SECOND: u32 = REQUIRED_MACROBLOCKS_PER_FRAME * 60;
pub(super) const H264_GUID: GUID = GUID::from_values(
    0x6bc8_2762,
    0x4e63,
    0x4ca4,
    [0xaa, 0x85, 0x1e, 0x50, 0xf3, 0x21, 0xf6, 0xbf],
);

const FUNCTION_GET_GUID_COUNT: usize = 1;
const FUNCTION_GET_GUIDS: usize = 4;
const FUNCTION_GET_CAPS: usize = 7;
const FUNCTION_DESTROY_ENCODER: usize = 27;
const FUNCTION_OPEN_SESSION_EX: usize = 29;

pub(super) const fn nvenc_struct_version(version: u32) -> u32 {
    NVENC_API_VERSION | (version << 16) | NVENC_STRUCT_VERSION_BASE
}

// NvEncodeAPIGetMaxSupportedVersion encodes the version as
// (major << 4) | minor. This differs from the NVENCAPI_VERSION value passed
// in versioned API structures.
type GetMaxSupportedVersion = unsafe extern "system" fn(*mut u32) -> i32;
type CreateInstance = unsafe extern "system" fn(*mut NvencFunctionList) -> i32;
type GetEncodeGuidCount = unsafe extern "system" fn(*mut c_void, *mut u32) -> i32;
type GetEncodeGuids = unsafe extern "system" fn(*mut c_void, *mut GUID, u32, *mut u32) -> i32;
type GetEncodeCaps =
    unsafe extern "system" fn(*mut c_void, GUID, *mut NvencCapsParam, *mut i32) -> i32;
type DestroyEncoder = unsafe extern "system" fn(*mut c_void) -> i32;
type OpenEncodeSessionEx =
    unsafe extern "system" fn(*mut NvencOpenSessionParams, *mut *mut c_void) -> i32;

#[repr(C)]
pub(super) struct NvencFunctionList {
    version: u32,
    reserved: u32,
    functions: [*mut c_void; 43],
    reserved2: [*mut c_void; 275],
}

impl NvencFunctionList {
    fn new() -> Self {
        Self {
            version: NVENC_FUNCTION_LIST_VERSION,
            reserved: 0,
            functions: [std::ptr::null_mut(); 43],
            reserved2: [std::ptr::null_mut(); 275],
        }
    }

    fn get_encode_guid_count(&self) -> Result<GetEncodeGuidCount> {
        let pointer = self.required_function(FUNCTION_GET_GUID_COUNT, "nvEncGetEncodeGUIDCount")?;
        // SAFETY: pinned header 12.2 places PNVENCGETENCODEGUIDCOUNT at this
        // index and all function pointers have pointer width on Win64.
        Ok(unsafe { std::mem::transmute::<*mut c_void, GetEncodeGuidCount>(pointer) })
    }

    fn get_encode_guids(&self) -> Result<GetEncodeGuids> {
        let pointer = self.required_function(FUNCTION_GET_GUIDS, "nvEncGetEncodeGUIDs")?;
        // SAFETY: pinned header 12.2 places PNVENCGETENCODEGUIDS at this index.
        Ok(unsafe { std::mem::transmute::<*mut c_void, GetEncodeGuids>(pointer) })
    }

    fn get_encode_caps(&self) -> Result<GetEncodeCaps> {
        let pointer = self.required_function(FUNCTION_GET_CAPS, "nvEncGetEncodeCaps")?;
        // SAFETY: pinned header 12.2 places PNVENCGETENCODECAPS at this index.
        Ok(unsafe { std::mem::transmute::<*mut c_void, GetEncodeCaps>(pointer) })
    }

    fn destroy_encoder(&self) -> Result<DestroyEncoder> {
        let pointer = self.required_function(FUNCTION_DESTROY_ENCODER, "nvEncDestroyEncoder")?;
        // SAFETY: pinned header 12.2 places PNVENCDESTROYENCODER at this index.
        Ok(unsafe { std::mem::transmute::<*mut c_void, DestroyEncoder>(pointer) })
    }

    fn open_encode_session_ex(&self) -> Result<OpenEncodeSessionEx> {
        let pointer =
            self.required_function(FUNCTION_OPEN_SESSION_EX, "nvEncOpenEncodeSessionEx")?;
        // SAFETY: pinned header 12.2 places PNVENCOPENENCODESESSIONEX at this
        // index and the parameter structs below reproduce its C layout.
        Ok(unsafe { std::mem::transmute::<*mut c_void, OpenEncodeSessionEx>(pointer) })
    }

    pub(super) fn required_function(&self, index: usize, name: &str) -> Result<*mut c_void> {
        self.functions
            .get(index)
            .copied()
            .filter(|pointer| !pointer.is_null())
            .with_context(|| format!("NVENC function list has no {name}"))
    }
}

#[repr(C)]
pub(super) struct NvencOpenSessionParams {
    version: u32,
    device_type: i32,
    device: *mut c_void,
    reserved: *mut c_void,
    api_version: u32,
    reserved1: [u32; 253],
    reserved2: [*mut c_void; 64],
}

impl NvencOpenSessionParams {
    pub(super) fn directx(device: *mut c_void) -> Self {
        Self {
            version: NVENC_OPEN_SESSION_VERSION,
            device_type: NVENC_DEVICE_TYPE_DIRECTX,
            device,
            reserved: std::ptr::null_mut(),
            api_version: NVENC_API_VERSION,
            reserved1: [0; 253],
            reserved2: [std::ptr::null_mut(); 64],
        }
    }
}

#[repr(C)]
struct NvencCapsParam {
    version: u32,
    caps_to_query: i32,
    reserved: [u32; 62],
}

impl NvencCapsParam {
    fn new(caps_to_query: i32) -> Self {
        Self {
            version: NVENC_CAPS_PARAM_VERSION,
            caps_to_query,
            reserved: [0; 62],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NvencApiVersion {
    pub major: u32,
    pub minor: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvencH264Capability {
    pub max_width: u32,
    pub max_height: u32,
    pub max_macroblocks_per_frame: u32,
    pub max_macroblocks_per_second: u32,
    pub async_encode_supported: bool,
}

impl NvencApiVersion {
    fn decode(raw: u32) -> Self {
        Self {
            major: raw >> 4,
            minor: raw & 0x0f,
        }
    }
}

/// Dynamically loaded pinned NVENC 12.2 prerequisite surface.
pub struct NvencDriverProbe {
    module: HMODULE,
    version: NvencApiVersion,
}

impl NvencDriverProbe {
    pub fn load() -> Result<Self> {
        let wide: Vec<u16> = "nvEncodeAPI64.dll\0".encode_utf16().collect();
        // SAFETY: `wide` is NUL-terminated and remains live for the call. The
        // restricted search flag prevents resolving an attacker-controlled DLL
        // from the working directory or PATH.
        let module =
            unsafe { LoadLibraryExW(PCWSTR(wide.as_ptr()), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
                .context("nvEncodeAPI64.dll is unavailable")?;

        let result = (|| -> Result<NvencApiVersion> {
            // SAFETY: `module` is live and the symbol name is a static,
            // NUL-terminated string.
            let max_version_symbol = unsafe {
                GetProcAddress(
                    module,
                    windows::core::s!("NvEncodeAPIGetMaxSupportedVersion"),
                )
            }
            .context("NvEncodeAPIGetMaxSupportedVersion is unavailable")?;
            // SAFETY: same live module and static symbol-name invariants.
            unsafe { GetProcAddress(module, windows::core::s!("NvEncodeAPICreateInstance")) }
                .context("NvEncodeAPICreateInstance is unavailable")?;

            // SAFETY: NVIDIA specifies this exact stdcall/system ABI and
            // signature for NvEncodeAPIGetMaxSupportedVersion on Windows.
            let get_max_version: GetMaxSupportedVersion =
                unsafe { std::mem::transmute(max_version_symbol) };
            let mut raw = 0_u32;
            // SAFETY: `raw` is a valid writable u32 for the duration of the
            // driver call and the function pointer was validated above.
            let status = unsafe { get_max_version(&mut raw) };
            if status != 0 {
                bail!("NvEncodeAPIGetMaxSupportedVersion failed with NVENCSTATUS {status}");
            }
            Ok(NvencApiVersion::decode(raw))
        })();

        match result {
            Ok(version) => Ok(Self { module, version }),
            Err(error) => {
                // SAFETY: `module` was returned by LoadLibraryExW and ownership
                // has not been transferred or released.
                unsafe {
                    let _ = FreeLibrary(module);
                }
                Err(error)
            }
        }
    }

    pub fn version(&self) -> NvencApiVersion {
        self.version
    }

    pub(super) fn ensure_required_api(&self) -> Result<()> {
        let required = NvencApiVersion {
            major: NVENC_API_MAJOR_VERSION,
            minor: NVENC_API_MINOR_VERSION,
        };
        if self.version < required {
            bail!(
                "NVIDIA driver supports NVENC API {}.{}, but Chronobreak requires {}.{}",
                self.version.major,
                self.version.minor,
                required.major,
                required.minor
            );
        }
        Ok(())
    }

    /// Open a temporary NVENC session on the exact D3D11 device owned by the
    /// native WGC source and prove the H.264/1080p60 asynchronous prerequisites.
    /// Production encoding validates the same capability contract on its real
    /// session instead; this temporary path exists for the standalone source
    /// diagnostic probe.
    pub fn probe_h264_on_source(&self, source: &NativeWgcSource) -> Result<NvencH264Capability> {
        self.ensure_required_api()?;

        let functions = self.create_function_list()?;
        let open_session = functions.open_encode_session_ex()?;
        let destroy_encoder = functions.destroy_encoder()?;
        let mut params = NvencOpenSessionParams::directx(source.device().device().as_raw());
        let mut encoder = std::ptr::null_mut();
        // SAFETY: the parameter layout is verified against pinned header 12.2,
        // the D3D11 device remains owned by `source`, and `encoder` is a valid
        // writable out-pointer.
        let status = unsafe { open_session(&mut params, &mut encoder) };
        nvenc_status(
            status,
            "nvEncOpenEncodeSessionEx on the capture D3D11 device",
        )?;
        let encoder = NonNull::new(encoder)
            .context("nvEncOpenEncodeSessionEx succeeded without an encoder handle")?;
        let session = NvencProbeSession {
            encoder: Some(encoder),
            destroy_encoder,
        };
        let capability = validate_h264_session(&functions, session.handle())?;
        session.close()?;
        Ok(capability)
    }

    pub(super) fn create_function_list(&self) -> Result<NvencFunctionList> {
        // SAFETY: `module` is live and the symbol name is static and
        // NUL-terminated.
        let symbol =
            unsafe { GetProcAddress(self.module, windows::core::s!("NvEncodeAPICreateInstance")) }
                .context("NvEncodeAPICreateInstance is unavailable")?;
        // SAFETY: NVIDIA specifies this exact stdcall/system ABI and signature
        // for NvEncodeAPICreateInstance on Windows.
        let create_instance: CreateInstance = unsafe { std::mem::transmute(symbol) };
        let mut functions = NvencFunctionList::new();
        // SAFETY: `functions` has the pinned 12.2 C layout, version field and
        // writable storage required by NvEncodeAPICreateInstance.
        let status = unsafe { create_instance(&mut functions) };
        nvenc_status(status, "NvEncodeAPICreateInstance")?;
        Ok(functions)
    }
}

pub(super) fn validate_h264_session(
    functions: &NvencFunctionList,
    encoder: *mut c_void,
) -> Result<NvencH264Capability> {
    let codecs = supported_codec_guids(functions, encoder)?;
    if !codecs.contains(&H264_GUID) {
        bail!("same-device NVENC session does not advertise H.264 encoding");
    }

    let max_width = query_u32_cap(
        functions,
        encoder,
        NVENC_CAPS_WIDTH_MAX,
        "maximum H.264 width",
    )?;
    let max_height = query_u32_cap(
        functions,
        encoder,
        NVENC_CAPS_HEIGHT_MAX,
        "maximum H.264 height",
    )?;
    let max_macroblocks_per_frame = query_u32_cap(
        functions,
        encoder,
        NVENC_CAPS_MB_NUM_MAX,
        "maximum H.264 macroblocks per frame",
    )?;
    let max_macroblocks_per_second = query_u32_cap(
        functions,
        encoder,
        NVENC_CAPS_MB_PER_SEC_MAX,
        "maximum H.264 macroblocks per second",
    )?;
    let async_encode_supported = query_u32_cap(
        functions,
        encoder,
        NVENC_CAPS_ASYNC_ENCODE_SUPPORT,
        "asynchronous H.264 encode support",
    )? != 0;

    if max_width < REQUIRED_WIDTH
        || max_height < REQUIRED_HEIGHT
        || max_macroblocks_per_frame < REQUIRED_MACROBLOCKS_PER_FRAME
        || max_macroblocks_per_second < REQUIRED_MACROBLOCKS_PER_SECOND
        || !async_encode_supported
    {
        bail!(
            "same-device NVENC H.264 capability is below 1920x1080@60 async requirements: max={}x{}, macroblocks/frame={}, macroblocks/s={}, async={}",
            max_width,
            max_height,
            max_macroblocks_per_frame,
            max_macroblocks_per_second,
            async_encode_supported
        );
    }

    Ok(NvencH264Capability {
        max_width,
        max_height,
        max_macroblocks_per_frame,
        max_macroblocks_per_second,
        async_encode_supported,
    })
}

struct NvencProbeSession {
    encoder: Option<NonNull<c_void>>,
    destroy_encoder: DestroyEncoder,
}

impl NvencProbeSession {
    fn handle(&self) -> *mut c_void {
        self.encoder.map_or(std::ptr::null_mut(), NonNull::as_ptr)
    }

    fn close(mut self) -> Result<()> {
        self.close_inner()
    }

    fn close_inner(&mut self) -> Result<()> {
        let Some(encoder) = self.encoder.take() else {
            return Ok(());
        };
        // SAFETY: `encoder` is the live handle returned by open-session and is
        // consumed exactly once by this method or Drop.
        let status = unsafe { (self.destroy_encoder)(encoder.as_ptr()) };
        nvenc_status(status, "nvEncDestroyEncoder")
    }
}

impl Drop for NvencProbeSession {
    fn drop(&mut self) {
        let _ = self.close_inner();
    }
}

fn supported_codec_guids(functions: &NvencFunctionList, encoder: *mut c_void) -> Result<Vec<GUID>> {
    let get_count = functions.get_encode_guid_count()?;
    let get_guids = functions.get_encode_guids()?;
    let mut count = 0_u32;
    // SAFETY: `encoder` is a live session handle and `count` is writable.
    let status = unsafe { get_count(encoder, &mut count) };
    nvenc_status(status, "nvEncGetEncodeGUIDCount")?;
    if count == 0 || count > NVENC_MAX_CODEC_GUIDS {
        bail!("NVENC returned an invalid codec GUID count {count}");
    }
    let capacity = usize::try_from(count).context("NVENC codec GUID count exceeds usize")?;
    let mut guids = vec![GUID::zeroed(); capacity];
    let mut written = 0_u32;
    // SAFETY: `guids` has `count` writable GUID elements and `written` is a
    // valid out-pointer; the encoder remains live.
    let status = unsafe { get_guids(encoder, guids.as_mut_ptr(), count, &mut written) };
    nvenc_status(status, "nvEncGetEncodeGUIDs")?;
    if written > count {
        bail!("NVENC wrote an invalid codec GUID count {written} > {count}");
    }
    guids.truncate(usize::try_from(written).context("NVENC GUID count exceeds usize")?);
    Ok(guids)
}

fn query_u32_cap(
    functions: &NvencFunctionList,
    encoder: *mut c_void,
    capability: i32,
    label: &str,
) -> Result<u32> {
    let get_caps = functions.get_encode_caps()?;
    let mut params = NvencCapsParam::new(capability);
    let mut value = 0_i32;
    // SAFETY: the session is live, H264_GUID is the pinned header constant,
    // and both output/input parameter pointers have the verified C layout.
    let status = unsafe { get_caps(encoder, H264_GUID, &mut params, &mut value) };
    nvenc_status(status, &format!("nvEncGetEncodeCaps ({label})"))?;
    u32::try_from(value).with_context(|| format!("NVENC returned negative {label}: {value}"))
}

pub(super) fn nvenc_status(status: i32, operation: &str) -> Result<()> {
    if status == NVENC_SUCCESS {
        return Ok(());
    }
    let name = match status {
        1 => "NO_ENCODE_DEVICE",
        2 => "UNSUPPORTED_DEVICE",
        3 => "INVALID_ENCODERDEVICE",
        4 => "INVALID_DEVICE",
        5 => "DEVICE_NOT_EXIST",
        6 => "INVALID_PTR",
        8 => "INVALID_PARAM",
        10 => "OUT_OF_MEMORY",
        12 => "UNSUPPORTED_PARAM",
        15 => "INVALID_VERSION",
        21 => "GENERIC",
        23 => "UNIMPLEMENTED",
        _ => "UNKNOWN",
    };
    bail!("{operation} failed with NVENCSTATUS {status} ({name})")
}

impl Drop for NvencDriverProbe {
    fn drop(&mut self) {
        // SAFETY: this value uniquely owns the successful LoadLibraryExW
        // reference and releases it exactly once here.
        unsafe {
            let _ = FreeLibrary(self.module);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_nvenc_api_version_layout() {
        assert_eq!(
            NvencApiVersion::decode((12 << 4) | 2),
            NvencApiVersion {
                major: 12,
                minor: 2
            }
        );
    }

    #[test]
    fn rust_ffi_layout_matches_pinned_12_2_header_probe() {
        assert_eq!(std::mem::size_of::<NvencFunctionList>(), 2552);
        assert_eq!(std::mem::offset_of!(NvencFunctionList, functions), 8);
        assert_eq!(8 + FUNCTION_GET_GUID_COUNT * 8, 16);
        assert_eq!(8 + FUNCTION_GET_GUIDS * 8, 40);
        assert_eq!(8 + FUNCTION_GET_CAPS * 8, 64);
        assert_eq!(8 + FUNCTION_DESTROY_ENCODER * 8, 224);
        assert_eq!(8 + FUNCTION_OPEN_SESSION_EX * 8, 240);
        assert_eq!(std::mem::size_of::<NvencOpenSessionParams>(), 1552);
        assert_eq!(std::mem::offset_of!(NvencOpenSessionParams, device), 8);
        assert_eq!(
            std::mem::offset_of!(NvencOpenSessionParams, api_version),
            24
        );
        assert_eq!(std::mem::size_of::<NvencCapsParam>(), 256);
        assert_eq!(NVENC_FUNCTION_LIST_VERSION, 0x7202_000c);
        assert_eq!(NVENC_OPEN_SESSION_VERSION, 0x7201_000c);
        assert_eq!(NVENC_CAPS_PARAM_VERSION, 0x7201_000c);
    }
}
