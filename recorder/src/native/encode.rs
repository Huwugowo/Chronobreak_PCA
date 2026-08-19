//! Direct NVENC H.264 submission for the fixed M3 NV12 texture ring.
//!
//! The D3D11 video context and all submission calls stay on the native GPU
//! worker. A bounded secondary thread waits on four registered completion
//! events, drains bitstreams in submission order, and never touches D3D11.

use std::ffi::c_void;
use std::io::Write;
use std::marker::PhantomData;
use std::os::windows::io::AsRawHandle;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail, ensure};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::System::IO::CancelSynchronousIo;
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForMultipleObjects};
use windows::core::{GUID, Interface, PCWSTR};

use super::NATIVE_ENCODER_SLOT_COUNT;
use super::convert::{ConvertedNv12Frame, NativeNv12Converter, NativeNv12SlotStates};
use super::nvenc::{
    H264_GUID, NvencDriverProbe, NvencFunctionList, NvencOpenSessionParams, nvenc_status,
    nvenc_struct_version, validate_h264_session,
};
use super::source::NativeWgcSource;

const ENCODE_WIDTH: u32 = 1920;
const ENCODE_HEIGHT: u32 = 1080;
const ENCODE_FPS: u32 = 60;
const H264_BITRATE: u32 = 12_000_000;
const H264_MAX_BITRATE: u32 = 18_000_000;
const H264_VBV_BUFFER: u32 = 24_000_000;
const H264_GOP_LENGTH: u32 = 120;

const COMPLETION_EVENT_TIMEOUT_MS: u32 = 5_000;
const COMPLETION_ACK_TIMEOUT: Duration = Duration::from_secs(6);
const COMPLETION_JOIN_TIMEOUT: Duration = Duration::from_secs(6);
const COMPLETION_ABORT_GRACE: Duration = Duration::from_secs(1);

const NVENC_SUCCESS: i32 = 0;
const NVENC_ERR_NEED_MORE_INPUT: i32 = 17;
const NV_ENC_TUNING_INFO_HIGH_QUALITY: i32 = 1;
const NV_ENC_PARAMS_RC_VBR: u32 = 1;
const NV_ENC_PARAMS_FRAME_FIELD_MODE_FRAME: u32 = 1;
const NV_ENC_MV_PRECISION_QUARTER_PEL: u32 = 3;
const NV_ENC_BUFFER_FORMAT_NV12: u32 = 1;
const NV_ENC_PIC_STRUCT_FRAME: u32 = 1;
const NV_ENC_PIC_FLAG_EOS: u32 = 8;
const NV_ENC_BIT_DEPTH_8: u32 = 8;

const FUNCTION_INITIALIZE_ENCODER: usize = 11;
const FUNCTION_CREATE_BITSTREAM_BUFFER: usize = 14;
const FUNCTION_DESTROY_BITSTREAM_BUFFER: usize = 15;
const FUNCTION_ENCODE_PICTURE: usize = 16;
const FUNCTION_LOCK_BITSTREAM: usize = 17;
const FUNCTION_UNLOCK_BITSTREAM: usize = 18;
const FUNCTION_REGISTER_ASYNC_EVENT: usize = 23;
const FUNCTION_UNREGISTER_ASYNC_EVENT: usize = 24;
const FUNCTION_MAP_INPUT_RESOURCE: usize = 25;
const FUNCTION_UNMAP_INPUT_RESOURCE: usize = 26;
const FUNCTION_DESTROY_ENCODER: usize = 27;
const FUNCTION_OPEN_SESSION_EX: usize = 29;
const FUNCTION_REGISTER_RESOURCE: usize = 30;
const FUNCTION_UNREGISTER_RESOURCE: usize = 31;
const FUNCTION_GET_PRESET_CONFIG_EX: usize = 39;

const CONFIG_SIZE: usize = 3_584;
const PRESET_CONFIG_SIZE: usize = 5_128;
const INITIALIZE_PARAMS_SIZE: usize = 1_800;
const CREATE_BITSTREAM_SIZE: usize = 776;
const REGISTER_RESOURCE_SIZE: usize = 1_536;
const MAP_INPUT_RESOURCE_SIZE: usize = 1_544;
const PIC_PARAMS_SIZE: usize = 3_360;
const LOCK_BITSTREAM_SIZE: usize = 1_544;
const EVENT_PARAMS_SIZE: usize = 1_544;

const CONFIG_VERSION: u32 = nvenc_struct_version(9) | (1 << 31);
const PRESET_CONFIG_VERSION: u32 = nvenc_struct_version(5) | (1 << 31);
const INITIALIZE_PARAMS_VERSION: u32 = nvenc_struct_version(7) | (1 << 31);
const CREATE_BITSTREAM_VERSION: u32 = nvenc_struct_version(1);
const REGISTER_RESOURCE_VERSION: u32 = nvenc_struct_version(5);
const MAP_INPUT_RESOURCE_VERSION: u32 = nvenc_struct_version(4);
const PIC_PARAMS_VERSION: u32 = nvenc_struct_version(7) | (1 << 31);
const LOCK_BITSTREAM_VERSION: u32 = nvenc_struct_version(2) | (1 << 31);
const EVENT_PARAMS_VERSION: u32 = nvenc_struct_version(2);
const RC_PARAMS_VERSION: u32 = nvenc_struct_version(1);

const H264_HIGH_PROFILE_GUID: GUID = GUID::from_values(
    0xe7cb_c309,
    0x4f7a,
    0x4b89,
    [0xaf, 0x2a, 0xd5, 0x37, 0xc9, 0x2b, 0xe3, 0x10],
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EncodePictureStatus {
    Accepted,
    AcceptedNeedsMoreInput,
}

fn classify_encode_picture_status(
    status: i32,
    operation: &'static str,
) -> Result<EncodePictureStatus> {
    match status {
        NVENC_SUCCESS => Ok(EncodePictureStatus::Accepted),
        NVENC_ERR_NEED_MORE_INPUT => Ok(EncodePictureStatus::AcceptedNeedsMoreInput),
        status => {
            nvenc_status(status, operation)?;
            unreachable!("non-success NVENC status unexpectedly passed validation")
        }
    }
}
const P4_PRESET_GUID: GUID = GUID::from_values(
    0x90a7_b826,
    0xdf06,
    0x4862,
    [0xb9, 0xd2, 0xcd, 0x6d, 0x73, 0xa0, 0x86, 0x81],
);

#[repr(C, align(8))]
struct NvencBlob<const N: usize> {
    bytes: [u8; N],
}

impl<const N: usize> NvencBlob<N> {
    fn zeroed() -> Self {
        Self { bytes: [0; N] }
    }

    fn write<T: Copy>(&mut self, offset: usize, value: T) {
        let end = offset
            .checked_add(std::mem::size_of::<T>())
            .expect("NVENC ABI offset arithmetic must not overflow");
        assert!(end <= N, "NVENC ABI write exceeds pinned structure");
        // SAFETY: the bounds check above proves the destination covers one T.
        // `write_unaligned` permits every offset used by the pinned C layout,
        // and `value` is copied into otherwise opaque initialized bytes.
        unsafe {
            self.bytes
                .as_mut_ptr()
                .add(offset)
                .cast::<T>()
                .write_unaligned(value);
        }
    }

    unsafe fn read_unchecked<T: Copy>(&self, offset: usize) -> T {
        let end = offset
            .checked_add(std::mem::size_of::<T>())
            .expect("NVENC ABI offset arithmetic must not overflow");
        assert!(end <= N, "NVENC ABI read exceeds pinned structure");
        // SAFETY: the caller proves every bit pattern is valid for T. The
        // bounds check proves the source covers one T and `read_unaligned`
        // permits each pinned C offset.
        unsafe { self.bytes.as_ptr().add(offset).cast::<T>().read_unaligned() }
    }

    #[cfg(test)]
    fn read_u16(&self, offset: usize) -> u16 {
        // SAFETY: every u16 bit pattern is valid.
        unsafe { self.read_unchecked(offset) }
    }

    fn read_u32(&self, offset: usize) -> u32 {
        // SAFETY: every u32 bit pattern is valid.
        unsafe { self.read_unchecked(offset) }
    }

    #[cfg(test)]
    fn read_i32(&self, offset: usize) -> i32 {
        // SAFETY: every i32 bit pattern is valid.
        unsafe { self.read_unchecked(offset) }
    }

    fn read_mut_ptr(&self, offset: usize) -> *mut c_void {
        // SAFETY: every bit pattern is valid for a raw pointer; it remains
        // opaque and is validated for null before wrapping/dereferencing.
        unsafe { self.read_unchecked(offset) }
    }

    fn read_const_u8_ptr(&self, offset: usize) -> *const u8 {
        // SAFETY: every bit pattern is valid for a raw pointer; NVENC's byte
        // count and null check gate creation of a borrowed slice.
        unsafe { self.read_unchecked(offset) }
    }

    #[cfg(test)]
    fn read_guid(&self, offset: usize) -> GUID {
        // SAFETY: GUID is composed exclusively of integer fields, so every
        // 128-bit representation is a valid GUID value.
        unsafe { self.read_unchecked(offset) }
    }

    fn as_mut_void(&mut self) -> *mut c_void {
        self.bytes.as_mut_ptr().cast()
    }
}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct EncoderHandle(NonNull<c_void>);

// SAFETY: the pinned NVENC 12.2 header explicitly prescribes submitting
// EncodePicture on the main thread and waiting/locking bitstreams on a
// secondary thread in asynchronous mode. This wrapper exposes only that split.
unsafe impl Send for EncoderHandle {}
// SAFETY: NVENC's documented asynchronous workflow concurrently references
// the same opaque session from submission and completion; no pointee memory is
// dereferenced by Rust.
unsafe impl Sync for EncoderHandle {}

impl EncoderHandle {
    fn as_ptr(self) -> *mut c_void {
        self.0.as_ptr()
    }
}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct NvencObjectHandle(NonNull<c_void>);

// SAFETY: these are opaque NVENC registered-resource, mapped-input, and
// bitstream tokens. The secondary thread uses only tokens passed for a
// successfully submitted frame, exactly as required by the asynchronous API.
unsafe impl Send for NvencObjectHandle {}
// SAFETY: Rust never dereferences these tokens; NVENC owns and synchronizes the
// referenced objects for the lifetime of their encoder session.
unsafe impl Sync for NvencObjectHandle {}

impl NvencObjectHandle {
    fn as_ptr(self) -> *mut c_void {
        self.0.as_ptr()
    }
}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct EventHandle(HANDLE);

// SAFETY: Windows kernel event handles are process-wide and WaitForSingleObject
// is designed for a different thread than the creator. Ownership remains in
// `OwnedEvent`; this copy is non-owning.
unsafe impl Send for EventHandle {}
// SAFETY: waiting and driver signaling are synchronized by the Windows kernel;
// Rust never accesses the underlying event object directly.
unsafe impl Sync for EventHandle {}

impl EventHandle {
    fn as_ptr(self) -> *mut c_void {
        self.0.0
    }
}

struct OwnedEvent {
    handle: EventHandle,
}

impl OwnedEvent {
    fn auto_reset() -> Result<Self> {
        // SAFETY: no security descriptor or name is supplied; the returned
        // auto-reset event is uniquely owned and initially nonsignaled.
        let handle = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
            .context("could not create NVENC completion event")?;
        Ok(Self {
            handle: EventHandle(handle),
        })
    }

    fn manual_reset() -> Result<Self> {
        // SAFETY: no security descriptor or name is supplied; the returned
        // manual-reset event is uniquely owned and initially nonsignaled.
        let handle = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }
            .context("could not create NVENC completion-cancellation event")?;
        Ok(Self {
            handle: EventHandle(handle),
        })
    }

    fn handle(&self) -> EventHandle {
        self.handle
    }
}

