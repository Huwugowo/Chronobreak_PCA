use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use anyhow::{Context, Result, ensure};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_VIDEO_ENCODER, D3D11_BOX, D3D11_TEX2D_VPIV,
    D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_CAPS,
    D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT,
    D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM,
    D3D11_VIDEO_USAGE_PLAYBACK_NORMAL, D3D11_VPIV_DIMENSION_TEXTURE2D,
    D3D11_VPOV_DIMENSION_TEXTURE2D, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    ID3D11VideoContext1, ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator,
    ID3D11VideoProcessorEnumerator1, ID3D11VideoProcessorInputView, ID3D11VideoProcessorOutputView,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709,
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::core::Interface;

use super::capture::CapturedWgcFrame;
use super::d3d11::{NativeD3d11Device, NativeSourceTexture};
use super::{NATIVE_ENCODER_SLOT_COUNT, NATIVE_WGC_FRAME_POOL_CAPACITY};

const NV12_SLOT_FREE: u8 = 0;
const NV12_SLOT_CONVERTED: u8 = 1;
const NV12_SLOT_SUBMITTED: u8 = 2;

/// Thread-safe ownership states for the fixed texture ring. The atomics carry
/// ownership only; the D3D11 immediate/video contexts never leave the worker.
pub(super) struct NativeNv12SlotStates {
    states: [AtomicU8; NATIVE_ENCODER_SLOT_COUNT],
}

impl NativeNv12SlotStates {
    fn new() -> Self {
        Self {
            states: std::array::from_fn(|_| AtomicU8::new(NV12_SLOT_FREE)),
        }
    }

    fn try_acquire_converted(&self) -> Option<usize> {
        self.states.iter().position(|state| {
            state
                .compare_exchange(
                    NV12_SLOT_FREE,
                    NV12_SLOT_CONVERTED,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_ok()
        })
    }

    fn release_converted(&self, slot_index: usize) {
        debug_assert_eq!(
            self.states[slot_index].load(Ordering::Relaxed),
            NV12_SLOT_CONVERTED
        );
        self.states[slot_index].store(NV12_SLOT_FREE, Ordering::Release);
    }

    fn mark_submitted(&self, slot_index: usize) -> Result<()> {
        self.states[slot_index]
            .compare_exchange(
                NV12_SLOT_CONVERTED,
                NV12_SLOT_SUBMITTED,
                Ordering::Release,
                Ordering::Relaxed,
            )
            .map(|_| ())
            .map_err(|actual| {
                anyhow::anyhow!(
                    "native NV12 slot {slot_index} changed from converted before NVENC submission (state {actual})"
                )
            })
    }

    pub(super) fn complete_submitted(&self, slot_index: usize) -> Result<()> {
        self.states[slot_index]
            .compare_exchange(
                NV12_SLOT_SUBMITTED,
                NV12_SLOT_FREE,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|actual| {
                anyhow::anyhow!(
                    "native NV12 slot {slot_index} completed outside submitted state (state {actual})"
                )
            })
    }

    pub(super) fn all_free(&self) -> bool {
        self.states
            .iter()
            .all(|state| state.load(Ordering::Acquire) == NV12_SLOT_FREE)
    }

    fn free_count(&self) -> usize {
        self.states
            .iter()
            .filter(|state| state.load(Ordering::Acquire) == NV12_SLOT_FREE)
            .count()
    }
}

struct NativeNv12Slot {
    texture: ID3D11Texture2D,
    output_view: ID3D11VideoProcessorOutputView,
}

struct CachedInputView {
    texture_identity: usize,
    view: ID3D11VideoProcessorInputView,
}

struct LatestSourceSnapshot {
    texture: ID3D11Texture2D,
    view: ID3D11VideoProcessorInputView,
    populated: bool,
}

/// Worker-owned accounting for the fixed GPU conversion resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeNv12TelemetrySnapshot {
    pub converted_frames: u64,
    pub no_free_slot_drops: u64,
    pub processor_state_configurations: u64,
    pub slot_texture_allocations: u64,
    pub input_view_creations: u64,
    pub input_view_replacements: u64,
    pub input_view_cache_resets: u64,
    pub processor_recreations: u64,
    pub output_view_recreations: u64,
    pub source_snapshot_allocations: u64,
    pub source_snapshot_copies: u64,
}

#[derive(Default)]
struct NativeNv12Telemetry {
    converted_frames: u64,
    no_free_slot_drops: u64,
    processor_state_configurations: u64,
    slot_texture_allocations: u64,
    input_view_creations: u64,
    input_view_replacements: u64,
    input_view_cache_resets: u64,
    processor_recreations: u64,
    output_view_recreations: u64,
    source_snapshot_allocations: u64,
    source_snapshot_copies: u64,
}

