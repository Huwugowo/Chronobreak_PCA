//! Exercise the converter itself with synchronous, CPU-only D3D call recording.
//! No capture window, GPU, NVENC driver, sleeps, or completion thread is needed.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_COLOR_SPACE_TYPE, DXGI_FORMAT};
use windows::core::{BOOL, GUID, HRESULT, IUnknown};

use super::*;

#[derive(Default)]
struct Calls {
    copies: Cell<u64>,
    blits: RefCell<Vec<u32>>,
}

// Each object has a complete, owned vtable. Only the entries used below are
// callable; an unexpected D3D call aborts instead of dispatching through null or
// uninitialized memory. The installed callbacks have the SDK's exact ABI types.
#[repr(C)]
struct TestCom {
    vtable: *const c_void,
    references: Cell<u32>,
    _table: Box<dyn Any>,
    calls: Rc<Calls>,
    interfaces: Vec<GUID>,
}

unsafe extern "system" fn unexpected_call() -> ! {
    std::process::abort()
}

unsafe extern "system" fn query_interface(
    this: *mut c_void,
    iid: *const GUID,
    output: *mut *mut c_void,
) -> HRESULT {
    // SAFETY: all test interfaces originate from TestCom; COM supplies live
    // IID/output pointers and retains this reference for the duration of QI.
    unsafe {
        let object = &*this.cast::<TestCom>();
        if *iid == IUnknown::IID || object.interfaces.contains(&*iid) {
            add_ref(this);
            output.write(this);
            HRESULT(0)
        } else {
            output.write(std::ptr::null_mut());
            HRESULT(0x8000_4002_u32 as i32)
        }
    }
}

unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
    // SAFETY: TestCom is reference counted and confined to the current test.
    let count = unsafe { &(*this.cast::<TestCom>()).references };
    count.set(count.get() + 1);
    count.get()
}

unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    // SAFETY: every clone/QI adds a reference and only the final release takes
    // back the allocation, including its vtable and recorded-call owner.
    unsafe {
        let count = &(*this.cast::<TestCom>()).references;
        let remaining = count.get() - 1;
        count.set(remaining);
        if remaining == 0 {
            drop(Box::from_raw(this.cast::<TestCom>()));
        }
        remaining
    }
}

fn interface<I: Interface>(table: I::Vtable, calls: &Rc<Calls>) -> I
where
    I::Vtable: 'static,
{
    let table = Box::new(table);
    let object = Box::new(TestCom {
        vtable: (&*table as *const I::Vtable).cast(),
        references: Cell::new(1),
        _table: table,
        calls: Rc::clone(calls),
        interfaces: if I::IID == ID3D11VideoProcessorEnumerator1::IID {
            vec![I::IID, ID3D11VideoProcessorEnumerator::IID]
        } else {
            vec![I::IID]
        },
    });
    // SAFETY: the first field points at the complete vtable for I; the owned
    // allocation survives until the last COM Release and starts with one ref.
    unsafe { I::from_raw(Box::into_raw(object).cast()) }
}

macro_rules! table {
    ($kind:ty) => {{
        // SAFETY: these repr(C) SDK vtables consist solely of function pointers
        // (or pointer-sized reserved fields). All are initialized to a valid
        // code address; every invoked entry is replaced with a correctly typed
        // callback before this table is exposed through an interface.
        let mut table: $kind = unsafe {
            std::mem::transmute::<
                [unsafe extern "system" fn() -> !; size_of::<$kind>() / size_of::<usize>()],
                $kind,
            >([unexpected_call; size_of::<$kind>() / size_of::<usize>()])
        };
        // Every D3D vtable starts with the same three IUnknown entries.
        // SAFETY: repr(C) COM inheritance places IUnknown at offset zero.
        unsafe {
            let unknown = (&mut table as *mut $kind).cast::<windows::core::IUnknown_Vtbl>();
            (*unknown).QueryInterface = query_interface;
            (*unknown).AddRef = add_ref;
            (*unknown).Release = release;
        }
        table
    }};
}

fn texture(calls: &Rc<Calls>) -> ID3D11Texture2D {
    interface(table!(ID3D11Texture2D_Vtbl), calls)
}

type Raw = *mut c_void;

