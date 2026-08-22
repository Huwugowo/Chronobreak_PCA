#[cfg(target_os = "windows")]
mod windows_fixture {
    use std::io::{self, Write};
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result, bail};
    use windows::Win32::Foundation::{COLORREF, HWND, RECT};
    use windows::Win32::Graphics::Dwm::DwmFlush;
    use windows::Win32::Graphics::Gdi::{
        CreateSolidBrush, DeleteObject, FillRect, GdiFlush, GetDC, ReleaseDC, SetBkMode,
        SetTextColor, TRANSPARENT, TextOutW,
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
    const FRAME_INTERVAL: Duration = Duration::from_millis(16);

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
    }

    pub fn run() -> Result<()> {
        let arguments = parse_arguments()?;
        let _execution_state = ExecutionStateGuard::new(arguments.always_on_top)?;
        let event_loop = EventLoop::new().context("could not create the WGC fixture event loop")?;
        event_loop.set_control_flow(ControlFlow::Wait);
        let mut fixture = Fixture::new(arguments);
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
                value => bail!("unknown argument {value:?}"),
            }
        }
        if restore_after <= action_after {
            bail!("--restore-after-seconds must be later than --action-after-seconds");
        }
        Ok(Arguments {
            duration,
            scenario,
            action_after,
            restore_after,
            always_on_top,
        })
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
    }

    impl Fixture {
        fn new(arguments: Arguments) -> Self {
            let now = Instant::now();
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
            if self.target_reported || self.window.is_none() {
                return;
            }
            match league_replay_recorder::platform::capture_target_for_process(std::process::id()) {
                Ok(target) => {
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
                Err(error) => eprintln!("QUEUEBACK_WGC_TARGET_ERROR {error:#}"),
            }
        }

        fn draw_next_frame(&mut self) {
            if let Some(window) = &self.window {
                draw_frame(window, self.frame);
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
                .with_inner_size(PhysicalSize::new(1920, 1080))
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
                    if self
                        .occluder
                        .as_ref()
                        .is_some_and(|window| window.id() == window_id)
                    {
                        if let Some(window) = &self.occluder {
                            draw_occluder(window);
                        }
                        return;
                    }
                    if let Some(window) = &self.window {
                        draw_frame(window, self.frame);
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

    fn draw_frame(window: &Window, frame: u64) {
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

        let phase = (frame % 360) as u32;
        fill(device, &bounds, rgb(8, 12, 24));
        let width = (bounds.right - bounds.left).max(1);
        let height = (bounds.bottom - bounds.top).max(1);
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
            let _ = DwmFlush();
        }
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
            let _ = DwmFlush();
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
}

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    windows_fixture::run()
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("The QueueBack WGC fixture is Windows-only.");
}