impl NativeNv12Telemetry {
    fn snapshot(&self) -> NativeNv12TelemetrySnapshot {
        NativeNv12TelemetrySnapshot {
            converted_frames: self.converted_frames,
            no_free_slot_drops: self.no_free_slot_drops,
            processor_state_configurations: self.processor_state_configurations,
            slot_texture_allocations: self.slot_texture_allocations,
            input_view_creations: self.input_view_creations,
            input_view_replacements: self.input_view_replacements,
            input_view_cache_resets: self.input_view_cache_resets,
            processor_recreations: self.processor_recreations,
            output_view_recreations: self.output_view_recreations,
            source_snapshot_allocations: self.source_snapshot_allocations,
            source_snapshot_copies: self.source_snapshot_copies,
        }
    }
}

/// One same-device D3D11 video processor and exactly four reusable GPU-only
/// NV12 output textures. This value is deliberately !Send/!Sync: all video
/// context calls remain on the worker that constructed the native source.
pub struct NativeNv12Converter {
    d3d_device: ID3D11Device,
    immediate_context: ID3D11DeviceContext,
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext1,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    slots: [NativeNv12Slot; NATIVE_ENCODER_SLOT_COUNT],
    slot_states: Arc<NativeNv12SlotStates>,
    input_views: Vec<CachedInputView>,
    next_input_view_replacement: usize,
    latest_source: LatestSourceSnapshot,
    input_width: u32,
    input_height: u32,
    output_width: u32,
    output_height: u32,
    fps: u32,
    output_frame_index: u32,
    telemetry: NativeNv12Telemetry,
    _thread_affinity: PhantomData<Rc<()>>,
}

impl NativeNv12Converter {
    pub(crate) fn new(
        device: &NativeD3d11Device,
        input_width: u32,
        input_height: u32,
        output_width: u32,
        output_height: u32,
        fps: u32,
    ) -> Result<Self> {
        validate_dimensions(input_width, input_height, output_width, output_height, fps)?;
        let d3d_device = device.device().clone();
        let video_device = device.video_device().clone();
        let video_context = device.video_context().clone();
        let immediate_context: ID3D11DeviceContext = video_context
            .cast()
            .context("could not recover the same-device D3D11 immediate context")?;
        let (enumerator, processor) = create_processor(
            &video_device,
            input_width,
            input_height,
            output_width,
            output_height,
            fps,
        )?;
        let slots = create_slots(
            &d3d_device,
            &video_device,
            &enumerator,
            output_width,
            output_height,
        )?;
        let latest_source = create_source_snapshot(
            &d3d_device,
            &video_device,
            &enumerator,
            input_width,
            input_height,
        )?;

        Ok(Self {
            d3d_device,
            immediate_context,
            video_device,
            video_context,
            enumerator,
            processor,
            slots,
            slot_states: Arc::new(NativeNv12SlotStates::new()),
            input_views: Vec::with_capacity(NATIVE_WGC_FRAME_POOL_CAPACITY as usize),
            next_input_view_replacement: 0,
            latest_source,
            input_width,
            input_height,
            output_width,
            output_height,
            fps,
            output_frame_index: 0,
            telemetry: NativeNv12Telemetry {
                slot_texture_allocations: NATIVE_ENCODER_SLOT_COUNT as u64,
                source_snapshot_allocations: 1,
                ..NativeNv12Telemetry::default()
            },
            _thread_affinity: PhantomData,
        })
    }

    /// Rebuild the size-dependent processor and lightweight views after WGC
    /// pool recreation. The four full-size NV12 textures are retained.
    pub fn reconfigure_input(&mut self, input_width: u32, input_height: u32) -> Result<()> {
        ensure!(
            input_width > 0 && input_height > 0,
            "native WGC resize is empty"
        );
        ensure!(
            self.slot_states.all_free(),
            "cannot reconfigure native conversion while an NV12 slot is leased"
        );
        if (input_width, input_height) == (self.input_width, self.input_height) {
            return Ok(());
        }

        let (enumerator, processor) = create_processor(
            &self.video_device,
            input_width,
            input_height,
            self.output_width,
            self.output_height,
            self.fps,
        )?;
        let output_views = create_output_views(&self.video_device, &enumerator, &self.slots)?;
        let latest_source = create_source_snapshot(
            &self.d3d_device,
            &self.video_device,
            &enumerator,
            input_width,
            input_height,
        )?;

        self.input_views.clear();
        self.next_input_view_replacement = 0;
        self.telemetry.input_view_cache_resets =
            self.telemetry.input_view_cache_resets.saturating_add(1);
        self.enumerator = enumerator;
        self.processor = processor;
        self.latest_source = latest_source;
        for (slot, view) in self.slots.iter_mut().zip(output_views) {
            slot.output_view = view;
        }
        self.input_width = input_width;
        self.input_height = input_height;
        self.telemetry.processor_recreations =
            self.telemetry.processor_recreations.saturating_add(1);
        self.telemetry.output_view_recreations = self
            .telemetry
            .output_view_recreations
            .saturating_add(NATIVE_ENCODER_SLOT_COUNT as u64);
        self.telemetry.source_snapshot_allocations =
            self.telemetry.source_snapshot_allocations.saturating_add(1);
        Ok(())
    }