impl Drop for OwnedEvent {
    fn drop(&mut self) {
        // SAFETY: this value uniquely owns the valid CreateEventW handle and
        // encoder teardown unregisters or destroys all references first.
        let _ = unsafe { CloseHandle(self.handle.0) };
    }
}

type OpenSessionEx =
    unsafe extern "system" fn(*mut NvencOpenSessionParams, *mut *mut c_void) -> i32;
type GetPresetConfigEx =
    unsafe extern "system" fn(*mut c_void, GUID, GUID, i32, *mut c_void) -> i32;
type InitializeEncoder = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type CreateBitstreamBuffer = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type DestroyBitstreamBuffer = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type EncodePicture = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type LockBitstream = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type UnlockBitstream = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type RegisterAsyncEvent = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type UnregisterAsyncEvent = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type MapInputResource = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type UnmapInputResource = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type DestroyEncoder = unsafe extern "system" fn(*mut c_void) -> i32;
type RegisterResource = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type UnregisterResource = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;

macro_rules! load_nvenc_function {
    ($functions:expr, $index:expr, $name:literal, $ty:ty) => {{
        let pointer = $functions.required_function($index, $name)?;
        // SAFETY: the pinned NVENC 12.2 function-list layout fixes this index
        // and the named C signature; Win64 data and function pointers match in
        // width and the loaded module remains owned by the encoder.
        unsafe { std::mem::transmute::<*mut c_void, $ty>(pointer) }
    }};
}

#[derive(Clone, Copy)]
struct SubmissionApi {
    get_preset_config_ex: GetPresetConfigEx,
    initialize_encoder: InitializeEncoder,
    create_bitstream_buffer: CreateBitstreamBuffer,
    destroy_bitstream_buffer: DestroyBitstreamBuffer,
    encode_picture: EncodePicture,
    register_async_event: RegisterAsyncEvent,
    unregister_async_event: UnregisterAsyncEvent,
    map_input_resource: MapInputResource,
    unmap_input_resource: UnmapInputResource,
    destroy_encoder: DestroyEncoder,
    register_resource: RegisterResource,
    unregister_resource: UnregisterResource,
}

#[derive(Clone, Copy)]
struct CompletionApi {
    encoder: EncoderHandle,
    lock_bitstream: LockBitstream,
    unlock_bitstream: UnlockBitstream,
    unmap_input_resource: UnmapInputResource,
}

impl SubmissionApi {
    fn load(functions: &NvencFunctionList) -> Result<(OpenSessionEx, Self, CompletionApiSeed)> {
        Ok((
            load_nvenc_function!(
                functions,
                FUNCTION_OPEN_SESSION_EX,
                "nvEncOpenEncodeSessionEx",
                OpenSessionEx
            ),
            Self {
                get_preset_config_ex: load_nvenc_function!(
                    functions,
                    FUNCTION_GET_PRESET_CONFIG_EX,
                    "nvEncGetEncodePresetConfigEx",
                    GetPresetConfigEx
                ),
                initialize_encoder: load_nvenc_function!(
                    functions,
                    FUNCTION_INITIALIZE_ENCODER,
                    "nvEncInitializeEncoder",
                    InitializeEncoder
                ),
                create_bitstream_buffer: load_nvenc_function!(
                    functions,
                    FUNCTION_CREATE_BITSTREAM_BUFFER,
                    "nvEncCreateBitstreamBuffer",
                    CreateBitstreamBuffer
                ),
                destroy_bitstream_buffer: load_nvenc_function!(
                    functions,
                    FUNCTION_DESTROY_BITSTREAM_BUFFER,
                    "nvEncDestroyBitstreamBuffer",
                    DestroyBitstreamBuffer
                ),
                encode_picture: load_nvenc_function!(
                    functions,
                    FUNCTION_ENCODE_PICTURE,
                    "nvEncEncodePicture",
                    EncodePicture
                ),
                register_async_event: load_nvenc_function!(
                    functions,
                    FUNCTION_REGISTER_ASYNC_EVENT,
                    "nvEncRegisterAsyncEvent",
                    RegisterAsyncEvent
                ),
                unregister_async_event: load_nvenc_function!(
                    functions,
                    FUNCTION_UNREGISTER_ASYNC_EVENT,
                    "nvEncUnregisterAsyncEvent",
                    UnregisterAsyncEvent
                ),
                map_input_resource: load_nvenc_function!(
                    functions,
                    FUNCTION_MAP_INPUT_RESOURCE,
                    "nvEncMapInputResource",
                    MapInputResource
                ),
                unmap_input_resource: load_nvenc_function!(
                    functions,
                    FUNCTION_UNMAP_INPUT_RESOURCE,
                    "nvEncUnmapInputResource",
                    UnmapInputResource
                ),
                destroy_encoder: load_nvenc_function!(
                    functions,
                    FUNCTION_DESTROY_ENCODER,
                    "nvEncDestroyEncoder",
                    DestroyEncoder
                ),
                register_resource: load_nvenc_function!(
                    functions,
                    FUNCTION_REGISTER_RESOURCE,
                    "nvEncRegisterResource",
                    RegisterResource
                ),
                unregister_resource: load_nvenc_function!(
                    functions,
                    FUNCTION_UNREGISTER_RESOURCE,
                    "nvEncUnregisterResource",
                    UnregisterResource
                ),
            },
            CompletionApiSeed {
                lock_bitstream: load_nvenc_function!(
                    functions,
                    FUNCTION_LOCK_BITSTREAM,
                    "nvEncLockBitstream",
                    LockBitstream
                ),
                unlock_bitstream: load_nvenc_function!(
                    functions,
                    FUNCTION_UNLOCK_BITSTREAM,
                    "nvEncUnlockBitstream",
                    UnlockBitstream
                ),
                unmap_input_resource: load_nvenc_function!(
                    functions,
                    FUNCTION_UNMAP_INPUT_RESOURCE,
                    "nvEncUnmapInputResource",
                    UnmapInputResource
                ),
            },
        ))
    }
}

#[derive(Clone, Copy)]
struct CompletionApiSeed {
    lock_bitstream: LockBitstream,
    unlock_bitstream: UnlockBitstream,
    unmap_input_resource: UnmapInputResource,
}

