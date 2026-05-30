use crate::{
    LiveCaptureSettings,
    browser::open_url_in_browser,
    error::Result,
    google_sheets::GoogleSheetsClient,
    handle_live_packet,
    live::process_live_capture_until,
    store::TradeStore,
    web::{WebNotifier, WebServer},
};

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};
use tao::{
    event::{Event, StartCause},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
};
use tokio::runtime::Handle;
use tray_icon::{
    Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};

const APP_NAME_LABEL: &str = "Albion Accountant";
const START_CAPTURE_LABEL: &str = "Start Capture";
const STOP_CAPTURE_LABEL: &str = "Stop Capture";

enum UserEvent {
    TrayIconEvent(tray_icon::TrayIconEvent),
    MenuEvent(tray_icon::menu::MenuEvent),
    CaptureStopped(std::result::Result<(), String>),
}

pub(crate) fn run_live_tray(
    settings: LiveCaptureSettings,
    trade_store: TradeStore,
    sheets_client: Option<GoogleSheetsClient>,
    runtime_handle: Handle,
    web_server: WebServer,
) -> Result<()> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let capture_config = CaptureConfig {
        settings,
        trade_store,
        sheets_client,
        runtime_handle,
    };

    let tray_menu = Menu::new();
    let app_name = MenuItem::new(APP_NAME_LABEL, true, None);
    let toggle_capture = MenuItem::new(STOP_CAPTURE_LABEL, true, None);
    let exit = MenuItem::new("Exit", true, None);
    tray_menu
        .append_items(&[&app_name, &toggle_capture, &exit])
        .map_err(|err| format!("failed to build tray menu: {err}"))?;

    TrayIconEvent::set_event_handler(Some({
        let proxy = proxy.clone();
        move |event| {
            let _ = proxy.send_event(UserEvent::TrayIconEvent(event));
        }
    }));
    MenuEvent::set_event_handler(Some({
        let proxy = proxy.clone();
        move |event| {
            let _ = proxy.send_event(UserEvent::MenuEvent(event));
        }
    }));

    let app_name_id = app_name.id().clone();
    let toggle_capture_id = toggle_capture.id().clone();
    let exit_id = exit.id().clone();
    let mut tray_icon: Option<TrayIcon> = None;
    let web_url = web_server.url.clone();
    let web_notifier = web_server.notifier.clone();
    let mut capture =
        CaptureWorker::start(capture_config.clone(), proxy.clone(), web_notifier.clone());

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {
                match TrayIconBuilder::new()
                    .with_menu(Box::new(tray_menu.clone()))
                    .with_tooltip("Albion Accountant")
                    .with_icon(tray_icon_image())
                    .build()
                {
                    Ok(icon) => tray_icon = Some(icon),
                    Err(error) => {
                        eprintln!("ERROR:albion:failed to create tray icon: {error}");
                        capture.stop();
                        capture.join();
                        *control_flow = ControlFlow::Exit;
                    }
                }
            }
            Event::UserEvent(UserEvent::MenuEvent(event)) if event.id == app_name_id => {
                if let Err(err) = open_url_in_browser(&web_url) {
                    eprintln!(
                        "WARN:albion:failed to open web app automatically: {err}. Open this URL manually: {web_url}"
                    );
                }
            }
            Event::UserEvent(UserEvent::MenuEvent(event)) if event.id == toggle_capture_id => {
                if capture.is_running() {
                    capture.stop();
                    capture.join();
                    toggle_capture.set_text(START_CAPTURE_LABEL);
                } else {
                    capture = CaptureWorker::start(capture_config.clone(), proxy.clone(), web_notifier.clone());
                    toggle_capture.set_text(STOP_CAPTURE_LABEL);
                }
            }
            Event::UserEvent(UserEvent::MenuEvent(event)) if event.id == exit_id => {
                eprintln!("INFO:albion:exiting from tray menu");
                capture.stop();
                capture.join();
                tray_icon.take();
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(UserEvent::CaptureStopped(result)) => {
                if let Err(error) = result {
                    eprintln!("ERROR:albion:live capture stopped unexpectedly: {error}");
                }
                capture.join_finished();
                toggle_capture.set_text(START_CAPTURE_LABEL);
            }
            Event::UserEvent(UserEvent::TrayIconEvent(TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            })) => {
                let web_url = web_server.url.clone();
                if let Err(err) = open_url_in_browser(&web_url) {
                    eprintln!(
                        "WARN:albion:failed to open web app automatically: {err}. Open this URL manually: {web_url}"
                    );
                }
            }
            Event::UserEvent(UserEvent::TrayIconEvent(_)) => {}
            _ => {}
        }
    })
}

#[derive(Clone)]
struct CaptureConfig {
    settings: LiveCaptureSettings,
    trade_store: TradeStore,
    sheets_client: Option<GoogleSheetsClient>,
    runtime_handle: Handle,
}

struct CaptureWorker {
    stop_requested: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
}

impl CaptureWorker {
    fn start(
        config: CaptureConfig,
        proxy: EventLoopProxy<UserEvent>,
        notifier: WebNotifier,
    ) -> Self {
        let stop_requested = Arc::new(AtomicBool::new(false));
        let worker_stop_requested = Arc::clone(&stop_requested);
        let join_handle = thread::spawn(move || {
            let result = process_live_capture_until(
                config.settings.debug,
                worker_stop_requested,
                move |packet| {
                    handle_live_packet(
                        &packet,
                        config.settings,
                        &config.trade_store,
                        config.sheets_client.as_ref(),
                        &config.runtime_handle,
                        &notifier,
                    )
                },
            )
            .map_err(|err| err.0);
            let _ = proxy.send_event(UserEvent::CaptureStopped(result));
        });

        Self {
            stop_requested,
            join_handle: Some(join_handle),
        }
    }

    fn is_running(&self) -> bool {
        self.join_handle.is_some()
    }

    fn stop(&self) {
        self.stop_requested.store(true, Ordering::Relaxed);
    }

    fn join(&mut self) {
        if let Some(join_handle) = self.join_handle.take()
            && let Err(error) = join_handle.join()
        {
            eprintln!("ERROR:albion:live capture worker panicked: {error:?}");
        }
    }

    fn join_finished(&mut self) {
        if self
            .join_handle
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            self.join();
        }
    }
}

fn tray_icon_image() -> Icon {
    const WIDTH: u32 = 32;
    const HEIGHT: u32 = 32;

    let mut rgba = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let dx = x as i32 - 16;
            let dy = y as i32 - 16;
            let inside_coin = dx * dx + dy * dy <= 14 * 14;
            let inside_cutout = (10..=22).contains(&x) && (9..=23).contains(&y);
            let pixel = if inside_coin && !inside_cutout {
                [231, 175, 64, 255]
            } else if inside_coin {
                [58, 45, 31, 255]
            } else {
                [0, 0, 0, 0]
            };
            rgba.extend_from_slice(&pixel);
        }
    }

    Icon::from_rgba(rgba, WIDTH, HEIGHT).expect("generated tray icon should be valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tray_icon_is_valid() {
        let _ = tray_icon_image();
    }
}