    /// Copy the newest admitted WGC surface into one persistent same-device
    /// BGRA texture. This releases the capacity-two WGC pool surface promptly
    /// while preserving GPU-resident pixels for future CFR duplicate ticks.
    pub fn stage_latest_source(&mut self, frame: &CapturedWgcFrame<'_>) -> Result<()> {
        ensure!(
            frame.dimensions() == (self.input_width, self.input_height),
            "native WGC content size {:?} does not match source snapshot {}x{}; reconfigure first",
            frame.dimensions(),
            self.input_width,
            self.input_height
        );
        let source = frame.source_texture()?;
        ensure!(
            source.desc().Format == DXGI_FORMAT_B8G8R8A8_UNORM,
            "native WGC source format {:?} is not BGRA8 UNORM",
            source.desc().Format
        );
        ensure!(
            source.desc().Width >= self.input_width && source.desc().Height >= self.input_height,
            "native WGC source texture {}x{} is smaller than content {}x{}",
            source.desc().Width,
            source.desc().Height,
            self.input_width,
            self.input_height
        );
        let source_box = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: self.input_width,
            bottom: self.input_height,
            back: 1,
        };
        // SAFETY: both textures belong to the worker-owned same D3D11 device,
        // use BGRA8 mip zero, and the checked source dimensions contain the
        // exact destination-sized box. The worker exclusively submits context
        // commands, and D3D11 preserves their order before later video blits.
        unsafe {
            self.immediate_context.CopySubresourceRegion(
                &self.latest_source.texture,
                0,
                0,
                0,
                0,
                source.texture(),
                0,
                Some(&source_box),
            );
        }
        self.latest_source.populated = true;
        self.telemetry.source_snapshot_copies =
            self.telemetry.source_snapshot_copies.saturating_add(1);
        Ok(())
    }

    /// Convert the persistent latest-source snapshot at an exact CFR tick.
    /// A full four-slot ring is an accounted drop, never a wait or allocation.
    pub fn convert_staged<'converter>(
        &'converter mut self,
        qpc_100ns: i64,
    ) -> Result<Option<ConvertedNv12Frame<'converter>>> {
        ensure!(
            self.latest_source.populated,
            "native CFR tick requested before the first source snapshot"
        );
        ensure!(qpc_100ns > 0, "native CFR tick timestamp must be positive");
        let Some(slot_index) = self.slot_states.try_acquire_converted() else {
            self.telemetry.no_free_slot_drops = self.telemetry.no_free_slot_drops.saturating_add(1);
            return Ok(None);
        };

        let input_view = self.latest_source.view.clone();
        if let Err(error) = self.blit_input_view(input_view, slot_index) {
            self.slot_states.release_converted(slot_index);
            return Err(error);
        }

        self.telemetry.converted_frames = self.telemetry.converted_frames.saturating_add(1);
        Ok(Some(ConvertedNv12Frame {
            converter: self,
            slot_index,
            qpc_100ns,
            released: false,
        }))
    }

    /// Convert one live WGC surface into a free fixed slot. A full ring causes
    /// an intentional discard instead of waiting or allocating a fifth slot.
    pub fn convert<'converter>(
        &'converter mut self,
        frame: &CapturedWgcFrame<'_>,
    ) -> Result<Option<ConvertedNv12Frame<'converter>>> {
        ensure!(
            frame.dimensions() == (self.input_width, self.input_height),
            "native WGC content size {:?} does not match converter input {}x{}; reconfigure first",
            frame.dimensions(),
            self.input_width,
            self.input_height
        );
        let Some(slot_index) = self.slot_states.try_acquire_converted() else {
            self.telemetry.no_free_slot_drops = self.telemetry.no_free_slot_drops.saturating_add(1);
            return Ok(None);
        };

        let result = self.convert_into_slot(frame, slot_index);
        if let Err(error) = result {
            self.slot_states.release_converted(slot_index);
            return Err(error);
        }

        let qpc_100ns = frame.qpc_100ns();
        self.telemetry.converted_frames = self.telemetry.converted_frames.saturating_add(1);
        Ok(Some(ConvertedNv12Frame {
            converter: self,
            slot_index,
            qpc_100ns,
            released: false,
        }))
    }

    pub fn telemetry(&self) -> NativeNv12TelemetrySnapshot {
        self.telemetry.snapshot()
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.output_width, self.output_height)
    }

    pub fn free_slot_count(&self) -> usize {
        self.slot_states.free_count()
    }

    /// Clones the four texture interfaces once for M4 registration and shares
    /// only their ownership state with the completion thread.
    pub(super) fn encoder_resources(
        &self,
    ) -> (
        [ID3D11Texture2D; NATIVE_ENCODER_SLOT_COUNT],
        Arc<NativeNv12SlotStates>,
    ) {
        (
            std::array::from_fn(|index| self.slots[index].texture.clone()),
            Arc::clone(&self.slot_states),
        )
    }

    fn convert_into_slot(&mut self, frame: &CapturedWgcFrame<'_>, slot_index: usize) -> Result<()> {
        let source = frame.source_texture()?;
        ensure!(
            source.desc().Format == DXGI_FORMAT_B8G8R8A8_UNORM,
            "native WGC source format {:?} is not BGRA8 UNORM",
            source.desc().Format
        );
        ensure!(
            source.desc().Width >= self.input_width && source.desc().Height >= self.input_height,
            "native WGC source texture {}x{} is smaller than content {}x{}",
            source.desc().Width,
            source.desc().Height,
            self.input_width,
            self.input_height
        );
        let input_view_index = self.ensure_input_view(&source)?;
        let input_view = self.input_views[input_view_index].view.clone();
        self.blit_input_view(input_view, slot_index)
    }

    fn blit_input_view(
        &mut self,
        input_view: ID3D11VideoProcessorInputView,
        slot_index: usize,
    ) -> Result<()> {
        let source_rect = center_crop_rect(
            self.input_width,
            self.input_height,
            self.output_width,
            self.output_height,
        )?;
        let destination_rect = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(self.output_width).context("NV12 width exceeds RECT range")?,
            bottom: i32::try_from(self.output_height).context("NV12 height exceeds RECT range")?,
        };

        // SAFETY: the converter owns the processor/context, both RECT values
        // remain live for each call, and stream 0 is supported by the checked
        // processor capability. The worker is the exclusive immediate-context
        // submitter.
        unsafe {
            self.video_context.VideoProcessorSetStreamColorSpace1(
                &self.processor,
                0,
                DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
            );
            self.video_context.VideoProcessorSetOutputColorSpace1(
                &self.processor,
                DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709,
            );
            self.video_context.VideoProcessorSetStreamFrameFormat(
                &self.processor,
                0,
                D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            );
            self.video_context
                .VideoProcessorSetStreamAutoProcessingMode(&self.processor, 0, false);
            self.video_context.VideoProcessorSetStreamSourceRect(
                &self.processor,
                0,
                true,
                Some(&source_rect),
            );
            self.video_context.VideoProcessorSetStreamDestRect(
                &self.processor,
                0,
                true,
                Some(&destination_rect),
            );
            self.video_context.VideoProcessorSetOutputTargetRect(
                &self.processor,
                true,
                Some(&destination_rect),
            );
        }
        self.telemetry.processor_state_configurations = self
            .telemetry
            .processor_state_configurations
            .saturating_add(1);

        let mut stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: true.into(),
            pInputSurface: ManuallyDrop::new(Some(input_view)),
            ..D3D11_VIDEO_PROCESSOR_STREAM::default()
        };
        // SAFETY: the input/output views, processor and stream storage remain
        // live for the call. They were created for the same enumerator/device;
        // no other thread submits work through this immediate video context.
        let blit = unsafe {
            self.video_context.VideoProcessorBlt(
                &self.processor,
                &self.slots[slot_index].output_view,
                self.output_frame_index,
                std::slice::from_ref(&stream),
            )
        };
        // SAFETY: this drops exactly the owned COM clone placed into the
        // ManuallyDrop field above, after the synchronous API call returned.
        unsafe { ManuallyDrop::drop(&mut stream.pInputSurface) };
        blit.context("D3D11 video processor BGRA-to-NV12 conversion failed")?;
        self.output_frame_index = self.output_frame_index.wrapping_add(1);
        Ok(())
    }

    fn ensure_input_view(&mut self, source: &NativeSourceTexture<'_>) -> Result<usize> {
        let texture_identity = source.texture().as_raw() as usize;
        if let Some(index) = self
            .input_views
            .iter()
            .position(|cached| cached.texture_identity == texture_identity)
        {
            return Ok(index);
        }
        let descriptor = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
            FourCC: 0,
            ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPIV {
                    MipSlice: 0,
                    ArraySlice: 0,
                },
            },
        };
        let mut view = None;
        // SAFETY: the WGC texture is live through `source`, the descriptor is
        // initialized for a non-array 2D surface, and the out-pointer targets
        // a valid local Option. The same-device enumerator is retained.
        unsafe {
            self.video_device.CreateVideoProcessorInputView(
                source.texture(),
                &self.enumerator,
                &descriptor,
                Some(&mut view),
            )
        }
        .context("could not create cached WGC video-processor input view")?;
        let view = view.context("D3D11 returned no video-processor input view")?;
        let cached = CachedInputView {
            texture_identity,
            view,
        };
        let (index, next_replacement, replaced) =
            input_cache_insertion(self.input_views.len(), self.next_input_view_replacement);
        if replaced {
            self.input_views[index] = cached;
            self.telemetry.input_view_replacements =
                self.telemetry.input_view_replacements.saturating_add(1);
        } else {
            self.input_views.push(cached);
        }
        self.next_input_view_replacement = next_replacement;
        self.telemetry.input_view_creations = self.telemetry.input_view_creations.saturating_add(1);
        Ok(index)
    }

    fn release_slot(&mut self, slot_index: usize) {
        self.slot_states.release_converted(slot_index);
    }
}