impl CompletionApiSeed {
    fn bind(self, encoder: EncoderHandle) -> CompletionApi {
        CompletionApi {
            encoder,
            lock_bitstream: self.lock_bitstream,
            unlock_bitstream: self.unlock_bitstream,
            unmap_input_resource: self.unmap_input_resource,
        }
    }
}

fn open_session(
    open: OpenSessionEx,
    destroy: DestroyEncoder,
    source: &NativeWgcSource,
) -> Result<EncoderSession> {
    let mut params = NvencOpenSessionParams::directx(source.device().device().as_raw());
    let mut encoder = std::ptr::null_mut();
    // SAFETY: the source retains the live D3D11 device, `params` has the
    // pinned 12.2 layout/API version, and `encoder` is writable storage.
    let status = unsafe { open(&mut params, &mut encoder) };
    nvenc_status(status, "nvEncOpenEncodeSessionEx for direct recording")?;
    let handle = NonNull::new(encoder)
        .map(EncoderHandle)
        .context("NVENC opened a recording session without a handle")?;
    Ok(EncoderSession {
        handle: Some(handle),
        destroy,
    })
}

struct EncoderSession {
    handle: Option<EncoderHandle>,
    destroy: DestroyEncoder,
}

impl EncoderSession {
    fn handle(&self) -> EncoderHandle {
        self.handle
            .expect("live NativeNvencEncoder must retain its NVENC session")
    }

    fn close(&mut self) -> Result<()> {
        let Some(handle) = self.handle.take() else {
            return Ok(());
        };
        // SAFETY: `handle` is the live session returned by open-session and is
        // consumed exactly once after completion/resource teardown.
        let status = unsafe { (self.destroy)(handle.as_ptr()) };
        nvenc_status(status, "nvEncDestroyEncoder")
    }
}

impl Drop for EncoderSession {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn preset_h264_config(
    api: SubmissionApi,
    encoder: EncoderHandle,
) -> Result<NvencBlob<CONFIG_SIZE>> {
    let mut preset = NvencBlob::<PRESET_CONFIG_SIZE>::zeroed();
    preset.write(0, PRESET_CONFIG_VERSION);
    preset.write(8, CONFIG_VERSION);
    // SAFETY: `preset` is aligned/writable storage with the exact pinned
    // NV_ENC_PRESET_CONFIG layout and both nested version fields initialized.
    let status = unsafe {
        (api.get_preset_config_ex)(
            encoder.as_ptr(),
            H264_GUID,
            P4_PRESET_GUID,
            NV_ENC_TUNING_INFO_HIGH_QUALITY,
            preset.as_mut_void(),
        )
    };
    nvenc_status(status, "nvEncGetEncodePresetConfigEx (H.264 P4 HQ)")?;

    let mut config = NvencBlob::<CONFIG_SIZE>::zeroed();
    config
        .bytes
        .copy_from_slice(&preset.bytes[8..8 + CONFIG_SIZE]);
    configure_h264(&mut config);
    Ok(config)
}

fn configure_h264(config: &mut NvencBlob<CONFIG_SIZE>) {
    // NV_ENC_CONFIG and nested NV_ENC_RC_PARAMS offsets are generated and
    // verified against nvEncodeAPI.h 12.2.72.0 by the companion C layout probe.
    config.write(0, CONFIG_VERSION);
    config.write(4, H264_HIGH_PROFILE_GUID);
    config.write(20, H264_GOP_LENGTH);
    config.write(24, 1_i32); // IPP: no B frames.
    config.write(32, NV_ENC_PARAMS_FRAME_FIELD_MODE_FRAME);
    config.write(36, NV_ENC_MV_PRECISION_QUARTER_PEL);
    config.write(40, RC_PARAMS_VERSION);
    config.write(44, NV_ENC_PARAMS_RC_VBR);
    config.write(60, H264_BITRATE);
    config.write(64, H264_MAX_BITRATE);
    config.write(68, H264_VBV_BUFFER);
    config.write(72, 0_u32); // Let the driver choose the initial VBV delay.
    config.write(76, 0_u32); // Disable AQ/lookahead/non-default RC bitfields.
    config.write(130, 0_u16); // lookaheadDepth.
    config.write(140, 0_u32); // NV_ENC_MULTI_PASS_DISABLED.

    // NV_ENC_CONFIG_H264 begins at offset 168.
    config.write(176, H264_GOP_LENGTH); // idrPeriod.
    config.write(248, 1_u32); // videoSignalTypePresentFlag.
    config.write(252, 5_u32); // videoFormat: unspecified.
    config.write(256, 0_u32); // videoFullRangeFlag: studio/limited range.
    config.write(260, 1_u32); // colourDescriptionPresentFlag.
    config.write(264, 1_u32); // BT.709 colour primaries.
    config.write(268, 1_u32); // BT.709 transfer characteristics.
    config.write(272, 1_u32); // BT.709 matrix coefficients.
    config.write(360, 1_u32); // 4:2:0 chromaFormatIDC.
    config.write(380, NV_ENC_BIT_DEPTH_8);
    config.write(384, NV_ENC_BIT_DEPTH_8);
}

fn initialize_encoder(
    api: SubmissionApi,
    encoder: EncoderHandle,
    config: &mut NvencBlob<CONFIG_SIZE>,
) -> Result<()> {
    let mut params = NvencBlob::<INITIALIZE_PARAMS_SIZE>::zeroed();
    params.write(0, INITIALIZE_PARAMS_VERSION);
    params.write(4, H264_GUID);
    params.write(20, P4_PRESET_GUID);
    params.write(36, ENCODE_WIDTH);
    params.write(40, ENCODE_HEIGHT);
    params.write(44, ENCODE_WIDTH);
    params.write(48, ENCODE_HEIGHT);
    params.write(52, ENCODE_FPS);
    params.write(56, 1_u32);
    params.write(60, 1_u32); // enableEncodeAsync.
    params.write(64, 1_u32); // enablePTD.
    params.write(88, config.as_mut_void());
    params.write(96, ENCODE_WIDTH);
    params.write(100, ENCODE_HEIGHT);
    params.write(136, NV_ENC_TUNING_INFO_HIGH_QUALITY);
    // SAFETY: params/config are aligned exact-layout blobs retained for the
    // call, the config pointer targets the mutable blob above, and the session
    // was opened on the converter's D3D11 device.
    let status = unsafe { (api.initialize_encoder)(encoder.as_ptr(), params.as_mut_void()) };
    nvenc_status(status, "nvEncInitializeEncoder (H.264 1080p60 P4 VBR)")
}

struct NvencSlot {
    registered_resource: Option<NvencObjectHandle>,
    bitstream: Option<NvencObjectHandle>,
    event_registered: bool,
    event: OwnedEvent,
    _texture: ID3D11Texture2D,
}

impl NvencSlot {
    fn event_handle(&self) -> EventHandle {
        self.event.handle()
    }

    fn registered_resource(&self) -> Result<NvencObjectHandle> {
        self.registered_resource
            .context("NVENC slot has no registered input resource")
    }