macro_rules! callback {
    (($($arg:ident: $kind:ty),*) -> $result:ty $body:block) => {{
        unsafe extern "system" fn callback($($arg: $kind),*) -> $result $body
        callback
    }};
}

fn video_device(calls: &Rc<Calls>) -> ID3D11VideoDevice {
    let mut table = table!(ID3D11VideoDevice_Vtbl);
    table.CreateVideoProcessorEnumerator = callback!((this: Raw, _desc: *const D3D11_VIDEO_PROCESSOR_CONTENT_DESC, output: *mut Raw) -> HRESULT {
        // SAFETY: TestCom owns the call recorder throughout the callback.
        let calls = unsafe { &(*this.cast::<TestCom>()).calls };
        let mut table = table!(ID3D11VideoProcessorEnumerator1_Vtbl);
        table.base__.CheckVideoProcessorFormat = callback!((_this: Raw, _format: DXGI_FORMAT, flags: *mut u32) -> HRESULT {
            // SAFETY: D3D supplies writable support flags.
            unsafe { *flags = (D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT.0
                | D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0) as u32 };
            HRESULT(0)
        });
        table.base__.GetVideoProcessorCaps = callback!((_this: Raw, caps: *mut D3D11_VIDEO_PROCESSOR_CAPS) -> HRESULT {
            // SAFETY: D3D supplies writable capabilities.
            unsafe {
                (*caps).MaxInputStreams = 1;
                (*caps).RateConversionCapsCount = 1;
            }
            HRESULT(0)
        });
        table.CheckVideoProcessorFormatConversion = callback!((_this: Raw, _input: DXGI_FORMAT, _input_color: DXGI_COLOR_SPACE_TYPE, _output: DXGI_FORMAT, _output_color: DXGI_COLOR_SPACE_TYPE, supported: *mut BOOL) -> HRESULT {
            // SAFETY: D3D supplies writable conversion support.
            unsafe { *supported = true.into() };
            HRESULT(0)
        });
        let enumerator = interface::<ID3D11VideoProcessorEnumerator1>(table, calls);
        // SAFETY: writable output receives an owned ref with the base prefix.
        unsafe { *output = enumerator.into_raw() };
        HRESULT(0)
    });
    table.CreateVideoProcessor = callback!((this: Raw, _enumerator: Raw, _index: u32, output: *mut Raw) -> HRESULT {
        // SAFETY: TestCom lives throughout the callback.
        let calls = unsafe { &(*this.cast::<TestCom>()).calls };
        let processor = interface::<ID3D11VideoProcessor>(table!(ID3D11VideoProcessor_Vtbl), calls);
        // SAFETY: writable output receives one owned reference.
        unsafe { *output = processor.into_raw() };
        HRESULT(0)
    });
    table.CreateVideoProcessorInputView = callback!((this: Raw, _texture: Raw, _enumerator: Raw, _desc: *const D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC, output: *mut Raw) -> HRESULT {
        // SAFETY: TestCom lives throughout the callback.
        let calls = unsafe { &(*this.cast::<TestCom>()).calls };
        let view = interface::<ID3D11VideoProcessorInputView>(table!(ID3D11VideoProcessorInputView_Vtbl), calls);
        // SAFETY: writable output receives one owned reference.
        unsafe { *output = view.into_raw() };
        HRESULT(0)
    });
    table.CreateVideoProcessorOutputView = callback!((this: Raw, _texture: Raw, _enumerator: Raw, _desc: *const D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC, output: *mut Raw) -> HRESULT {
        // SAFETY: TestCom lives throughout the callback.
        let calls = unsafe { &(*this.cast::<TestCom>()).calls };
        let view = interface::<ID3D11VideoProcessorOutputView>(table!(ID3D11VideoProcessorOutputView_Vtbl), calls);
        // SAFETY: writable output receives one owned reference.
        unsafe { *output = view.into_raw() };
        HRESULT(0)
    });
    interface(table, calls)
}