/// RAII ownership of one converted slot. Milestone 4 will transition this
/// lease into NVENC submission; until then, dropping it safely frees the slot.
pub struct ConvertedNv12Frame<'converter> {
    converter: &'converter mut NativeNv12Converter,
    slot_index: usize,
    qpc_100ns: i64,
    released: bool,
}

impl ConvertedNv12Frame<'_> {
    pub fn slot_index(&self) -> usize {
        self.slot_index
    }

    pub fn qpc_100ns(&self) -> i64 {
        self.qpc_100ns
    }

    /// Proves that this lease belongs to the exact four-texture ring whose
    /// resources an encoder registered. Pointer identity is unforgeable
    /// through the public API while both live `Arc`s exist.
    pub(super) fn belongs_to(&self, states: &Arc<NativeNv12SlotStates>) -> bool {
        same_ring(&self.converter.slot_states, states)
    }

    pub fn texture_desc(&self) -> D3D11_TEXTURE2D_DESC {
        output_texture_desc(self.converter.output_width, self.converter.output_height)
    }

    pub fn release(mut self) {
        self.release_inner();
    }

    /// Transfers ownership of this slot from conversion to NVENC. The caller
    /// must enqueue exactly one completion for the returned index.
    pub(super) fn mark_submitted(mut self) -> Result<(usize, i64)> {
        if let Err(error) = self.converter.slot_states.mark_submitted(self.slot_index) {
            // The state no longer belongs to this Converted lease, so Drop
            // must not overwrite the unexpected owner with Free.
            self.released = true;
            return Err(error);
        }
        self.released = true;
        Ok((self.slot_index, self.qpc_100ns))
    }

    fn release_inner(&mut self) {
        if !self.released {
            self.converter.release_slot(self.slot_index);
            self.released = true;
        }
    }
}