    fn bitstream(&self) -> Result<NvencObjectHandle> {
        self.bitstream
            .context("NVENC slot has no output bitstream buffer")
    }
}

fn create_slot_shells(
    textures: [ID3D11Texture2D; NATIVE_ENCODER_SLOT_COUNT],
) -> Result<Vec<NvencSlot>> {
    let mut slots = Vec::with_capacity(NATIVE_ENCODER_SLOT_COUNT);
    for texture in textures {
        slots.push(NvencSlot {
            registered_resource: None,
            bitstream: None,
            event_registered: false,
            event: OwnedEvent::auto_reset()?,
            _texture: texture,
        });
    }
    Ok(slots)
}

fn initialize_slots(
    api: SubmissionApi,
    encoder: EncoderHandle,
    slots: &mut [NvencSlot],
) -> Result<()> {
    ensure!(
        slots.len() == NATIVE_ENCODER_SLOT_COUNT,
        "native NVENC requires exactly four slot shells"
    );

    for slot in slots.iter_mut() {
        let mut params = event_params(slot.event_handle());
        // SAFETY: the event is live, params has the exact pinned layout, and
        // the encoder session remains initialized through slot teardown.
        let status = unsafe { (api.register_async_event)(encoder.as_ptr(), params.as_mut_void()) };
        nvenc_status(status, "nvEncRegisterAsyncEvent")?;
        slot.event_registered = true;
    }

    for slot in slots.iter_mut() {
        let mut params = NvencBlob::<REGISTER_RESOURCE_SIZE>::zeroed();
        params.write(0, REGISTER_RESOURCE_VERSION);
        params.write(4, 0_u32); // NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX.
        params.write(8, ENCODE_WIDTH);
        params.write(12, ENCODE_HEIGHT);
        params.write(16, 0_u32); // DirectX pitch is zero.
        params.write(20, 0_u32); // subResourceIndex.
        params.write(24, slot._texture.as_raw());
        params.write(40, NV_ENC_BUFFER_FORMAT_NV12);
        params.write(44, 0_u32); // NV_ENC_INPUT_IMAGE.
        // SAFETY: the texture is a live same-device GPU-only NV12 surface and
        // remains retained in this slot until after encoder destruction.
        let status = unsafe { (api.register_resource)(encoder.as_ptr(), params.as_mut_void()) };
        nvenc_status(status, "nvEncRegisterResource (D3D11 NV12)")?;
        let registered = params.read_mut_ptr(32);
        slot.registered_resource = NonNull::new(registered).map(NvencObjectHandle);
        ensure!(
            slot.registered_resource.is_some(),
            "NVENC registered a D3D11 texture without returning a handle"
        );
    }

    for slot in slots.iter_mut() {
        let mut params = NvencBlob::<CREATE_BITSTREAM_SIZE>::zeroed();
        params.write(0, CREATE_BITSTREAM_VERSION);
        // SAFETY: the blob has the exact pinned layout and the initialized
        // encoder owns all output allocation performed by this call.
        let status =
            unsafe { (api.create_bitstream_buffer)(encoder.as_ptr(), params.as_mut_void()) };
        nvenc_status(status, "nvEncCreateBitstreamBuffer")?;
        let bitstream = params.read_mut_ptr(16);
        slot.bitstream = NonNull::new(bitstream).map(NvencObjectHandle);
        ensure!(
            slot.bitstream.is_some(),
            "NVENC created no bitstream-buffer handle"
        );
    }
    Ok(())
}

fn event_params(event: EventHandle) -> NvencBlob<EVENT_PARAMS_SIZE> {
    let mut params = NvencBlob::<EVENT_PARAMS_SIZE>::zeroed();
    params.write(0, EVENT_PARAMS_VERSION);
    params.write(8, event.as_ptr());
    params
}

fn cleanup_slot_resources(
    api: SubmissionApi,
    encoder: EncoderHandle,
    slots: &mut [NvencSlot],
) -> Option<anyhow::Error> {
    let mut first_error = None;
    for slot in slots.iter_mut() {
        if let Some(resource) = slot.registered_resource.take() {
            // SAFETY: this is the live handle returned by registration and all
            // submitted work has been drained before normal cleanup.
            let status = unsafe { (api.unregister_resource)(encoder.as_ptr(), resource.as_ptr()) };
            remember_error(
                &mut first_error,
                nvenc_status(status, "nvEncUnregisterResource").err(),
            );
        }
    }
    for slot in slots.iter_mut() {
        if let Some(bitstream) = slot.bitstream.take() {
            // SAFETY: this live output buffer is no longer queued or locked.
            let status =
                unsafe { (api.destroy_bitstream_buffer)(encoder.as_ptr(), bitstream.as_ptr()) };
            remember_error(
                &mut first_error,
                nvenc_status(status, "nvEncDestroyBitstreamBuffer").err(),
            );
        }
    }
    for slot in slots.iter_mut() {
        if slot.event_registered {
            let mut params = event_params(slot.event_handle());
            // SAFETY: this is the same live event registered for the slot and
            // no encode submission can signal it after the drain barrier.
            let status =
                unsafe { (api.unregister_async_event)(encoder.as_ptr(), params.as_mut_void()) };
            remember_error(
                &mut first_error,
                nvenc_status(status, "nvEncUnregisterAsyncEvent").err(),
            );
            slot.event_registered = false;
        }
    }
    first_error
}

fn remember_error(target: &mut Option<anyhow::Error>, error: Option<anyhow::Error>) {
    if target.is_none() {
        *target = error;
    }
}

#[derive(Default)]
struct CompletionTelemetry {
    completed_frames: AtomicU64,
    output_bytes: AtomicU64,
    completion_errors: AtomicU64,
    abort_cleanup: AtomicBool,
    first_error: Mutex<Option<String>>,
}

impl CompletionTelemetry {
    fn record_error(&self, error: &anyhow::Error) {
        if let Ok(mut first_error) = self.first_error.lock()
            && first_error.is_none()
        {
            *first_error = Some(format!("{error:#}"));
        }
        self.completion_errors.fetch_add(1, Ordering::Release);
        self.abort_cleanup.store(true, Ordering::Release);
    }

    fn error_detail(&self) -> Option<String> {
        self.first_error
            .lock()
            .ok()
            .and_then(|first_error| first_error.clone())
    }
}

/// Lock-free snapshot of direct-NVENC queue and output accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeNvencTelemetrySnapshot {
    pub submitted_frames: u64,
    pub completed_frames: u64,
    pub output_bytes: u64,
    pub max_in_flight: u64,
    pub submission_queue_failures: u64,
    pub completion_errors: u64,
}

struct CompletionTask {
    slot_index: usize,
    mapped_input: NvencObjectHandle,
    output_bitstream: NvencObjectHandle,
    event: EventHandle,
    states: Arc<NativeNv12SlotStates>,
}

enum CompletionCommand {
    Frame(CompletionTask),
    Barrier(SyncSender<()>),
    Eos {
        event: EventHandle,
        acknowledged: SyncSender<()>,
    },
}

fn run_completion_thread(
    receiver: Receiver<CompletionCommand>,
    api: CompletionApi,
    cancellation: EventHandle,
    mut writer: Box<dyn Write + Send>,
    telemetry: Arc<CompletionTelemetry>,
) -> Result<()> {
    while let Ok(command) = receiver.recv() {
        let result = match command {
            CompletionCommand::Frame(task) => {
                process_completion(task, api, cancellation, &mut *writer, &telemetry)
            }
            CompletionCommand::Barrier(acknowledged) => {
                let _ = acknowledged.send(());
                Ok(())
            }
            CompletionCommand::Eos {
                event,
                acknowledged,
            } => {
                let result = wait_for_event(event, cancellation, "NVENC EOS completion");
                let _ = acknowledged.send(());
                result
            }
        };
        if let Err(error) = result {
            telemetry.record_error(&error);
            return Err(error);
        }
    }

    if let Err(error) = writer
        .flush()
        .context("could not flush native H.264 output")
    {
        telemetry.record_error(&error);
        return Err(error);
    }
    Ok(())
}

