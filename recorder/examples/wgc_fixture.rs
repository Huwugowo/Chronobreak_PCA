#[cfg(target_os = "windows")]
mod windows_fixture {
    use std::fs;
    use std::io::{self, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::ptr::NonNull;
    use std::sync::{Arc, OnceLock};
    use std::thread;
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result, bail};
    use windows::Win32::Foundation::{COLORREF, HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleDC, CreateDIBSection,
        CreateSolidBrush, DIB_RGB_COLORS, DeleteDC, DeleteObject, ExcludeClipRect, FillRect,
        GdiFlush, GetDC, HBITMAP, HDC, HGDIOBJ, ReleaseDC, RestoreDC, SRCCOPY, SaveDC,
        SelectObject, SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
    };
    use windows::Win32::System::Power::{
        ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
    use winit::application::ApplicationHandler;
    use winit::dpi::{PhysicalPosition, PhysicalSize};
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::{Window, WindowAttributes, WindowId, WindowLevel};

    const TITLE: &str = "QueueBack WGC Changing-Window Fixture";
    const FRAME_INTERVAL: Duration = Duration::from_nanos(8_333_333);
    const INITIAL_WIDTH: u32 = 1920;
    const INITIAL_HEIGHT: u32 = 1080;
    const MARKER_RATE: u64 = 60;
    const MARKER_SAMPLE_RATE: u64 = 48_000;
    const MARKER_SAMPLES_PER_FRAME: u64 = MARKER_SAMPLE_RATE / MARKER_RATE;
    const MARKER_IMPULSE_SAMPLES: u64 = 96;
    const MARKER_IMPULSE_AMPLITUDE: i32 = 30_000;
    const MARKER_CROP_LEFT: i32 = 320;
    const MARKER_CROP_TOP: i32 = 60;
    const MARKER_SCALE: usize = 4;
    const MARKER_CELL_ORIGIN: usize = 16;
    const MARKER_CELL_SIZE: usize = 16;
    const MARKER_WIDTH: usize = 320;
    const MARKER_HEIGHT: usize = 240;
    const MARKER_SURFACE_WIDTH: usize = MARKER_WIDTH * MARKER_SCALE;
    const MARKER_SURFACE_HEIGHT: usize = MARKER_HEIGHT * MARKER_SCALE;
    const MARKER_SURFACE_PIXELS: usize = MARKER_SURFACE_WIDTH * MARKER_SURFACE_HEIGHT;
    const MARKER_COLUMNS: u32 = 12;
    const MARKER_BITS: u32 = 96;
    const MARKER_MILLISECONDS_PER_SECOND: u64 = 1_000;
    const MARKER_FLASH_HALF_WIDTH_MS: u64 = 25;
    const MARKER_TIMESTAMP_MASK: u32 = 0x00FF_FFFF;
    const MARKER_MAGIC: u16 = 0xC3A5;
    const MARKER_STATE_PRE_EPOCH: u8 = 0x5A;
    const MARKER_STATE_LIVE: u8 = 0xA5;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Scenario {
        Steady,
        Resize,
        MinimizeRestore,
        Occlusion,
        CloseWindow,
    }

    struct Arguments {
        duration: Option<Duration>,
        scenario: Scenario,
        action_after: Duration,
        restore_after: Duration,
        always_on_top: bool,
        marker_duration: Option<Duration>,
        marker_start_signal: Option<PathBuf>,
    }

    pub fn run() -> Result<()> {
        let arguments = parse_arguments()?;
        let _execution_state = ExecutionStateGuard::new(arguments.always_on_top)?;
        let marker_source = match (
            arguments.marker_duration,
            arguments.marker_start_signal.clone(),
        ) {
            (Some(duration), Some(start_signal)) => {
                Some(MarkerSource::start(duration, start_signal)?)
            }
            (None, None) => None,
            _ => bail!("marker duration and start signal must be provided together"),
        };
        if let Some(source) = &marker_source {
            println!("QUEUEBACK_WGC_MARKER_AUDIO endpoint={}", source.endpoint);
            io::stdout().flush().ok();
        }
        let event_loop = EventLoop::new().context("could not create the WGC fixture event loop")?;
        event_loop.set_control_flow(ControlFlow::Wait);
        let mut fixture = Fixture::new(arguments, marker_source);
        event_loop
            .run_app(&mut fixture)
            .context("WGC fixture event loop failed")
    }

    struct ExecutionStateGuard {
        active: bool,
    }

    impl ExecutionStateGuard {
        fn new(active: bool) -> Result<Self> {
            if active
                && unsafe {
                    SetThreadExecutionState(
                        ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED,
                    )
                }
                .0 == 0
            {
                bail!("Windows refused the fixture display-required execution state");
            }
            Ok(Self { active })
        }
    }

    impl Drop for ExecutionStateGuard {
        fn drop(&mut self) {
            if self.active {
                unsafe {
                    SetThreadExecutionState(ES_CONTINUOUS);
                }
            }
        }
    }

    fn parse_arguments() -> Result<Arguments> {
        let mut values = std::env::args().skip(1);
        let mut duration = None;
        let mut scenario = Scenario::Steady;
        let mut action_after = Duration::from_secs(3);
        let mut restore_after = Duration::from_secs(6);
        let mut always_on_top = false;
        let mut marker_duration = None;
        let mut marker_start_signal = None;
        while let Some(argument) = values.next() {
            match argument.as_str() {
                "--duration-seconds" => {
                    duration = Some(Duration::from_secs(
                        values
                            .next()
                            .context("--duration-seconds requires a value")?
                            .parse::<u64>()
                            .context("--duration-seconds must be an integer")?,
                    ));
                }
                "--scenario" => {
                    scenario = match values
                        .next()
                        .context("--scenario requires a value")?
                        .as_str()
                    {
                        "steady" => Scenario::Steady,
                        "resize" => Scenario::Resize,
                        "minimize_restore" => Scenario::MinimizeRestore,
                        "occlusion" => Scenario::Occlusion,
                        "close_window" => Scenario::CloseWindow,
                        value => bail!("unsupported fixture scenario {value:?}"),
                    };
                }
                "--action-after-seconds" => {
                    action_after = Duration::from_secs(
                        values
                            .next()
                            .context("--action-after-seconds requires a value")?
                            .parse::<u64>()
                            .context("--action-after-seconds must be an integer")?,
                    );
                }
                "--restore-after-seconds" => {
                    restore_after = Duration::from_secs(
                        values
                            .next()
                            .context("--restore-after-seconds requires a value")?
                            .parse::<u64>()
                            .context("--restore-after-seconds must be an integer")?,
                    );
                }
                "--always-on-top" => always_on_top = true,
                "--marker-duration-seconds" => {
                    let seconds = values
                        .next()
                        .context("--marker-duration-seconds requires a value")?
                        .parse::<u64>()
                        .context("--marker-duration-seconds must be an integer")?;
                    if !(2..=7200).contains(&seconds) {
                        bail!("--marker-duration-seconds must be between 2 and 7200");
                    }
                    marker_duration = Some(Duration::from_secs(seconds));
                }
                "--marker-start-signal" => {
                    marker_start_signal = Some(PathBuf::from(
                        values
                            .next()
                            .context("--marker-start-signal requires a path")?,
                    ));
                }
                value => bail!("unknown argument {value:?}"),
            }
        }
        if restore_after <= action_after {
            bail!("--restore-after-seconds must be later than --action-after-seconds");
        }
        if marker_duration.is_some() != marker_start_signal.is_some() {
            bail!("marker duration and start signal must be provided together");
        }
        Ok(Arguments {
            duration,
            scenario,
            action_after,
            restore_after,
            always_on_top,
            marker_duration,
            marker_start_signal,
        })
    }

    struct MarkerSource {
        endpoint: String,
        epoch: Arc<OnceLock<Instant>>,
        duration_frames: u64,
        generation: u16,
    }

    impl MarkerSource {
        fn start(duration: Duration, start_signal: PathBuf) -> Result<Self> {
            match fs::symlink_metadata(&start_signal) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Ok(_) => bail!("marker start signal already exists"),
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "could not inspect marker start signal {}",
                            start_signal.display()
                        )
                    });
                }
            }
            let listener = TcpListener::bind(("127.0.0.1", 0))
                .context("could not bind replay-time marker audio listener")?;
            let endpoint = format!(
                "tcp://127.0.0.1:{}",
                listener
                    .local_addr()
                    .context("could not inspect marker audio listener")?
                    .port()
            );
            let epoch = Arc::new(OnceLock::new());
            let writer_epoch = Arc::clone(&epoch);
            let duration_frames = duration
                .as_secs()
                .checked_mul(MARKER_RATE)
                .context("marker duration frame count overflowed")?;
            let generation = u16::try_from(std::process::id() & u32::from(u16::MAX))
                .expect("masked fixture process id must fit u16");
            let markers = marker_frames(duration_frames);
            println!(
                "QUEUEBACK_WGC_MARKER_CONTRACT generation={generation} duration_frames={duration_frames} sample_rate={MARKER_SAMPLE_RATE} impulse_samples={MARKER_IMPULSE_SAMPLES} marker_frames={},{},{} marker_samples={},{},{}",
                markers[0],
                markers[1],
                markers[2],
                markers[0].saturating_mul(MARKER_SAMPLES_PER_FRAME),
                markers[1].saturating_mul(MARKER_SAMPLES_PER_FRAME),
                markers[2].saturating_mul(MARKER_SAMPLES_PER_FRAME),
            );
            io::stdout().flush().ok();
            thread::Builder::new()
                .name("replay-time-marker-audio".to_owned())
                .spawn(move || {
                    if let Err(error) = stream_marker_audio(
                        listener,
                        writer_epoch,
                        duration_frames,
                        generation,
                        start_signal,
                    ) {
                        eprintln!("QUEUEBACK_WGC_MARKER_AUDIO_ERROR {error:#}");
                    }
                })
                .context("could not start replay-time marker audio writer")?;
            Ok(Self {
                endpoint,
                epoch,
                duration_frames,
                generation,
            })
        }
    }

    fn stream_marker_audio(
        listener: TcpListener,
        epoch: Arc<OnceLock<Instant>>,
        duration_frames: u64,
        generation: u16,
        start_signal: PathBuf,
    ) -> Result<()> {
        let (mut stream, peer) = listener
            .accept()
            .context("could not accept replay-time marker audio consumer")?;
        if !peer.ip().is_loopback() {
            bail!("replay-time marker audio consumer is not loopback");
        }
        stream
            .set_nodelay(true)
            .context("could not configure replay-time marker audio socket")?;
        println!("QUEUEBACK_WGC_MARKER_AUDIO_ACCEPTED generation={generation}");
        io::stdout().flush().ok();
        wait_for_marker_start_signal(&start_signal)?;
        let started = Instant::now();
        epoch
            .set(started)
            .map_err(|_| anyhow::anyhow!("replay-time marker epoch was already set"))?;
        println!("QUEUEBACK_WGC_MARKER_AUDIO_CONNECTED generation={generation}");
        io::stdout().flush().ok();

        let marker_frames = marker_frames(duration_frames);
        let writer_frames = duration_frames.saturating_add(MARKER_RATE * 60);
        let mut payload = [0_u8; (MARKER_SAMPLES_PER_FRAME as usize) * 4];
        for audio_frame in 0..writer_frames {
            let target = started
                + Duration::from_nanos(audio_frame.saturating_mul(1_000_000_000) / MARKER_RATE);
            if let Some(delay) = target.checked_duration_since(Instant::now()) {
                thread::sleep(delay);
            }
            let first_sample = audio_frame.saturating_mul(MARKER_SAMPLES_PER_FRAME);
            for sample_offset in 0..MARKER_SAMPLES_PER_FRAME {
                let value = marker_impulse_sample(
                    first_sample.saturating_add(sample_offset),
                    &marker_frames,
                );
                let bytes = value.to_le_bytes();
                let offset = sample_offset as usize * 4;
                payload[offset..offset + 2].copy_from_slice(&bytes);
                payload[offset + 2..offset + 4].copy_from_slice(&bytes);
            }
            if let Err(error) = stream.write_all(&payload) {
                println!("QUEUEBACK_WGC_MARKER_AUDIO_DISCONNECTED error={error}");
                io::stdout().flush().ok();
                return Ok(());
            }
        }
        println!("QUEUEBACK_WGC_MARKER_AUDIO_COMPLETE");
        io::stdout().flush().ok();
        Ok(())
    }

    fn wait_for_marker_start_signal(path: &Path) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            match fs::symlink_metadata(path) {
                Ok(metadata) => {
                    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                        bail!("marker start signal must be a regular non-symlink file");
                    }
                    let payload = fs::read(path).with_context(|| {
                        format!("could not read marker start signal {}", path.display())
                    })?;
                    if payload != b"start\n" {
                        bail!("marker start signal has unexpected content");
                    }
                    println!(
                        "QUEUEBACK_WGC_MARKER_START_SIGNAL_ACCEPTED path={}",
                        path.display()
                    );
                    io::stdout().flush().ok();
                    return Ok(());
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("could not inspect marker start signal {}", path.display())
                    });
                }
            }
            if Instant::now() >= deadline {
                bail!("marker start signal exceeded its 60-second deadline");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn marker_frames(duration_frames: u64) -> [u64; 3] {
        [
            duration_frames / 6,
            duration_frames / 2,
            duration_frames.saturating_mul(5) / 6,
        ]
    }

    fn marker_impulse_sample(sample: u64, marker_frames: &[u64; 3]) -> i16 {
        for marker_frame in marker_frames {
            let marker_sample = marker_frame.saturating_mul(MARKER_SAMPLES_PER_FRAME);
            let offset = sample.saturating_sub(marker_sample);
            if sample >= marker_sample && offset < MARKER_IMPULSE_SAMPLES {
                let sign = if (offset / 8) % 2 == 0 { 1 } else { -1 };
                let envelope = MARKER_IMPULSE_SAMPLES - offset;
                let magnitude =
                    MARKER_IMPULSE_AMPLITUDE * envelope as i32 / MARKER_IMPULSE_SAMPLES as i32;
                return (sign * magnitude) as i16;
            }
        }
        0
    }

    struct MarkerDib {
        device: HDC,
        bitmap: HBITMAP,
        previous: HGDIOBJ,
        bits: NonNull<u32>,
    }

    impl MarkerDib {
        fn new(reference_device: HDC) -> Result<Self> {
            let surface_width =
                i32::try_from(MARKER_SURFACE_WIDTH).expect("marker width must fit i32");
            let surface_height =
                i32::try_from(MARKER_SURFACE_HEIGHT).expect("marker height must fit i32");
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>())
                        .expect("bitmap header size must fit u32"),
                    biWidth: surface_width,
                    biHeight: -surface_height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    biSizeImage: u32::try_from(MARKER_SURFACE_PIXELS * std::mem::size_of::<u32>())
                        .expect("marker bitmap size must fit u32"),
                    ..BITMAPINFOHEADER::default()
                },
                ..BITMAPINFO::default()
            };
            // SAFETY: `reference_device` is a live window DC owned by the
            // caller for this initialization call.
            let device = unsafe { CreateCompatibleDC(Some(reference_device)) };
            if device.0.is_null() {
                bail!("could not create marker memory DC");
            }
            let mut raw_bits = std::ptr::null_mut();
            // SAFETY: `info` describes a bounded 32-bit top-down DIB and
            // `raw_bits` remains valid for the out-pointer write.
            let bitmap = match unsafe {
                CreateDIBSection(
                    Some(reference_device),
                    &raw const info,
                    DIB_RGB_COLORS,
                    &raw mut raw_bits,
                    None,
                    0,
                )
            } {
                Ok(bitmap) => bitmap,
                Err(error) => {
                    // SAFETY: `device` was created above and no object was selected.
                    unsafe {
                        let _ = DeleteDC(device);
                    }
                    return Err(error).context("could not create marker DIB section");
                }
            };
            let Some(bits) = NonNull::new(raw_bits.cast::<u32>()) else {
                // SAFETY: both handles were created above and the bitmap has
                // not been selected into the memory DC.
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(bitmap.0));
                    let _ = DeleteDC(device);
                }
                bail!("marker DIB section returned a null pixel pointer");
            };
            // SAFETY: both handles are live and the bitmap is compatible with
            // the memory DC created from `reference_device`.
            let previous = unsafe { SelectObject(device, HGDIOBJ(bitmap.0)) };
            if previous.is_invalid() {
                // SAFETY: selection failed, so the bitmap can be deleted
                // before the memory DC.
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(bitmap.0));
                    let _ = DeleteDC(device);
                }
                bail!("could not select marker DIB into memory DC");
            }
            Ok(Self {
                device,
                bitmap,
                previous,
                bits,
            })
        }

        fn pixels_mut(&mut self) -> &mut [u32] {
            // SAFETY: `bits` points to the live DIB section owned by `self`;
            // its allocation is exactly `MARKER_SURFACE_PIXELS` u32 pixels
            // and this exclusive borrow prevents concurrent access.
            unsafe { std::slice::from_raw_parts_mut(self.bits.as_ptr(), MARKER_SURFACE_PIXELS) }
        }

        fn publish(&self, destination: HDC) -> Result<()> {
            let width = i32::try_from(MARKER_SURFACE_WIDTH).expect("marker width must fit i32");
            let height = i32::try_from(MARKER_SURFACE_HEIGHT).expect("marker height must fit i32");
            // SAFETY: both DCs are live, the source DC has the complete DIB
            // selected, and the fixed source/destination rectangles fit it.
            unsafe {
                BitBlt(
                    destination,
                    MARKER_CROP_LEFT,
                    MARKER_CROP_TOP,
                    width,
                    height,
                    Some(self.device),
                    0,
                    0,
                    SRCCOPY,
                )
            }
            .context("could not atomically publish marker DIB")
        }
    }

    impl Drop for MarkerDib {
        fn drop(&mut self) {
            // SAFETY: these handles are exclusively owned by `self`; restoring
            // the previous object makes the DIB deletable before deleting the DC.
            unsafe {
                let _ = SelectObject(self.device, self.previous);
                let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
                let _ = DeleteDC(self.device);
            }
        }
    }

    struct MarkerOverlay {
        epoch: Arc<OnceLock<Instant>>,
        duration_frames: u64,
        generation: u16,
        surface: Option<MarkerDib>,
    }

    impl MarkerOverlay {
        fn new(source: MarkerSource) -> Self {
            Self {
                epoch: source.epoch,
                duration_frames: source.duration_frames,
                generation: source.generation,
                surface: None,
            }
        }

        fn publish(&mut self, device: HDC, width: i32, height: i32) -> Result<()> {
            let surface_width =
                i32::try_from(MARKER_SURFACE_WIDTH).expect("marker width must fit i32");
            let surface_height =
                i32::try_from(MARKER_SURFACE_HEIGHT).expect("marker height must fit i32");
            if width < MARKER_CROP_LEFT + surface_width || height < MARKER_CROP_TOP + surface_height
            {
                bail!("marker DIB does not fit the fixture client area");
            }
            let timestamp_ms = self.epoch.get().map(|started| {
                let elapsed = started.elapsed().as_millis();
                u32::try_from(elapsed.min(u128::from(MARKER_TIMESTAMP_MASK)))
                    .expect("bounded marker timestamp must fit u32")
            });
            let duration_ms = self
                .duration_frames
                .saturating_mul(MARKER_MILLISECONDS_PER_SECOND)
                / MARKER_RATE;
            if self.surface.is_none() {
                self.surface = Some(MarkerDib::new(device)?);
            }
            let surface = self
                .surface
                .as_mut()
                .expect("marker DIB was initialized above");
            render_marker_pixels(
                surface.pixels_mut(),
                self.generation,
                timestamp_ms,
                duration_ms,
            );
            surface.publish(device)
        }
    }

    struct Fixture {
        window: Option<Window>,
        occluder: Option<Window>,
        frame: u64,
        started_at: Instant,
        next_frame: Instant,
        next_report: Instant,
        deadline: Option<Instant>,
        target_reported: bool,
        scenario: Scenario,
        action_after: Duration,
        restore_after: Duration,
        first_action_done: bool,
        second_action_done: bool,
        always_on_top: bool,
        marker: Option<MarkerOverlay>,
    }

    impl Fixture {
        fn new(arguments: Arguments, marker_source: Option<MarkerSource>) -> Self {
            let now = Instant::now();
            let marker = marker_source.map(MarkerOverlay::new);
            Self {
                window: None,
                occluder: None,
                frame: 0,
                started_at: now,
                next_frame: now,
                next_report: now + Duration::from_secs(5),
                deadline: arguments.duration.map(|duration| now + duration),
                target_reported: false,
                scenario: arguments.scenario,
                action_after: arguments.action_after,
                restore_after: arguments.restore_after,
                first_action_done: false,
                second_action_done: false,
                always_on_top: arguments.always_on_top,
                marker,
            }
        }

        fn run_first_action(&mut self, event_loop: &ActiveEventLoop) {
            let Some(window) = self.window.as_ref() else {
                return;
            };
            match self.scenario {
                Scenario::Steady => {}
                Scenario::Resize => {
                    let _ = window.request_inner_size(PhysicalSize::new(1600, 900));
                    println!("QUEUEBACK_WGC_ACTION resize_1600x900");
                }
                Scenario::MinimizeRestore => {
                    window.set_minimized(true);
                    println!("QUEUEBACK_WGC_ACTION minimized");
                }
                Scenario::Occlusion => {
                    let base = window
                        .outer_position()
                        .unwrap_or(PhysicalPosition::new(0, 0));
                    let attributes = WindowAttributes::default()
                        .with_title("QueueBack WGC Fixture Occluder")
                        .with_inner_size(window.inner_size())
                        .with_position(base)
                        .with_window_level(WindowLevel::AlwaysOnTop);
                    let occluder = event_loop
                        .create_window(attributes)
                        .expect("could not create the dedicated occlusion window");
                    occluder.focus_window();
                    occluder.request_redraw();
                    self.occluder = Some(occluder);
                    println!("QUEUEBACK_WGC_ACTION occluder_shown_and_focused");
                }
                Scenario::CloseWindow => {
                    let window = self.window.take();
                    drop(window);
                    println!("QUEUEBACK_WGC_ACTION target_window_closed");
                }
            }
            io::stdout().flush().ok();
        }

        fn run_second_action(&mut self) {
            let Some(window) = self.window.as_ref() else {
                return;
            };
            match self.scenario {
                Scenario::Steady => {}
                Scenario::Resize => {
                    let _ = window.request_inner_size(PhysicalSize::new(1920, 1080));
                    println!("QUEUEBACK_WGC_ACTION resize_1920x1080");
                }
                Scenario::MinimizeRestore => {
                    window.set_minimized(false);
                    window.focus_window();
                    println!("QUEUEBACK_WGC_ACTION restored_and_focused");
                }
                Scenario::Occlusion => {
                    if let Some(occluder) = self.occluder.take() {
                        occluder.set_visible(false);
                    }
                    window.focus_window();
                    println!("QUEUEBACK_WGC_ACTION occluder_removed");
                }
                Scenario::CloseWindow => {}
            }
            io::stdout().flush().ok();
        }

        fn report_target_once(&mut self) {
            if self.target_reported {
                return;
            }
            let Some(window) = self.window.as_ref() else {
                return;
            };
            let expected_size = PhysicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT);
            if window.inner_size() != expected_size {
                let _ = window.request_inner_size(expected_size);
                return;
            }
            match league_replay_recorder::platform::capture_target_for_process(std::process::id()) {
                Ok(target) if target.dimensions() == Some((INITIAL_WIDTH, INITIAL_HEIGHT)) => {
                    println!(
                        "QUEUEBACK_WGC_TARGET hwnd={} adapter_index={} adapter_luid={:016x} width={} height={}",
                        target.windows_hwnd().unwrap_or_default(),
                        target.windows_adapter_index().unwrap_or_default(),
                        target.windows_adapter_luid().unwrap_or_default(),
                        target.dimensions().map(|value| value.0).unwrap_or_default(),
                        target.dimensions().map(|value| value.1).unwrap_or_default(),
                    );
                    io::stdout().flush().ok();
                    self.target_reported = true;
                }
                Ok(_) => {
                    let _ = window.request_inner_size(expected_size);
                }
                Err(error) => eprintln!("QUEUEBACK_WGC_TARGET_ERROR {error:#}"),
            }
        }

        fn draw_next_frame(&mut self) {
            if let Some(window) = &self.window {
                draw_frame(window, self.frame, self.marker.as_mut());
                self.frame = self.frame.saturating_add(1);
                window.request_redraw();
            }
            if let Some(window) = &self.occluder {
                draw_occluder(window);
                window.request_redraw();
            }
            self.report_target_once();
        }
    }

    impl ApplicationHandler for Fixture {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let mut attributes = WindowAttributes::default()
                .with_title(TITLE)
                .with_inner_size(PhysicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT))
                .with_position(PhysicalPosition::new(0, 0))
                .with_decorations(false)
                .with_resizable(true);
            if self.always_on_top {
                attributes = attributes.with_window_level(WindowLevel::AlwaysOnTop);
            }
            let window = event_loop
                .create_window(attributes)
                .expect("could not create the WGC fixture window");
            let hwnd = hwnd(&window).expect("fixture did not expose a Win32 HWND");
            println!(
                "QUEUEBACK_WGC_FIXTURE_READY pid={} hwnd={}",
                std::process::id(),
                hwnd.0 as usize
            );
            io::stdout().flush().ok();
            window.request_redraw();
            self.window = Some(window);
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            window_id: WindowId,
            event: WindowEvent,
        ) {
            match event {
                WindowEvent::CloseRequested => {
                    if self
                        .window
                        .as_ref()
                        .is_some_and(|window| window.id() == window_id)
                    {
                        event_loop.exit();
                    }
                }
                WindowEvent::RedrawRequested => {
                    if let Some(window) = &self.occluder
                        && window.id() == window_id
                    {
                        draw_occluder(window);
                    }
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            let now = Instant::now();
            if self.deadline.is_some_and(|deadline| now >= deadline) {
                event_loop.exit();
                return;
            }
            let elapsed = now.saturating_duration_since(self.started_at);
            if !self.first_action_done && elapsed >= self.action_after {
                self.run_first_action(event_loop);
                self.first_action_done = true;
            }
            if !self.second_action_done && elapsed >= self.restore_after {
                self.run_second_action();
                self.second_action_done = true;
            }
            if now >= self.next_frame {
                // RedrawRequested can be throttled for fully occluded windows.
                // Drive the dedicated GDI surface directly so the soak fixture
                // keeps changing even while the operator covers it.
                self.draw_next_frame();
                self.next_frame = now + FRAME_INTERVAL;
            }
            if now >= self.next_report {
                println!(
                    "QUEUEBACK_WGC_FIXTURE_PROGRESS elapsed_ms={} drawn_frames={}",
                    now.saturating_duration_since(self.started_at).as_millis(),
                    self.frame
                );
                io::stdout().flush().ok();
                self.next_report = now + Duration::from_secs(5);
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
        }
    }

    fn hwnd(window: &Window) -> Option<HWND> {
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return None;
        };
        Some(HWND(handle.hwnd.get() as *mut _))
    }

    fn draw_frame(window: &Window, frame: u64, marker: Option<&mut MarkerOverlay>) {
        let Some(hwnd) = hwnd(window) else {
            return;
        };
        let mut bounds = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut bounds) }.is_err() {
            return;
        }
        let device = unsafe { GetDC(Some(hwnd)) };
        if device.0.is_null() {
            return;
        }

        let width = (bounds.right - bounds.left).max(1);
        let height = (bounds.bottom - bounds.top).max(1);
        let clip_state = if marker.is_some() {
            // SAFETY: `device` is a live window DC held until the matching
            // restore below.
            let saved = unsafe { SaveDC(device) };
            if saved == 0 {
                eprintln!("QUEUEBACK_WGC_MARKER_DRAW_ERROR could not save window DC");
                // SAFETY: `device` was acquired from `hwnd` above.
                unsafe {
                    ReleaseDC(Some(hwnd), device);
                }
                return;
            }
            let marker_right = MARKER_CROP_LEFT
                + i32::try_from(MARKER_SURFACE_WIDTH).expect("marker width must fit i32");
            let marker_bottom = MARKER_CROP_TOP
                + i32::try_from(MARKER_SURFACE_HEIGHT).expect("marker height must fit i32");
            // SAFETY: the rectangle is bounded by the required fixture client
            // area and `device` is live.
            let region = unsafe {
                ExcludeClipRect(
                    device,
                    MARKER_CROP_LEFT,
                    MARKER_CROP_TOP,
                    marker_right,
                    marker_bottom,
                )
            };
            if region.0 == 0 {
                // SAFETY: `saved` is the state returned by `SaveDC` above and
                // `device` was acquired from `hwnd`.
                unsafe {
                    let _ = RestoreDC(device, saved);
                    ReleaseDC(Some(hwnd), device);
                }
                eprintln!("QUEUEBACK_WGC_MARKER_DRAW_ERROR could not exclude marker crop");
                return;
            }
            Some(saved)
        } else {
            None
        };
        fill(device, &bounds, rgb(8, 12, 24));
        let phase = (frame % 360) as u32;
        for index in 0..12_i32 {
            let bar_width = (width / 6).max(16);
            let travel = width + bar_width;
            let x = ((frame as i32 * (3 + index % 5) + index * 137) % travel) - bar_width;
            let top = index * height / 12;
            let bottom = ((index + 1) * height / 12).min(height);
            let color = rgb(
                ((phase + index as u32 * 23) % 256) as u8,
                ((phase * 3 + index as u32 * 41) % 256) as u8,
                ((phase * 7 + index as u32 * 17) % 256) as u8,
            );
            fill(
                device,
                &RECT {
                    left: x,
                    top,
                    right: (x + bar_width).min(width),
                    bottom,
                },
                color,
            );
        }
        if let Some(saved) = clip_state {
            // SAFETY: `saved` is the state returned by `SaveDC` for this live
            // DC before drawing the background.
            unsafe {
                let _ = RestoreDC(device, saved);
            }
        }

        if let Some(marker) = marker
            && let Err(error) = marker.publish(device, width, height)
        {
            eprintln!("QUEUEBACK_WGC_MARKER_DRAW_ERROR {error:#}");
        }

        unsafe {
            SetBkMode(device, TRANSPARENT);
            SetTextColor(device, rgb(255, 255, 255));
        }
        let label: Vec<u16> = format!("QueueBack WGC fixture frame {frame:08}")
            .encode_utf16()
            .collect();
        unsafe {
            let _ = TextOutW(device, 32, 32, &label);
            ReleaseDC(Some(hwnd), device);
            let _ = GdiFlush();
        }
    }

    fn render_marker_pixels(
        pixels: &mut [u32],
        generation: u16,
        timestamp_ms: Option<u32>,
        duration_ms: u64,
    ) {
        debug_assert_eq!(pixels.len(), MARKER_SURFACE_PIXELS);
        let timestamp = timestamp_ms.map(u64::from);
        let centers = [
            duration_ms / 6,
            duration_ms / 2,
            duration_ms.saturating_mul(5) / 6,
        ];
        let flash_lumas = [112_u8, 152_u8, 192_u8];
        let background = timestamp
            .and_then(|value| {
                centers.iter().zip(flash_lumas).find_map(|(center, luma)| {
                    (value.abs_diff(*center) <= MARKER_FLASH_HALF_WIDTH_MS).then_some(luma)
                })
            })
            .unwrap_or(if timestamp.is_some() { 48 } else { 32 });
        pixels.fill(dib_grayscale(background));

        let payload = marker_visual_payload(generation, timestamp_ms);
        for bit_index in 0..MARKER_BITS {
            let bit = (payload >> (MARKER_BITS - 1 - bit_index)) & 1;
            let column =
                usize::try_from(bit_index % MARKER_COLUMNS).expect("marker column must fit usize");
            let row =
                usize::try_from(bit_index / MARKER_COLUMNS).expect("marker row must fit usize");
            let left = (MARKER_CELL_ORIGIN + column * MARKER_CELL_SIZE) * MARKER_SCALE;
            let top = (MARKER_CELL_ORIGIN + row * MARKER_CELL_SIZE) * MARKER_SCALE;
            let physical_cell_size = MARKER_CELL_SIZE * MARKER_SCALE;
            let color = dib_grayscale(if bit == 0 { 0 } else { 255 });
            for y in top..top + physical_cell_size {
                let start = y * MARKER_SURFACE_WIDTH + left;
                pixels[start..start + physical_cell_size].fill(color);
            }
        }
    }

    fn marker_visual_payload(generation: u16, timestamp_ms: Option<u32>) -> u128 {
        let state = if timestamp_ms.is_some() {
            MARKER_STATE_LIVE
        } else {
            MARKER_STATE_PRE_EPOCH
        };
        let timestamp = timestamp_ms.unwrap_or(MARKER_TIMESTAMP_MASK) & MARKER_TIMESTAMP_MASK;
        let gray = timestamp ^ (timestamp >> 1);
        let without_checksum = (u128::from(MARKER_MAGIC) << 80)
            | (u128::from(generation) << 64)
            | (u128::from(state) << 56)
            | (u128::from(gray) << 32)
            | (u128::from(gray) << 8);
        without_checksum | u128::from(marker_checksum(without_checksum >> 8))
    }

    fn marker_checksum(mut payload: u128) -> u8 {
        let mut checksum = 0xA7_u8;
        for _ in 0..11 {
            checksum ^= u8::try_from(payload & 0xFF).expect("marker byte must fit u8");
            payload >>= 8;
        }
        checksum
    }

    fn dib_grayscale(value: u8) -> u32 {
        u32::from(value) | (u32::from(value) << 8) | (u32::from(value) << 16)
    }

    fn draw_occluder(window: &Window) {
        let Some(hwnd) = hwnd(window) else {
            return;
        };
        let mut bounds = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut bounds) }.is_err() {
            return;
        }
        let device = unsafe { GetDC(Some(hwnd)) };
        if device.0.is_null() {
            return;
        }
        fill(device, &bounds, rgb(255, 0, 255));
        unsafe {
            ReleaseDC(Some(hwnd), device);
            let _ = GdiFlush();
        }
    }

    fn fill(device: windows::Win32::Graphics::Gdi::HDC, bounds: &RECT, color: COLORREF) {
        let brush = unsafe { CreateSolidBrush(color) };
        if brush.0.is_null() {
            return;
        }
        unsafe {
            FillRect(device, bounds, brush);
            let _ = DeleteObject(brush.into());
        }
    }

    fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
        COLORREF(u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn visual_timestamp_payload_binds_generation_state_gray_code_and_checksum() {
            let generation = 0x1234;
            let current = marker_visual_payload(generation, Some(999));
            let next = marker_visual_payload(generation, Some(1_000));
            let current_gray = u32::try_from((current >> 32) & u128::from(MARKER_TIMESTAMP_MASK))
                .expect("gray timestamp must fit u32");
            let duplicate = u32::try_from((current >> 8) & u128::from(MARKER_TIMESTAMP_MASK))
                .expect("duplicate timestamp must fit u32");
            let next_gray = u32::try_from((next >> 32) & u128::from(MARKER_TIMESTAMP_MASK))
                .expect("gray timestamp must fit u32");

            assert_eq!(current >> 80, u128::from(MARKER_MAGIC));
            assert_eq!((current >> 64) & u128::from(u16::MAX), generation.into());
            assert_eq!((current >> 56) & 0xFF, u128::from(MARKER_STATE_LIVE));
            assert_eq!(current_gray, duplicate);
            assert_eq!((current_gray ^ next_gray).count_ones(), 1);
            assert_eq!(current & 0xFF, u128::from(marker_checksum(current >> 8)));
        }

        #[test]
        fn visual_pre_epoch_payload_cannot_decode_as_timestamp_zero() {
            let payload = marker_visual_payload(7, None);

            assert_eq!((payload >> 56) & 0xFF, u128::from(MARKER_STATE_PRE_EPOCH));
            let reserved_timestamp = MARKER_TIMESTAMP_MASK;
            let reserved_gray = reserved_timestamp ^ (reserved_timestamp >> 1);
            assert_eq!(
                (payload >> 32) & u128::from(MARKER_TIMESTAMP_MASK),
                u128::from(reserved_gray)
            );
            assert_eq!(payload & 0xFF, u128::from(marker_checksum(payload >> 8)));
        }

        #[test]
        fn marker_impulse_uses_the_exact_sample_envelope() {
            let markers = [60, 180, 300];
            assert_eq!(marker_impulse_sample(48_000, &markers), 30_000);
            assert_eq!(marker_impulse_sample(48_008, &markers), -27_500);
            assert_eq!(marker_impulse_sample(48_095, &markers), -312);
            assert_eq!(marker_impulse_sample(48_096, &markers), 0);
        }
    }
}

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    windows_fixture::run()
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("The QueueBack WGC fixture is Windows-only.");
}