fn same_ring(
    candidate: &Arc<NativeNv12SlotStates>,
    registered: &Arc<NativeNv12SlotStates>,
) -> bool {
    Arc::ptr_eq(candidate, registered)
}

impl Drop for ConvertedNv12Frame<'_> {
    fn drop(&mut self) {
        self.release_inner();
    }
}

fn create_processor(
    video_device: &ID3D11VideoDevice,
    input_width: u32,
    input_height: u32,
    output_width: u32,
    output_height: u32,
    fps: u32,
) -> Result<(ID3D11VideoProcessorEnumerator, ID3D11VideoProcessor)> {
    let descriptor = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
        InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
        InputFrameRate: DXGI_RATIONAL {
            Numerator: fps,
            Denominator: 1,
        },
        InputWidth: input_width,
        InputHeight: input_height,
        OutputFrameRate: DXGI_RATIONAL {
            Numerator: fps,
            Denominator: 1,
        },
        OutputWidth: output_width,
        OutputHeight: output_height,
        Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
    };
    // SAFETY: the descriptor is fully initialized and retained for the call;
    // `video_device` is the live same-adapter video-capable D3D11 device.
    let enumerator = unsafe { video_device.CreateVideoProcessorEnumerator(&descriptor) }
        .context("could not create D3D11 video-processor enumerator")?;
    // SAFETY: the enumerator is live and each format query only writes its
    // returned support mask.
    let bgra_support = unsafe { enumerator.CheckVideoProcessorFormat(DXGI_FORMAT_B8G8R8A8_UNORM) }
        .context("could not query BGRA video-processor support")?;
    ensure!(
        bgra_support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT.0 as u32 != 0,
        "same-adapter D3D11 video processor cannot consume BGRA8"
    );
    // SAFETY: same invariant as the BGRA format query above.
    let nv12_support = unsafe { enumerator.CheckVideoProcessorFormat(DXGI_FORMAT_NV12) }
        .context("could not query NV12 video-processor support")?;
    ensure!(
        nv12_support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0 as u32 != 0,
        "same-adapter D3D11 video processor cannot produce NV12"
    );
    let enumerator1: ID3D11VideoProcessorEnumerator1 = enumerator
        .cast()
        .context("D3D11 video processor cannot validate color-space conversion")?;
    // SAFETY: the enumerator is live and the exact input/output formats and
    // color spaces describe the conversion configured on the video context.
    let color_conversion = unsafe {
        enumerator1.CheckVideoProcessorFormatConversion(
            DXGI_FORMAT_B8G8R8A8_UNORM,
            DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
            DXGI_FORMAT_NV12,
            DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709,
        )
    }
    .context("could not query exact BGRA/Rec.709-to-NV12 conversion support")?;
    ensure!(
        color_conversion.as_bool(),
        "same-adapter D3D11 video processor cannot convert full-range RGB Rec.709 to studio-range NV12 Rec.709"
    );
    let mut caps = D3D11_VIDEO_PROCESSOR_CAPS::default();
    // SAFETY: `caps` is valid writable storage and the enumerator is live.
    unsafe { enumerator.GetVideoProcessorCaps(&mut caps) }
        .context("could not query D3D11 video-processor capabilities")?;
    ensure!(
        caps.MaxInputStreams >= 1 && caps.RateConversionCapsCount >= 1,
        "D3D11 video processor exposes no usable progressive stream"
    );
    // SAFETY: rate-conversion index zero is valid because the capability count
    // above is nonzero; the returned COM interface is owned by `windows`.
    let processor = unsafe { video_device.CreateVideoProcessor(&enumerator, 0) }
        .context("could not create reusable D3D11 video processor")?;
    Ok((enumerator, processor))
}