fn process_completion(
    task: CompletionTask,
    api: CompletionApi,
    cancellation: EventHandle,
    writer: &mut dyn Write,
    telemetry: &CompletionTelemetry,
) -> Result<()> {
    wait_for_event(task.event, cancellation, "NVENC frame completion")?;

    let mut lock = NvencBlob::<LOCK_BITSTREAM_SIZE>::zeroed();
    lock.write(0, LOCK_BITSTREAM_VERSION);
    lock.write(8, task.output_bitstream.as_ptr());
    // SAFETY: the completion event for this output buffer was signaled, the
    // session/output handle remain live, and `lock` has the exact pinned ABI.
    let status = unsafe { (api.lock_bitstream)(api.encoder.as_ptr(), lock.as_mut_void()) };
    nvenc_status(status, "nvEncLockBitstream after completion event")?;

    let byte_count = lock.read_u32(36);
    let bitstream_pointer = lock.read_const_u8_ptr(56);
    let mut output_error = None;
    if byte_count > 0 {
        if bitstream_pointer.is_null() {
            output_error = Some(anyhow!(
                "NVENC locked {byte_count} bytes with a null bitstream pointer"
            ));
        } else {
            let length =
                usize::try_from(byte_count).expect("u32 bitstream length must fit usize on Win64");
            // SAFETY: NVENC returned this pointer and byte count from a
            // successful lock; the slice is read-only and used before unlock.
            let bytes = unsafe { std::slice::from_raw_parts(bitstream_pointer, length) };
            output_error = writer
                .write_all(bytes)
                .context("could not write native H.264 bitstream")
                .err();
        }
    }

    // SAFETY: this output handle is locked exactly once above and remains live
    // until the call returns; no other thread accesses the bitstream buffer.
    let unlock_status =
        unsafe { (api.unlock_bitstream)(api.encoder.as_ptr(), task.output_bitstream.as_ptr()) };
    if let Err(error) = nvenc_status(unlock_status, "nvEncUnlockBitstream") {
        return Err(combine_completion_errors(error, output_error));
    }

    // SAFETY: the successful lock proves this submission completed; the
    // mapped token is invalidated once and is not used after this call.
    let unmap_status =
        unsafe { (api.unmap_input_resource)(api.encoder.as_ptr(), task.mapped_input.as_ptr()) };
    if let Err(error) = nvenc_status(unmap_status, "nvEncUnmapInputResource") {
        return Err(combine_completion_errors(error, output_error));
    }

    task.states
        .complete_submitted(task.slot_index)
        .map_err(|error| combine_completion_errors(error, output_error.take()))?;
    telemetry.completed_frames.fetch_add(1, Ordering::Relaxed);
    telemetry
        .output_bytes
        .fetch_add(u64::from(byte_count), Ordering::Relaxed);
    match output_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn wait_for_event(event: EventHandle, cancellation: EventHandle, operation: &str) -> Result<()> {
    wait_for_event_with_timeout(event, cancellation, operation, COMPLETION_EVENT_TIMEOUT_MS)
}

fn wait_for_event_with_timeout(
    event: EventHandle,
    cancellation: EventHandle,
    operation: &str,
    timeout_ms: u32,
) -> Result<()> {
    // SAFETY: both handles remain owned by the encoder until after the
    // completion thread joins (or are deliberately leaked with a detached
    // stuck thread). The finite wait covers one frame or cancellation.
    let result = unsafe { WaitForMultipleObjects(&[event.0, cancellation.0], false, timeout_ms) };
    if result == WAIT_OBJECT_0 {
        return Ok(());
    }
    if result.0 == WAIT_OBJECT_0.0 + 1 {
        bail!("{operation} was cancelled during bounded shutdown");
    }
    if result == WAIT_TIMEOUT {
        bail!(
            "{operation} exceeded the {} ms completion deadline",
            timeout_ms
        );
    }
    if result == WAIT_FAILED {
        return Err(windows::core::Error::from_thread()).with_context(|| operation.to_owned());
    }
    bail!(
        "{operation} returned unexpected wait status 0x{:08x}",
        result.0
    )
}

fn combine_completion_errors(
    primary: anyhow::Error,
    output: Option<anyhow::Error>,
) -> anyhow::Error {
    match output {
        Some(output) => anyhow!("{primary:#}; output error: {output:#}"),
        None => primary,
    }
}

/// Direct H.264 encoder for M3's four fixed NV12 textures. This type is
/// deliberately !Send/!Sync so D3D/NVENC submission remains on its creator
/// worker; only ordered output completion crosses to the secondary thread.
pub struct NativeNvencEncoder {
    api: SubmissionApi,
    completion_sender: Option<SyncSender<CompletionCommand>>,
    completion_thread: Option<JoinHandle<Result<()>>>,
    completion_done: Receiver<()>,
    completion_cancellation: Option<OwnedEvent>,
    eos_event: Option<OwnedEvent>,
    orphaned_completions: Vec<CompletionTask>,
    abandoned_mappings: Vec<NvencObjectHandle>,
    slots: Option<[NvencSlot; NATIVE_ENCODER_SLOT_COUNT]>,
    states: Arc<NativeNv12SlotStates>,
    session: Option<EncoderSession>,
    driver: Option<NvencDriverProbe>,
    completion_telemetry: Arc<CompletionTelemetry>,
    submitted_frames: u64,
    max_in_flight: u64,
    submission_queue_failures: u64,
    frame_index: u32,
    abort_cleanup: bool,
    _thread_affinity: PhantomData<Rc<()>>,
}

impl NativeNvencEncoder {
    pub fn new<W>(
        source: &NativeWgcSource,
        converter: &NativeNv12Converter,
        writer: W,
    ) -> Result<Self>
    where
        W: Write + Send + 'static,
    {
        ensure!(
            converter.dimensions() == (ENCODE_WIDTH, ENCODE_HEIGHT),
            "native NVENC requires a 1920x1080 M3 converter"
        );
        ensure!(
            converter.free_slot_count() == NATIVE_ENCODER_SLOT_COUNT,
            "native NVENC must register the M3 ring before any slot is leased"
        );

        let driver = NvencDriverProbe::load()?;
        driver.ensure_required_api()?;
        let functions = driver.create_function_list()?;
        let (open, api, completion_seed) = SubmissionApi::load(&functions)?;
        let mut session = open_session(open, api.destroy_encoder, source)?;
        let encoder = session.handle();
        if let Err(error) = validate_h264_session(&functions, encoder.as_ptr()) {
            let session_error = session.close().err();
            return Err(combine_initialization_errors(error, None, session_error));
        }
        let mut config = match preset_h264_config(api, encoder) {
            Ok(config) => config,
            Err(error) => {
                let session_error = session.close().err();
                return Err(combine_initialization_errors(error, None, session_error));
            }
        };
        if let Err(error) = initialize_encoder(api, encoder, &mut config) {
            let session_error = session.close().err();
            return Err(combine_initialization_errors(error, None, session_error));
        }

        let (textures, states) = converter.encoder_resources();
        let mut slot_vec = match create_slot_shells(textures) {
            Ok(slots) => slots,
            Err(error) => {
                let session_error = session.close().err();
                return Err(combine_initialization_errors(error, None, session_error));
            }
        };
        if let Err(error) = initialize_slots(api, encoder, &mut slot_vec) {
            let cleanup_error = cleanup_slot_resources(api, encoder, &mut slot_vec);
            let session_error = session.close().err();
            drop(slot_vec);
            return Err(combine_initialization_errors(
                error,
                cleanup_error,
                session_error,
            ));
        }
        let slots: [NvencSlot; NATIVE_ENCODER_SLOT_COUNT] = match slot_vec.try_into() {
            Ok(slots) => slots,
            Err(mut slots) => {
                let slot_count = slots.len();
                let cleanup_error = cleanup_slot_resources(api, encoder, &mut slots);
                let session_error = session.close().err();
                drop(slots);
                return Err(combine_initialization_errors(
                    anyhow!(
                        "NVENC initialized {slot_count} slots instead of {}",
                        NATIVE_ENCODER_SLOT_COUNT
                    ),
                    cleanup_error,
                    session_error,
                ));
            }
        };

        let completion_telemetry = Arc::new(CompletionTelemetry::default());
        let (completion_sender, completion_receiver) =
            sync_channel::<CompletionCommand>(NATIVE_ENCODER_SLOT_COUNT + 2);
        let completion_cancellation = match OwnedEvent::manual_reset() {
            Ok(event) => event,
            Err(error) => {
                let mut slots = slots;
                let cleanup_error = cleanup_slot_resources(api, encoder, &mut slots);
                let session_error = session.close().err();
                drop(slots);
                return Err(combine_initialization_errors(
                    error,
                    cleanup_error,
                    session_error,
                ));
            }
        };
        let cancellation_handle = completion_cancellation.handle();
        let (completion_done_sender, completion_done) = sync_channel(1);
        let thread_telemetry = Arc::clone(&completion_telemetry);
        let completion_api = completion_seed.bind(encoder);
        let completion_thread = match std::thread::Builder::new()
            .name("chronobreak-nvenc-output".to_owned())
            .spawn(move || {
                let result = run_completion_thread(
                    completion_receiver,
                    completion_api,
                    cancellation_handle,
                    Box::new(writer),
                    thread_telemetry,
                );
                let _ = completion_done_sender.send(());
                result
            }) {
            Ok(thread) => thread,
            Err(error) => {
                let mut slots = slots;
                let cleanup_error = cleanup_slot_resources(api, encoder, &mut slots);
                let session_error = session.close().err();
                drop(slots);
                return Err(combine_initialization_errors(
                    anyhow!(error).context("could not start NVENC completion thread"),
                    cleanup_error,
                    session_error,
                ));
            }
        };

        Ok(Self {
            api,
            completion_sender: Some(completion_sender),
            completion_thread: Some(completion_thread),
            completion_done,
            completion_cancellation: Some(completion_cancellation),
            eos_event: None,
            orphaned_completions: Vec::new(),
            abandoned_mappings: Vec::new(),
            slots: Some(slots),
            states,
            session: Some(session),
            driver: Some(driver),
            completion_telemetry,
            submitted_frames: 0,
            max_in_flight: 0,
            submission_queue_failures: 0,
            frame_index: 0,
            abort_cleanup: false,
            _thread_affinity: PhantomData,
        })
    }

    /// Submit without waiting for hardware or output I/O. A full fixed ring is
    /// handled by M3 before this call; this method performs no blocking send.
    pub fn submit(&mut self, frame: ConvertedNv12Frame<'_>) -> Result<()> {
        if self
            .completion_telemetry
            .completion_errors
            .load(Ordering::Acquire)
            != 0
        {
            return Err(self.completion_failure(
                "native NVENC completion/output thread failed before frame submission",
            ));
        }
        ensure!(!self.abort_cleanup, "native NVENC encoder is aborting");
        ensure!(
            frame.belongs_to(&self.states),
            "converted NV12 frame belongs to a different four-texture ring than this NVENC encoder"
        );
        let slot_index = frame.slot_index();
        let qpc_100ns = frame.qpc_100ns();
        let (registered_resource, output_bitstream, event) = {
            let slot = self
                .slots
                .as_ref()
                .and_then(|slots| slots.get(slot_index))
                .context("converted NV12 frame referenced an invalid NVENC slot")?;
            (
                slot.registered_resource()?,
                slot.bitstream()?,
                slot.event_handle(),
            )
        };
        let timestamp = u64::try_from(qpc_100ns)
            .context("native WGC timestamp is negative and cannot be submitted to NVENC")?;
        let duration = 10_000_000_u64 / u64::from(ENCODE_FPS);
        let encoder = self.encoder_handle()?;
        let mapped_input = self.map_input(registered_resource)?;

        let (submitted_slot, _) = match frame.mark_submitted() {
            Ok(submitted) => submitted,
            Err(error) => {
                if let Err(unmap_error) = self.unmap_rejected(mapped_input) {
                    self.abort_cleanup = true;
                    self.completion_telemetry
                        .abort_cleanup
                        .store(true, Ordering::Release);
                    return Err(combine_initialization_errors(
                        error,
                        Some(unmap_error),
                        None,
                    ));
                }
                return Err(error);
            }
        };
        debug_assert_eq!(submitted_slot, slot_index);

        let mut params = NvencBlob::<PIC_PARAMS_SIZE>::zeroed();
        params.write(0, PIC_PARAMS_VERSION);
        params.write(4, ENCODE_WIDTH);
        params.write(8, ENCODE_HEIGHT);
        params.write(12, ENCODE_WIDTH); // Pitch is ignored for mapped DirectX input.
        params.write(20, self.frame_index);
        params.write(24, timestamp);
        params.write(32, duration);
        params.write(40, mapped_input.as_ptr());
        params.write(48, output_bitstream.as_ptr());
        params.write(56, event.as_ptr());
        params.write(64, NV_ENC_BUFFER_FORMAT_NV12);
        params.write(68, NV_ENC_PIC_STRUCT_FRAME);

        // SAFETY: the mapped same-device NV12 input, distinct output/event and
        // exact-layout params all remain live until the queued completion runs.
        let status = unsafe { (self.api.encode_picture)(encoder.as_ptr(), params.as_mut_void()) };
        if let Err(submit_error) = classify_encode_picture_status(status, "nvEncEncodePicture") {
            let unmap_error = self.unmap_rejected(mapped_input).err();
            let state_error = if unmap_error.is_none() {
                self.states.complete_submitted(slot_index).err()
            } else {
                None
            };
            if unmap_error.is_some() || state_error.is_some() {
                self.abort_cleanup = true;
                self.completion_telemetry
                    .abort_cleanup
                    .store(true, Ordering::Release);
            }
            return Err(combine_initialization_errors(
                submit_error,
                unmap_error,
                state_error,
            ));
        }

        // Both SUCCESS and NEED_MORE_INPUT accept ownership of the input and
        // output sample. NVIDIA requires asynchronous clients to wait on every
        // completion event in submission order; EOS releases any final sample
        // still deferred for reordering or look-ahead.

        self.submitted_frames = self.submitted_frames.saturating_add(1);
        self.frame_index = self.frame_index.wrapping_add(1);
        let completed = self
            .completion_telemetry
            .completed_frames
            .load(Ordering::Relaxed);
        self.max_in_flight = self
            .max_in_flight
            .max(self.submitted_frames.saturating_sub(completed));

        let task = CompletionTask {
            slot_index,
            mapped_input,
            output_bitstream,
            event,
            states: Arc::clone(&self.states),
        };
        let sender = self
            .completion_sender
            .as_ref()
            .context("native NVENC completion channel is closed")?;
        match sender.try_send(CompletionCommand::Frame(task)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(CompletionCommand::Frame(task))) => {
                self.submission_queue_failures = self.submission_queue_failures.saturating_add(1);
                self.orphaned_completions.push(task);
                bail!("four-entry NVENC completion queue was unexpectedly full")
            }
            Err(TrySendError::Disconnected(CompletionCommand::Frame(task))) => {
                self.submission_queue_failures = self.submission_queue_failures.saturating_add(1);
                self.abort_cleanup = true;
                self.completion_telemetry
                    .abort_cleanup
                    .store(true, Ordering::Release);
                self.abandoned_mappings.push(task.mapped_input);
                bail!("NVENC completion thread disconnected after frame submission")
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                unreachable!("submit sends only frame completion commands")
            }
        }
    }

    pub fn telemetry(&self) -> NativeNvencTelemetrySnapshot {
        NativeNvencTelemetrySnapshot {
            submitted_frames: self.submitted_frames,
            completed_frames: self
                .completion_telemetry
                .completed_frames
                .load(Ordering::Relaxed),
            output_bytes: self
                .completion_telemetry
                .output_bytes
                .load(Ordering::Relaxed),
            max_in_flight: self.max_in_flight,
            submission_queue_failures: self.submission_queue_failures,
            completion_errors: self
                .completion_telemetry
                .completion_errors
                .load(Ordering::Relaxed),
        }
    }

    /// Test-only terminal failure seam for M6 lifecycle validation. It marks
    /// the session unsafe for normal resource cleanup without calling an
    /// invalid or undocumented driver entry point.
    #[cfg(feature = "native-failure-injection")]
    pub fn inject_terminal_failure_for_fixture(&mut self) -> Result<()> {
        self.abort_cleanup = true;
        self.completion_telemetry
            .abort_cleanup
            .store(true, Ordering::Release);
        bail!("injected terminal native NVENC failure")
    }

    /// Drain every previously submitted frame. This is a cold-path barrier for
    /// resize/reconfiguration and shutdown, never part of steady-state submit.
    pub fn drain(&mut self) -> Result<()> {
        let sender = self
            .completion_sender
            .as_ref()
            .context("native NVENC completion channel is closed")?;
        let (barrier_sender, barrier_receiver) = sync_channel(0);
        match sender.try_send(CompletionCommand::Barrier(barrier_sender)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.request_completion_abort();
                bail!("bounded NVENC completion queue was full before drain barrier");
            }
            Err(TrySendError::Disconnected(_)) => {
                self.request_completion_abort();
                bail!("NVENC completion thread closed before drain barrier");
            }
        }
        match barrier_receiver.recv_timeout(COMPLETION_ACK_TIMEOUT) {
            Ok(()) => {}
            Err(RecvTimeoutError::Timeout) => {
                self.request_completion_abort();
                bail!(
                    "NVENC completion thread did not acknowledge drain within {} seconds",
                    COMPLETION_ACK_TIMEOUT.as_secs_f64()
                );
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.request_completion_abort();
                bail!("NVENC completion thread closed without acknowledging drain");
            }
        }
        if self
            .completion_telemetry
            .completion_errors
            .load(Ordering::Acquire)
            != 0
        {
            return Err(self.completion_failure("native NVENC completion/output drain failed"));
        }
        ensure!(
            self.states.all_free(),
            "native NVENC drain completed with submitted slots still owned"
        );
        Ok(())
    }

    pub fn finish(mut self) -> Result<NativeNvencTelemetrySnapshot> {
        self.close_inner()?;
        Ok(self.telemetry())
    }

    fn encoder_handle(&self) -> Result<EncoderHandle> {
        self.session
            .as_ref()
            .map(EncoderSession::handle)
            .context("native NVENC session is closed")
    }

    fn completion_failure(&self, operation: &str) -> anyhow::Error {
        match self.completion_telemetry.error_detail() {
            Some(detail) => anyhow!("{operation}: {detail}"),
            None => anyhow!(operation.to_owned()),
        }
    }

    fn map_input(&mut self, registered: NvencObjectHandle) -> Result<NvencObjectHandle> {
        let mut params = NvencBlob::<MAP_INPUT_RESOURCE_SIZE>::zeroed();
        params.write(0, MAP_INPUT_RESOURCE_VERSION);
        params.write(16, registered.as_ptr());
        let encoder = self.encoder_handle()?;
        // SAFETY: the resource was registered once on this session and the
        // exact-layout blob remains writable for the returned mapped token.
        let status =
            unsafe { (self.api.map_input_resource)(encoder.as_ptr(), params.as_mut_void()) };
        nvenc_status(status, "nvEncMapInputResource")?;
        let Some(mapped) = NonNull::new(params.read_mut_ptr(24)).map(NvencObjectHandle) else {
            // A successful map without a token contradicts the pinned API
            // contract. Do not continue cleanup through a session whose
            // resource ownership can no longer be established safely.
            self.abort_cleanup = true;
            self.completion_telemetry
                .abort_cleanup
                .store(true, Ordering::Release);
            bail!("NVENC mapped a resource without returning an input handle");
        };
        let format = params.read_u32(32);
        if format != NV_ENC_BUFFER_FORMAT_NV12 {
            let format_error =
                anyhow!("NVENC mapped M3 NV12 texture as unexpected format 0x{format:08x}");
            let unmap_error = self.unmap_rejected(mapped).err();
            if unmap_error.is_some() {
                self.abort_cleanup = true;
                self.completion_telemetry
                    .abort_cleanup
                    .store(true, Ordering::Release);
            }
            return Err(combine_initialization_errors(
                format_error,
                unmap_error,
                None,
            ));
        }
        Ok(mapped)
    }

    fn unmap_rejected(&self, mapped: NvencObjectHandle) -> Result<()> {
        let encoder = self.encoder_handle()?;
        // SAFETY: this mapping was not accepted for asynchronous encoding and
        // is invalidated once on the same live encoder session.
        let status = unsafe { (self.api.unmap_input_resource)(encoder.as_ptr(), mapped.as_ptr()) };
        nvenc_status(status, "nvEncUnmapInputResource after rejected submission")
    }

    fn close_inner(&mut self) -> Result<()> {
        let Some(sender) = self.completion_sender.take() else {
            return Ok(());
        };
        let mut first_error = None;

        for task in self.orphaned_completions.drain(..) {
            match sender.try_send(CompletionCommand::Frame(task)) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    self.abort_cleanup = true;
                    remember_error(
                        &mut first_error,
                        Some(anyhow!(
                            "bounded NVENC completion queue filled during shutdown drain"
                        )),
                    );
                    break;
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.abort_cleanup = true;
                    remember_error(
                        &mut first_error,
                        Some(anyhow!(
                            "NVENC completion thread closed during shutdown drain"
                        )),
                    );
                    break;
                }
            }
        }

        let completion_failed = self
            .completion_telemetry
            .completion_errors
            .load(Ordering::Acquire)
            != 0;
        if !self.abort_cleanup
            && !completion_failed
            && !self
                .completion_telemetry
                .abort_cleanup
                .load(Ordering::Acquire)
        {
            match self.prepare_eos_event().and_then(|event| {
                self.submit_eos(event)?;
                Ok(event)
            }) {
                Ok(event) => {
                    // EOS is submitted before the completion-thread barrier.
                    // This lets NVENC release samples accepted with
                    // NEED_MORE_INPUT while the command order still drains
                    // every frame before waiting for the dedicated EOS event.
                    let (eos_sender, eos_receiver) = sync_channel(0);
                    match sender.try_send(CompletionCommand::Eos {
                        event,
                        acknowledged: eos_sender,
                    }) {
                        Ok(()) => match eos_receiver.recv_timeout(COMPLETION_ACK_TIMEOUT) {
                            Ok(()) => {}
                            Err(RecvTimeoutError::Timeout) => {
                                self.abort_cleanup = true;
                                remember_error(
                                    &mut first_error,
                                    Some(anyhow!(
                                        "NVENC frame/EOS drain exceeded {} seconds",
                                        COMPLETION_ACK_TIMEOUT.as_secs_f64()
                                    )),
                                );
                            }
                            Err(RecvTimeoutError::Disconnected) => {
                                self.abort_cleanup = true;
                                remember_error(
                                    &mut first_error,
                                    Some(anyhow!("NVENC frame/EOS drain was not acknowledged")),
                                );
                            }
                        },
                        Err(TrySendError::Full(_)) => {
                            self.abort_cleanup = true;
                            remember_error(
                                &mut first_error,
                                Some(anyhow!(
                                    "bounded NVENC completion queue was full before EOS drain"
                                )),
                            );
                        }
                        Err(TrySendError::Disconnected(_)) => {
                            self.abort_cleanup = true;
                            remember_error(
                                &mut first_error,
                                Some(anyhow!("NVENC completion thread closed before EOS drain")),
                            );
                        }
                    }
                }
                Err(error) => {
                    self.abort_cleanup = true;
                    remember_error(&mut first_error, Some(error));
                }
            }

            if !self.abort_cleanup && !self.states.all_free() {
                self.abort_cleanup = true;
                remember_error(
                    &mut first_error,
                    Some(anyhow!(
                        "native NVENC EOS drain completed with submitted slots still owned"
                    )),
                );
            }
        }

        if self.abort_cleanup
            || self
                .completion_telemetry
                .abort_cleanup
                .load(Ordering::Acquire)
        {
            remember_error(&mut first_error, self.request_completion_abort());
        }
        drop(sender);

        let initial_join_timeout = if self.abort_cleanup {
            COMPLETION_ABORT_GRACE
        } else {
            COMPLETION_JOIN_TIMEOUT
        };
        let mut joined = self.wait_and_join_completion(initial_join_timeout, &mut first_error);
        if !joined {
            remember_error(&mut first_error, self.request_completion_abort());
            joined = self.wait_and_join_completion(COMPLETION_ABORT_GRACE, &mut first_error);
        }
        if !joined {
            self.abort_cleanup = true;
            remember_error(
                &mut first_error,
                Some(anyhow!(
                    "NVENC completion/output thread remained stuck after cancellation; native resources were deliberately retained"
                )),
            );
            self.abandon_native_resources();
            return Err(first_error.expect("stuck completion thread must record an error"));
        }

        let encoder = self.session.as_ref().map(EncoderSession::handle);
        let native_cleanup_safe = !self.abort_cleanup
            && !self
                .completion_telemetry
                .abort_cleanup
                .load(Ordering::Acquire);
        if native_cleanup_safe
            && let (Some(encoder), Some(event)) = (encoder, self.eos_event.as_ref())
        {
            let mut params = event_params(event.handle());
            // SAFETY: EOS has completed, the event remains live and registered
            // on this encoder, and no later submission can reference it.
            let status = unsafe {
                (self.api.unregister_async_event)(encoder.as_ptr(), params.as_mut_void())
            };
            remember_error(
                &mut first_error,
                nvenc_status(status, "nvEncUnregisterAsyncEvent (EOS)").err(),
            );
        }
        if native_cleanup_safe && let (Some(encoder), Some(slots)) = (encoder, self.slots.as_mut())
        {
            remember_error(
                &mut first_error,
                cleanup_slot_resources(self.api, encoder, slots),
            );
        }
        if let Some(session) = self.session.as_mut() {
            remember_error(&mut first_error, session.close().err());
        }
        self.session = None;
        self.slots = None;
        self.eos_event = None;
        self.completion_cancellation = None;
        self.abandoned_mappings.clear();

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn prepare_eos_event(&mut self) -> Result<EventHandle> {
        if let Some(event) = self.eos_event.as_ref() {
            return Ok(event.handle());
        }

        let event = OwnedEvent::auto_reset()
            .context("could not create dedicated NVENC EOS completion event")?;
        let event_handle = event.handle();
        let mut params = event_params(event_handle);
        let encoder = self.encoder_handle()?;
        // SAFETY: this fresh event and exact-layout params stay live; after a
        // successful registration ownership moves into the encoder until
        // normal unregister or deliberate stuck-thread retention.
        let status =
            unsafe { (self.api.register_async_event)(encoder.as_ptr(), params.as_mut_void()) };
        nvenc_status(status, "nvEncRegisterAsyncEvent (EOS)")?;
        self.eos_event = Some(event);
        Ok(event_handle)
    }

    fn request_completion_abort(&mut self) -> Option<anyhow::Error> {
        self.abort_cleanup = true;
        self.completion_telemetry
            .abort_cleanup
            .store(true, Ordering::Release);
        let mut first_error = None;
        if let Some(event) = self.completion_cancellation.as_ref() {
            // SAFETY: the manual-reset cancellation event remains owned by the
            // encoder until the completion thread joins or all native owners
            // are deliberately retained with a detached stuck thread.
            remember_error(
                &mut first_error,
                unsafe { SetEvent(event.handle().0) }
                    .context("could not signal NVENC completion cancellation")
                    .err(),
            );
        }
        if let Some(thread) = self.completion_thread.as_ref() {
            let thread_handle = HANDLE(thread.as_raw_handle());
            // SAFETY: `as_raw_handle` is the live OS handle for the completion
            // thread. ERROR_NOT_FOUND simply means it was not currently in a
            // cancellable synchronous write, so cancellation-event signaling
            // remains the primary wakeup and this result is intentionally
            // advisory.
            let _ = unsafe { CancelSynchronousIo(thread_handle) };
        }
        first_error
    }

    fn wait_and_join_completion(
        &mut self,
        timeout: Duration,
        first_error: &mut Option<anyhow::Error>,
    ) -> bool {
        match self.completion_done.recv_timeout(timeout) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                if let Some(thread) = self.completion_thread.take() {
                    match thread.join() {
                        Ok(result) => remember_error(first_error, result.err()),
                        Err(_) => remember_error(
                            first_error,
                            Some(anyhow!("NVENC completion thread panicked")),
                        ),
                    }
                }
                true
            }
            Err(RecvTimeoutError::Timeout) => false,
        }
    }

    fn abandon_native_resources(&mut self) {
        // A detached thread may still be executing a driver call or output
        // writer. Retaining every object/function-table owner is preferable to
        // use-after-free. This exceptional path is bounded and terminal for
        // the recording; the process can still exit normally.
        if let Some(session) = self.session.take() {
            std::mem::forget(session);
        }
        if let Some(slots) = self.slots.take() {
            std::mem::forget(slots);
        }
        if let Some(event) = self.completion_cancellation.take() {
            std::mem::forget(event);
        }
        if let Some(event) = self.eos_event.take() {
            std::mem::forget(event);
        }
        if let Some(driver) = self.driver.take() {
            std::mem::forget(driver);
        }
        self.completion_thread = None;
    }

    fn submit_eos(&self, event: EventHandle) -> Result<()> {
        let mut params = NvencBlob::<PIC_PARAMS_SIZE>::zeroed();
        params.write(0, PIC_PARAMS_VERSION);
        params.write(16, NV_ENC_PIC_FLAG_EOS);
        params.write(56, event.as_ptr());
        let encoder = self.encoder_handle()?;
        // SAFETY: the dedicated registered event is not used by a frame and
        // the exact-layout EOS params remain live for the call. Prior samples
        // may still be outstanding specifically so EOS can release them.
        let status = unsafe { (self.api.encode_picture)(encoder.as_ptr(), params.as_mut_void()) };
        nvenc_status(status, "nvEncEncodePicture (EOS)")
    }
}

