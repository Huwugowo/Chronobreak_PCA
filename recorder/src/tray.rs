use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use tokio::sync::mpsc;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::WindowId;

use league_replay_recorder::config::Config;
use league_replay_recorder::service::{ServiceCommand, ServiceEvent};

#[derive(Debug)]
enum UserEvent {
    Menu(MenuEvent),
    Service(ServiceEvent),
}

pub fn run(
    config: Config,
    command_sender: mpsc::UnboundedSender<ServiceCommand>,
    command_receiver: mpsc::UnboundedReceiver<ServiceCommand>,
    smoke_test_timeout: Option<Duration>,
) -> Result<()> {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .context("failed to create tray event loop")?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();

    let worker = spawn_worker(
        config,
        command_receiver,
        proxy.clone(),
        command_sender.clone(),
        smoke_test_timeout,
    )?;

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event));
    }));

    let mut application = TrayApplication::new(command_sender);
    let tray_result = event_loop
        .run_app(&mut application)
        .context("tray event loop failed");
    application.request_shutdown();
    MenuEvent::set_event_handler::<fn(MenuEvent)>(None);

    let worker_result = worker
        .join()
        .map_err(|_| anyhow!("recorder service thread panicked"))
        .and_then(|result| result);
    match (tray_result, worker_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(tray_error), Err(worker_error)) => Err(tray_error.context(format!(
            "recorder service also failed while the tray stopped: {worker_error:#}"
        ))),
    }
}

fn spawn_worker(
    config: Config,
    command_receiver: mpsc::UnboundedReceiver<ServiceCommand>,
    proxy: EventLoopProxy<UserEvent>,
    command_sender: mpsc::UnboundedSender<ServiceCommand>,
    smoke_test_timeout: Option<Duration>,
) -> Result<std::thread::JoinHandle<Result<()>>> {
    let worker = std::thread::Builder::new()
        .name("recorder-service".to_owned())
        .spawn(move || {
            let sink = std::sync::Arc::new(move |event| {
                let _ = proxy.send_event(UserEvent::Service(event));
            });
            let service_sink = std::sync::Arc::clone(&sink);
            let result = (|| {
                let runtime = crate::build_service_runtime()?;
                runtime.block_on(async move {
                    let smoke_task = smoke_test_timeout.map(|timeout| {
                        tokio::spawn(async move {
                            tokio::time::sleep(timeout).await;
                            let _ = command_sender.send(ServiceCommand::Shutdown);
                        })
                    });
                    let result = league_replay_recorder::service::run(
                        config,
                        command_receiver,
                        service_sink,
                    )
                    .await;
                    if let Some(task) = smoke_task {
                        task.abort();
                        let _ = task.await;
                    }
                    result
                })
            })();
            if let Err(error) = &result {
                sink(ServiceEvent::Error {
                    message: format!("Recorder service stopped: {error:#}"),
                });
                sink(ServiceEvent::ShutdownComplete);
            }
            result
        })
        .context("failed to start recorder service thread")?;
    Ok(worker)
}

struct TrayApplication {
    tray: Option<TrayIcon>,
    idle_icon: Icon,
    recording_icon: Icon,
    error_icon: Icon,
    quit_id: MenuId,
    command_sender: mpsc::UnboundedSender<ServiceCommand>,
    state: TrayState,
    shutting_down: bool,
}

#[derive(Debug, Clone)]
enum TrayState {
    Idle,
    Recording(PathBuf),
    Error(String),
}

impl TrayApplication {
    fn new(command_sender: mpsc::UnboundedSender<ServiceCommand>) -> Self {
        Self {
            tray: None,
            idle_icon: circle_icon([128, 138, 150, 255]),
            recording_icon: circle_icon([224, 48, 58, 255]),
            error_icon: circle_icon([245, 158, 11, 255]),
            quit_id: MenuId::new("quit"),
            command_sender,
            state: TrayState::Idle,
            shutting_down: false,
        }
    }

    fn create_tray(&mut self) -> Result<()> {
        if self.tray.is_some() {
            return Ok(());
        }
        let menu = Menu::new();
        let quit = MenuItem::with_id(self.quit_id.clone(), "Quit", true, None);
        menu.append(&quit)
            .context("failed to add tray Quit action")?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("League Replay — Idle")
            .with_icon(self.idle_icon.clone())
            .build()
            .context("failed to create tray icon")?;
        self.tray = Some(tray);
        self.apply_state()
    }

    fn apply_state(&self) -> Result<()> {
        let Some(tray) = &self.tray else {
            return Ok(());
        };
        match &self.state {
            TrayState::Idle => {
                tray.set_icon(Some(self.idle_icon.clone()))?;
                tray.set_tooltip(Some("League Replay — Idle"))?;
            }
            TrayState::Recording(directory) => {
                tray.set_icon(Some(self.recording_icon.clone()))?;
                tray.set_tooltip(Some(format!(
                    "League Replay — Recording\n{}",
                    directory.display()
                )))?;
            }
            TrayState::Error(message) => {
                tray.set_icon(Some(self.error_icon.clone()))?;
                tray.set_tooltip(Some(format!("League Replay — Error\n{message}")))?;
            }
        }
        Ok(())
    }

    fn request_shutdown(&mut self) {
        if self.shutting_down {
            return;
        }
        self.shutting_down = true;
        self.state = TrayState::Idle;
        if let Some(tray) = &self.tray {
            let _ = tray.set_tooltip(Some("League Replay — Stopping…"));
        }
        let _ = self.command_sender.send(ServiceCommand::Shutdown);
    }
}

impl ApplicationHandler<UserEvent> for TrayApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.create_tray() {
            tracing::error!(%error, "tray creation failed");
            self.request_shutdown();
            event_loop.exit();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(event) if event.id == self.quit_id => {
                self.request_shutdown();
            }
            UserEvent::Menu(_) => {}
            UserEvent::Service(ServiceEvent::Idle) => {
                self.state = TrayState::Idle;
                let _ = self.apply_state();
            }
            UserEvent::Service(ServiceEvent::Recording { directory }) => {
                self.state = TrayState::Recording(directory);
                let _ = self.apply_state();
            }
            UserEvent::Service(ServiceEvent::Error { message }) => {
                self.state = TrayState::Error(message);
                let _ = self.apply_state();
            }
            UserEvent::Service(ServiceEvent::ShutdownComplete) => {
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        let _ = self.command_sender.send(ServiceCommand::Shutdown);
    }
}

fn circle_icon(color: [u8; 4]) -> Icon {
    const SIZE: u32 = 32;
    let mut rgba = vec![0_u8; (SIZE * SIZE * 4) as usize];
    let center = (SIZE as f32 - 1.0) / 2.0;
    let radius = center - 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance <= radius {
                let edge_alpha = ((radius - distance).clamp(0.0, 1.0) * 255.0) as u8;
                let index = ((y * SIZE + x) * 4) as usize;
                rgba[index] = color[0];
                rgba[index + 1] = color[1];
                rgba[index + 2] = color[2];
                rgba[index + 3] = color[3].min(edge_alpha.max(1));
            }
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).expect("generated tray icon has valid dimensions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_icons_are_valid() {
        let _ = circle_icon([128, 128, 128, 255]);
        let _ = circle_icon([255, 0, 0, 255]);
    }
}