fn create_slots(
    device: &ID3D11Device,
    video_device: &ID3D11VideoDevice,
    enumerator: &ID3D11VideoProcessorEnumerator,
    width: u32,
    height: u32,
) -> Result<[NativeNv12Slot; NATIVE_ENCODER_SLOT_COUNT]> {
    let descriptor = output_texture_desc(width, height);
    let mut slots = Vec::with_capacity(NATIVE_ENCODER_SLOT_COUNT);
    for _ in 0..NATIVE_ENCODER_SLOT_COUNT {
        let mut texture = None;
        // SAFETY: the descriptor is fully initialized for a GPU-only NV12
        // texture, no initial subresource data is supplied, and the out-pointer
        // targets valid local storage.
        unsafe { device.CreateTexture2D(&descriptor, None, Some(&mut texture)) }
            .context("could not allocate fixed NV12 encoder slot")?;
        let texture = texture.context("D3D11 returned no NV12 encoder-slot texture")?;
        let output_view = create_output_view(video_device, enumerator, &texture)?;
        slots.push(NativeNv12Slot {
            texture,
            output_view,
        });
    }
    slots.try_into().map_err(|slots: Vec<NativeNv12Slot>| {
        anyhow::anyhow!(
            "native NV12 ring allocated {} slots instead of {}",
            slots.len(),
            NATIVE_ENCODER_SLOT_COUNT
        )
    })
}

fn create_source_snapshot(
    device: &ID3D11Device,
    video_device: &ID3D11VideoDevice,
    enumerator: &ID3D11VideoProcessorEnumerator,
    width: u32,
    height: u32,
) -> Result<LatestSourceSnapshot> {
    let descriptor = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        // Microsoft explicitly permits bind flags zero for a video-processor
        // input view. This snapshot is only a CopySubresourceRegion target and
        // video-processor input; it is never mapped to the CPU.
        BindFlags: 0,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut texture = None;
    // SAFETY: the descriptor is fully initialized for one GPU-only BGRA8
    // texture, no initial data is supplied, and the out-pointer is writable.
    unsafe { device.CreateTexture2D(&descriptor, None, Some(&mut texture)) }
        .context("could not allocate persistent native CFR source snapshot")?;
    let texture = texture.context("D3D11 returned no native CFR source snapshot")?;
    let view = create_input_view(video_device, enumerator, &texture)?;
    Ok(LatestSourceSnapshot {
        texture,
        view,
        populated: false,
    })
}