impl Drop for NativeNvencEncoder {
    fn drop(&mut self) {
        let _ = self.close_inner();
    }
}

fn combine_initialization_errors(
    primary: anyhow::Error,
    cleanup: Option<anyhow::Error>,
    session: Option<anyhow::Error>,
) -> anyhow::Error {
    match (cleanup, session) {
        (None, None) => primary,
        (cleanup, session) => anyhow!(
            "{primary:#}; cleanup errors: resource={}; session={}",
            cleanup
                .as_ref()
                .map_or_else(|| "none".to_owned(), |error| format!("{error:#}")),
            session
                .as_ref()
                .map_or_else(|| "none".to_owned(), |error| format!("{error:#}")),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_opaque_layouts_match_pinned_12_2_c_probe() {
        assert_eq!(std::mem::size_of::<NvencBlob<CONFIG_SIZE>>(), 3_584);
        assert_eq!(std::mem::size_of::<NvencBlob<PRESET_CONFIG_SIZE>>(), 5_128);
        assert_eq!(
            std::mem::size_of::<NvencBlob<INITIALIZE_PARAMS_SIZE>>(),
            1_800
        );
        assert_eq!(std::mem::size_of::<NvencBlob<CREATE_BITSTREAM_SIZE>>(), 776);
        assert_eq!(
            std::mem::size_of::<NvencBlob<REGISTER_RESOURCE_SIZE>>(),
            1_536
        );
        assert_eq!(
            std::mem::size_of::<NvencBlob<MAP_INPUT_RESOURCE_SIZE>>(),
            1_544
        );
        assert_eq!(std::mem::size_of::<NvencBlob<PIC_PARAMS_SIZE>>(), 3_360);
        assert_eq!(std::mem::size_of::<NvencBlob<LOCK_BITSTREAM_SIZE>>(), 1_544);
        assert_eq!(std::mem::size_of::<NvencBlob<EVENT_PARAMS_SIZE>>(), 1_544);
        assert_eq!(std::mem::align_of::<NvencBlob<CONFIG_SIZE>>(), 8);
        assert_eq!(std::mem::align_of::<NvencBlob<PIC_PARAMS_SIZE>>(), 8);
    }

    #[test]
    fn pinned_structure_versions_match_header_probe() {
        assert_eq!(super::super::nvenc::NVENC_API_VERSION, 0x0200_000c);
        assert_eq!(CONFIG_VERSION, 0xf209_000c);
        assert_eq!(RC_PARAMS_VERSION, 0x7201_000c);
        assert_eq!(INITIALIZE_PARAMS_VERSION, 0xf207_000c);
        assert_eq!(PRESET_CONFIG_VERSION, 0xf205_000c);
        assert_eq!(CREATE_BITSTREAM_VERSION, 0x7201_000c);
        assert_eq!(REGISTER_RESOURCE_VERSION, 0x7205_000c);
        assert_eq!(MAP_INPUT_RESOURCE_VERSION, 0x7204_000c);
        assert_eq!(PIC_PARAMS_VERSION, 0xf207_000c);
        assert_eq!(LOCK_BITSTREAM_VERSION, 0xf202_000c);
        assert_eq!(EVENT_PARAMS_VERSION, 0x7202_000c);
    }

    #[test]
    fn function_indices_match_pinned_function_list_offsets() {
        assert_eq!(8 + FUNCTION_INITIALIZE_ENCODER * 8, 96);
        assert_eq!(8 + FUNCTION_CREATE_BITSTREAM_BUFFER * 8, 120);
        assert_eq!(8 + FUNCTION_DESTROY_BITSTREAM_BUFFER * 8, 128);
        assert_eq!(8 + FUNCTION_ENCODE_PICTURE * 8, 136);
        assert_eq!(8 + FUNCTION_LOCK_BITSTREAM * 8, 144);
        assert_eq!(8 + FUNCTION_UNLOCK_BITSTREAM * 8, 152);
        assert_eq!(8 + FUNCTION_REGISTER_ASYNC_EVENT * 8, 192);
        assert_eq!(8 + FUNCTION_UNREGISTER_ASYNC_EVENT * 8, 200);
        assert_eq!(8 + FUNCTION_MAP_INPUT_RESOURCE * 8, 208);
        assert_eq!(8 + FUNCTION_UNMAP_INPUT_RESOURCE * 8, 216);
        assert_eq!(8 + FUNCTION_DESTROY_ENCODER * 8, 224);
        assert_eq!(8 + FUNCTION_OPEN_SESSION_EX * 8, 240);
        assert_eq!(8 + FUNCTION_REGISTER_RESOURCE * 8, 248);
        assert_eq!(8 + FUNCTION_UNREGISTER_RESOURCE * 8, 256);
        assert_eq!(8 + FUNCTION_GET_PRESET_CONFIG_EX * 8, 320);
    }

    #[test]
    fn h264_configuration_enforces_audited_quality_and_latency_contract() {
        let mut config = NvencBlob::<CONFIG_SIZE>::zeroed();
        configure_h264(&mut config);

        assert_eq!(config.read_u32(0), CONFIG_VERSION);
        assert_eq!(config.read_guid(4), H264_HIGH_PROFILE_GUID);
        assert_eq!(config.read_u32(20), 120);
        assert_eq!(config.read_i32(24), 1);
        assert_eq!(config.read_u32(32), 1);
        assert_eq!(config.read_u32(40), RC_PARAMS_VERSION);
        assert_eq!(config.read_u32(44), NV_ENC_PARAMS_RC_VBR);
        assert_eq!(config.read_u32(60), 12_000_000);
        assert_eq!(config.read_u32(64), 18_000_000);
        assert_eq!(config.read_u32(68), 24_000_000);
        assert_eq!(config.read_u32(76), 0);
        assert_eq!(config.read_u16(130), 0);
        assert_eq!(config.read_u32(140), 0);
        assert_eq!(config.read_u32(176), 120);
        assert_eq!(config.read_u32(248), 1);
        assert_eq!(config.read_u32(256), 0);
        assert_eq!(config.read_u32(260), 1);
        assert_eq!(config.read_u32(264), 1);
        assert_eq!(config.read_u32(268), 1);
        assert_eq!(config.read_u32(272), 1);
        assert_eq!(config.read_u32(360), 1);
        assert_eq!(config.read_u32(380), 8);
        assert_eq!(config.read_u32(384), 8);
    }

    #[test]
    fn encode_picture_status_accepts_success_and_need_more_input_only() {
        assert_eq!(
            classify_encode_picture_status(NVENC_SUCCESS, "test").unwrap(),
            EncodePictureStatus::Accepted
        );
        assert_eq!(
            classify_encode_picture_status(NVENC_ERR_NEED_MORE_INPUT, "test").unwrap(),
            EncodePictureStatus::AcceptedNeedsMoreInput
        );
        assert!(classify_encode_picture_status(8, "test").is_err());
    }

    #[test]
    fn completion_wait_observes_frame_and_cancellation_events() {
        let completion = OwnedEvent::auto_reset().unwrap();
        let cancellation = OwnedEvent::manual_reset().unwrap();
        // SAFETY: both events are live and owned for the duration of the wait.
        unsafe { SetEvent(completion.handle().0) }.unwrap();
        wait_for_event_with_timeout(
            completion.handle(),
            cancellation.handle(),
            "test completion",
            50,
        )
        .unwrap();

        // SAFETY: the manual-reset cancellation event remains live/owned.
        unsafe { SetEvent(cancellation.handle().0) }.unwrap();
        assert!(
            wait_for_event_with_timeout(
                completion.handle(),
                cancellation.handle(),
                "test cancellation",
                50,
            )
            .is_err()
        );
    }

    #[test]
    fn completion_wait_has_a_finite_timeout() {
        let completion = OwnedEvent::auto_reset().unwrap();
        let cancellation = OwnedEvent::manual_reset().unwrap();
        let started = std::time::Instant::now();
        let error = wait_for_event_with_timeout(
            completion.handle(),
            cancellation.handle(),
            "test timeout",
            10,
        )
        .unwrap_err();
        assert!(error.to_string().contains("deadline"));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn completion_error_detail_preserves_the_first_output_failure() {
        let telemetry = CompletionTelemetry::default();
        telemetry.record_error(&anyhow!("mux pipe closed"));
        telemetry.record_error(&anyhow!("later cleanup failure"));

        assert_eq!(telemetry.error_detail().as_deref(), Some("mux pipe closed"));
        assert_eq!(telemetry.completion_errors.load(Ordering::Acquire), 2);
        assert!(telemetry.abort_cleanup.load(Ordering::Acquire));
    }
}