fn converter(calls: &Rc<Calls>) -> NativeNv12Converter {
    let mut device = table!(ID3D11Device_Vtbl);
    device.CreateTexture2D = callback!((this: Raw, _desc: *const D3D11_TEXTURE2D_DESC, _data: *const D3D11_SUBRESOURCE_DATA, output: *mut Raw) -> HRESULT {
        // SAFETY: the callback retains TestCom and writes one owned COM ref.
        unsafe { *output = texture(&(*this.cast::<TestCom>()).calls).into_raw() };
        HRESULT(0)
    });
    let d3d_device = interface(device, calls);
    let mut context = table!(ID3D11DeviceContext_Vtbl);
    context.CopySubresourceRegion = callback!((this: Raw, _dest: Raw, _dest_sub: u32, _x: u32, _y: u32, _z: u32, _source: Raw, _source_sub: u32, _region: *const D3D11_BOX) -> () {
        // SAFETY: this points at a live TestCom throughout the synchronous call.
        let calls = unsafe { &(*this.cast::<TestCom>()).calls };
        calls.copies.set(calls.copies.get() + 1);
    });
    let mut video = table!(ID3D11VideoContext1_Vtbl);
    video.base__.VideoProcessorBlt = callback!((this: Raw, _processor: Raw, _output: Raw, index: u32, _count: u32, _streams: *const D3D11_VIDEO_PROCESSOR_STREAM) -> HRESULT {
        // SAFETY: same synchronous TestCom contract; record real blit indices.
        unsafe { &(*this.cast::<TestCom>()).calls }
            .blits
            .borrow_mut()
            .push(index);
        HRESULT(0)
    });
    video.VideoProcessorSetStreamColorSpace1 = callback!((_this: Raw, _processor: Raw, _stream: u32, _space: DXGI_COLOR_SPACE_TYPE) -> () {});
    video.VideoProcessorSetOutputColorSpace1 =
        callback!((_this: Raw, _processor: Raw, _space: DXGI_COLOR_SPACE_TYPE) -> () {});
    video.base__.VideoProcessorSetStreamFrameFormat = callback!((_this: Raw, _processor: Raw, _stream: u32, _format: D3D11_VIDEO_FRAME_FORMAT) -> () {});
    video.base__.VideoProcessorSetStreamAutoProcessingMode =
        callback!((_this: Raw, _processor: Raw, _stream: u32, _enabled: BOOL) -> () {});
    video.base__.VideoProcessorSetStreamSourceRect = callback!((_this: Raw, _processor: Raw, _stream: u32, _enabled: BOOL, _rect: *const RECT) -> () {});
    video.base__.VideoProcessorSetStreamDestRect = callback!((_this: Raw, _processor: Raw, _stream: u32, _enabled: BOOL, _rect: *const RECT) -> () {});
    video.base__.VideoProcessorSetOutputTargetRect =
        callback!((_this: Raw, _processor: Raw, _enabled: BOOL, _rect: *const RECT) -> () {});
    let video_device = video_device(calls);
    let (enumerator, processor) =
        create_processor(&video_device, 1920, 1080, 1920, 1080, 60).unwrap();
    let slots = create_slots(&d3d_device, &video_device, &enumerator, 1920, 1080).unwrap();
    let latest_source =
        create_source_snapshot(&d3d_device, &video_device, &enumerator, 1920, 1080).unwrap();
    NativeNv12Converter {
        d3d_device,
        immediate_context: interface(context, calls),
        video_device,
        video_context: interface(video, calls),
        enumerator,
        processor,
        slots,
        slot_states: Arc::new(NativeNv12SlotStates::new()),
        input_views: Vec::new(),
        next_input_view_replacement: 0,
        latest_source,
        input_width: 1920,
        input_height: 1080,
        output_width: 1920,
        output_height: 1080,
        fps: 60,
        output_frame_index: 0,
        telemetry: NativeNv12Telemetry::default(),
        _thread_affinity: PhantomData,
    }
}

fn stage(converter: &mut NativeNv12Converter, calls: &Rc<Calls>) {
    let dimensions = (converter.input_width, converter.input_height);
    let source = NativeSourceTexture::test_texture(texture(calls), dimensions.0, dimensions.1);
    assert!(converter.stage_source_texture(&source, dimensions).unwrap());
}

fn submit_ring(converter: &mut NativeNv12Converter) {
    for index in 0..NATIVE_ENCODER_SLOT_COUNT {
        assert_eq!(
            converter
                .convert_staged(100 + index as i64)
                .unwrap()
                .unwrap()
                .mark_submitted()
                .unwrap(),
            (index, 100 + index as i64)
        );
    }
}