fn create_input_view(
    video_device: &ID3D11VideoDevice,
    enumerator: &ID3D11VideoProcessorEnumerator,
    texture: &ID3D11Texture2D,
) -> Result<ID3D11VideoProcessorInputView> {
    let descriptor = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
        FourCC: 0,
        ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
        Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
            Texture2D: D3D11_TEX2D_VPIV {
                MipSlice: 0,
                ArraySlice: 0,
            },
        },
    };
    let mut view = None;
    // SAFETY: the texture and enumerator belong to the same device, the
    // descriptor selects mip/array zero, and the out-pointer is writable.
    unsafe {
        video_device.CreateVideoProcessorInputView(
            texture,
            enumerator,
            &descriptor,
            Some(&mut view),
        )
    }
    .context("could not create persistent CFR video-processor input view")?;
    view.context("D3D11 returned no persistent CFR video-processor input view")
}

fn create_output_views(
    video_device: &ID3D11VideoDevice,
    enumerator: &ID3D11VideoProcessorEnumerator,
    slots: &[NativeNv12Slot; NATIVE_ENCODER_SLOT_COUNT],
) -> Result<[ID3D11VideoProcessorOutputView; NATIVE_ENCODER_SLOT_COUNT]> {
    let mut views = Vec::with_capacity(NATIVE_ENCODER_SLOT_COUNT);
    for slot in slots {
        views.push(create_output_view(video_device, enumerator, &slot.texture)?);
    }
    views
        .try_into()
        .map_err(|views: Vec<ID3D11VideoProcessorOutputView>| {
            anyhow::anyhow!(
                "native NV12 ring recreated {} output views instead of {}",
                views.len(),
                NATIVE_ENCODER_SLOT_COUNT
            )
        })
}

fn create_output_view(
    video_device: &ID3D11VideoDevice,
    enumerator: &ID3D11VideoProcessorEnumerator,
    texture: &ID3D11Texture2D,
) -> Result<ID3D11VideoProcessorOutputView> {
    let descriptor = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
        ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
        Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
            Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
        },
    };
    let mut view = None;
    // SAFETY: texture/enumerator are live same-device interfaces, the
    // descriptor selects mip zero of a non-array texture, and the out-pointer
    // targets valid local storage.
    unsafe {
        video_device.CreateVideoProcessorOutputView(
            texture,
            enumerator,
            &descriptor,
            Some(&mut view),
        )
    }
    .context("could not create NV12 video-processor output view")?;
    view.context("D3D11 returned no video-processor output view")
}

fn output_texture_desc(width: u32, height: u32) -> D3D11_TEXTURE2D_DESC {
    D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_NV12,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_RENDER_TARGET | D3D11_BIND_VIDEO_ENCODER).0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    }
}

fn validate_dimensions(
    input_width: u32,
    input_height: u32,
    output_width: u32,
    output_height: u32,
    fps: u32,
) -> Result<()> {
    ensure!(
        input_width > 0 && input_height > 0,
        "native WGC input is empty"
    );
    ensure!(
        output_width > 0 && output_height > 0,
        "native NV12 output is empty"
    );
    ensure!(
        output_width.is_multiple_of(2) && output_height.is_multiple_of(2),
        "native NV12 dimensions must be even, got {output_width}x{output_height}"
    );
    ensure!(fps > 0, "native conversion FPS must be nonzero");
    Ok(())
}

fn center_crop_rect(
    input_width: u32,
    input_height: u32,
    output_width: u32,
    output_height: u32,
) -> Result<RECT> {
    let mut crop_width = input_width & !1;
    let mut crop_height = input_height & !1;
    ensure!(
        crop_width >= 2 && crop_height >= 2,
        "native WGC input is too small for NV12"
    );

    if u64::from(crop_width) * u64::from(output_height)
        > u64::from(crop_height) * u64::from(output_width)
    {
        crop_width = ((u64::from(crop_height) * u64::from(output_width) / u64::from(output_height))
            as u32)
            & !1;
    } else {
        crop_height = ((u64::from(crop_width) * u64::from(output_height) / u64::from(output_width))
            as u32)
            & !1;
    }
    ensure!(
        crop_width >= 2 && crop_height >= 2,
        "native center crop collapsed"
    );
    let left = ((input_width - crop_width) / 2) & !1;
    let top = ((input_height - crop_height) / 2) & !1;
    Ok(RECT {
        left: i32::try_from(left).context("crop left exceeds RECT range")?,
        top: i32::try_from(top).context("crop top exceeds RECT range")?,
        right: i32::try_from(left + crop_width).context("crop right exceeds RECT range")?,
        bottom: i32::try_from(top + crop_height).context("crop bottom exceeds RECT range")?,
    })
}

fn input_cache_insertion(current_len: usize, next_replacement: usize) -> (usize, usize, bool) {
    let capacity = NATIVE_WGC_FRAME_POOL_CAPACITY as usize;
    debug_assert!(capacity > 0);
    debug_assert!(current_len <= capacity);
    debug_assert!(next_replacement < capacity);
    if current_len < capacity {
        return (current_len, next_replacement, false);
    }
    (next_replacement, (next_replacement + 1) % capacity, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_descriptor_is_exact_gpu_only_four_slot_contract() {
        let descriptor = output_texture_desc(1920, 1080);
        assert_eq!(descriptor.Width, 1920);
        assert_eq!(descriptor.Height, 1080);
        assert_eq!(descriptor.MipLevels, 1);
        assert_eq!(descriptor.ArraySize, 1);
        assert_eq!(descriptor.Format, DXGI_FORMAT_NV12);
        assert_eq!(descriptor.Usage, D3D11_USAGE_DEFAULT);
        assert_eq!(
            descriptor.BindFlags,
            (D3D11_BIND_RENDER_TARGET | D3D11_BIND_VIDEO_ENCODER).0 as u32
        );
        assert_eq!(descriptor.CPUAccessFlags, 0);
        assert_eq!(descriptor.MiscFlags, 0);
        assert_eq!(NATIVE_ENCODER_SLOT_COUNT, 4);
    }

    #[test]
    fn centered_crop_preserves_target_aspect_with_even_chroma_edges() {
        let wide = center_crop_rect(2560, 1080, 1920, 1080).unwrap();
        assert_eq!(
            (wide.left, wide.top, wide.right, wide.bottom),
            (320, 0, 2240, 1080)
        );

        let tall = center_crop_rect(1100, 720, 1920, 1080).unwrap();
        assert_eq!(tall.left % 2, 0);
        assert_eq!(tall.top % 2, 0);
        assert_eq!((tall.right - tall.left) % 2, 0);
        assert_eq!((tall.bottom - tall.top) % 2, 0);
        assert!(tall.right <= 1100 && tall.bottom <= 720);
    }

    #[test]
    fn invalid_output_contract_fails_closed() {
        assert!(validate_dimensions(1920, 1080, 1919, 1080, 60).is_err());
        assert!(validate_dimensions(1920, 1080, 1920, 1079, 60).is_err());
        assert!(validate_dimensions(1920, 1080, 1920, 1080, 0).is_err());
    }

    #[test]
    fn fixed_ring_never_acquires_a_fifth_slot() {
        let states = NativeNv12SlotStates::new();
        for expected in 0..NATIVE_ENCODER_SLOT_COUNT {
            assert_eq!(states.try_acquire_converted(), Some(expected));
        }
        assert_eq!(states.try_acquire_converted(), None);
        states.release_converted(2);
        assert_eq!(states.try_acquire_converted(), Some(2));
    }

    #[test]
    fn submitted_slot_is_released_only_by_completion() {
        let states = NativeNv12SlotStates::new();
        assert_eq!(states.try_acquire_converted(), Some(0));
        states.mark_submitted(0).unwrap();
        assert_eq!(states.free_count(), NATIVE_ENCODER_SLOT_COUNT - 1);
        assert!(states.mark_submitted(0).is_err());
        states.complete_submitted(0).unwrap();
        assert_eq!(states.free_count(), NATIVE_ENCODER_SLOT_COUNT);
    }

    #[test]
    fn encoder_ring_identity_rejects_a_distinct_four_slot_owner() {
        let registered = Arc::new(NativeNv12SlotStates::new());
        let same = Arc::clone(&registered);
        let different = Arc::new(NativeNv12SlotStates::new());
        assert!(same_ring(&registered, &same));
        assert!(!same_ring(&registered, &different));
    }

    #[test]
    fn input_view_cache_replaces_in_round_robin_without_exceeding_pool_capacity() {
        assert_eq!(input_cache_insertion(0, 0), (0, 0, false));
        assert_eq!(input_cache_insertion(1, 0), (1, 0, false));
        assert_eq!(input_cache_insertion(2, 0), (0, 1, true));
        assert_eq!(input_cache_insertion(2, 1), (1, 0, true));
    }
}