#[test]
fn completed_slot_reuses_nv12_but_still_advances_each_tick() {
    let calls = Rc::new(Calls::default());
    let mut converter = converter(&calls);
    stage(&mut converter, &calls);
    submit_ring(&mut converter);
    assert!(converter.convert_staged(200).unwrap().is_none());
    assert_eq!(converter.output_frame_index, 4);
    converter.slot_states.complete_submitted(2).unwrap();
    assert_eq!(
        converter
            .convert_staged(200)
            .unwrap()
            .unwrap()
            .mark_submitted()
            .unwrap(),
        (2, 200)
    );
    assert_eq!(converter.output_frame_index, 5);
    assert_eq!(converter.telemetry().converted_frames, 4);
    assert_eq!(*calls.blits.borrow(), [0, 1, 2, 3]);
    assert!(converter.convert_staged(300).unwrap().is_none());
    assert_eq!(converter.output_frame_index, 5);
    converter.slot_states.complete_submitted(2).unwrap();
    let frame = converter.convert_staged(300).unwrap().unwrap();
    assert_eq!((frame.slot_index(), frame.qpc_100ns()), (2, 300));
    drop(frame);
    assert_eq!(converter.output_frame_index, 6);
    assert_eq!(calls.blits.borrow().len(), 4);
}

#[test]
fn new_snapshot_invalidates_every_slot_including_submitted_slots() {
    let calls = Rc::new(Calls::default());
    let mut converter = converter(&calls);
    stage(&mut converter, &calls);
    submit_ring(&mut converter);
    assert!(
        converter
            .slots
            .iter()
            .all(|slot| slot.holds_latest_snapshot)
    );
    stage(&mut converter, &calls);
    assert_eq!(calls.copies.get(), 2);
    assert!(
        converter
            .slots
            .iter()
            .all(|slot| !slot.holds_latest_snapshot)
    );
    for index in (0..NATIVE_ENCODER_SLOT_COUNT).rev() {
        converter.slot_states.complete_submitted(index).unwrap();
        let frame = converter.convert_staged(200).unwrap().unwrap();
        assert_eq!(frame.slot_index(), index);
        frame.mark_submitted().unwrap();
    }
    assert_eq!(converter.telemetry().converted_frames, 8);
    assert_eq!(*calls.blits.borrow(), [0, 1, 2, 3, 4, 5, 6, 7]);
    converter.slot_states.complete_submitted(0).unwrap();
    converter.convert_staged(300).unwrap().unwrap().release();
    assert_eq!(converter.telemetry().converted_frames, 8);
}

#[test]
fn resize_invalidates_cached_nv12_and_requires_a_fresh_snapshot() {
    let calls = Rc::new(Calls::default());
    let mut converter = converter(&calls);
    stage(&mut converter, &calls);
    submit_ring(&mut converter);
    assert!(converter.reconfigure_input(1600, 900).is_err());
    assert!(
        converter
            .slots
            .iter()
            .all(|slot| slot.holds_latest_snapshot)
    );
    for index in 0..NATIVE_ENCODER_SLOT_COUNT {
        converter.slot_states.complete_submitted(index).unwrap();
    }
    let textures = converter.slots.each_ref().map(|slot| slot.texture.as_raw());
    converter.reconfigure_input(1920, 1080).unwrap();
    assert!(
        converter
            .slots
            .iter()
            .all(|slot| slot.holds_latest_snapshot)
    );
    converter.reconfigure_input(1600, 900).unwrap();
    assert_eq!(
        converter.slots.each_ref().map(|slot| slot.texture.as_raw()),
        textures
    );
    assert!(
        converter
            .slots
            .iter()
            .all(|slot| !slot.holds_latest_snapshot)
    );
    assert!(converter.convert_staged(200).is_err());
    assert_eq!(converter.free_slot_count(), NATIVE_ENCODER_SLOT_COUNT);
    assert_eq!(converter.telemetry().processor_recreations, 1);
    assert_eq!(converter.telemetry().output_view_recreations, 4);
    stage(&mut converter, &calls);
    submit_ring(&mut converter);
    assert_eq!(converter.telemetry().converted_frames, 8);
    converter.slot_states.complete_submitted(0).unwrap();
    converter.convert_staged(300).unwrap().unwrap().release();
    assert_eq!(converter.telemetry().converted_frames, 8);
    assert_eq!(converter.dimensions(), (1920, 1080));
}
