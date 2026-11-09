//! The window: created when it opens, hidden when it closes or loses focus,
//! and destroyed 60 s after it was hidden, so the WebView's memory goes back
//! to the system while nobody looks (plan 3.6 and 9.2).

use std::time::{Duration, Instant};

use n8tray_core::host::Command;
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Rect, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use tracing::{info, warn};

use crate::state::AppState;

pub const LABEL: &str = "hub";
/// How long a hidden window is kept before it is destroyed.
pub const KEEP_HIDDEN: Duration = Duration::from_secs(60);
const WIDTH: f64 = 400.0;
const HEIGHT: f64 = 520.0;
const GAP: i32 = 8;

#[derive(Default)]
pub struct HubState {
    generation: u64,
    /// When focus loss hid it: the tray click that caused it must not reopen it.
    hidden_by_blur: Option<Instant>,
}

/// A left click on the icon: open, or close when open.
pub fn toggle(app: &AppHandle, anchor: Option<Rect>) {
    let state = app.state::<AppState>();
    let just_hidden = state
        .hub
        .lock()
        .unwrap()
        .hidden_by_blur
        .is_some_and(|t| t.elapsed() < Duration::from_millis(400));
    match app.get_webview_window(LABEL) {
        Some(w) if w.is_visible().unwrap_or(false) => hide(app),
        _ if just_hidden => {}
        _ => show(app, anchor),
    }
}

pub fn show(app: &AppHandle, anchor: Option<Rect>) {
    let state = app.state::<AppState>();
    state.hub.lock().unwrap().generation += 1;
    let window = match app.get_webview_window(LABEL) {
        Some(w) => w,
        None => match create(app) {
            Ok(w) => w,
            Err(e) => {
                warn!("The window could not be created: {e}");
                return;
            }
        },
    };
    place(app, &window, anchor);
    let _ = window.show();
    let _ = window.set_focus();
    state.send(Command::WindowOpen(true));
}

pub fn hide(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let _ = window.hide();
    let state = app.state::<AppState>();
    let generation = {
        let mut hub = state.hub.lock().unwrap();
        hub.generation += 1;
        hub.generation
    };
    state.send(Command::WindowOpen(false));
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(KEEP_HIDDEN);
        let state = app.state::<AppState>();
        if state.hub.lock().unwrap().generation != generation {
            return;
        }
        if let Some(window) = app.get_webview_window(LABEL)
            && !window.is_visible().unwrap_or(true)
        {
            info!("The window was hidden for {KEEP_HIDDEN:?}: destroyed to free its memory");
            let _ = window.destroy();
            // Give the pages the WebView left behind back to the system.
            std::thread::sleep(Duration::from_secs(2));
            n8tray_platform::memory::trim_own_working_set();
        }
    });
}

/// WebView2's browser arguments: Tauri's defaults plus what keeps a small
/// window small (D19). `N8TRAY_WEBVIEW_ARGS` replaces them, for measuring.
#[cfg(windows)]
fn webview_args() -> String {
    if let Ok(args) = std::env::var("N8TRAY_WEBVIEW_ARGS") {
        return args;
    }
    WEBVIEW_ARGS.to_string()
}

#[cfg(windows)]
const WEBVIEW_ARGS: &str = concat!(
    // Tauri's defaults.
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection",
    // A small static page needs no GPU process and one renderer at most.
    " --disable-gpu --in-process-gpu --renderer-process-limit=1",
    " --disable-background-networking",
);

fn create(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let started = Instant::now();
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()));
    #[cfg(windows)]
    let builder = builder.additional_browser_args(&webview_args());
    let window = builder
        .title("N8 Tray")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .build()?;
    let handle = app.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Focused(false) => {
            handle
                .state::<AppState>()
                .hub
                .lock()
                .unwrap()
                .hidden_by_blur = Some(Instant::now());
            hide(&handle);
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(&handle);
        }
        _ => {}
    });
    info!(
        "The window was created in {} ms",
        started.elapsed().as_millis()
    );
    Ok(window)
}

/// Next to the icon, inside the work area of its monitor; bottom-right of
/// the primary monitor without an anchor.
fn place(app: &AppHandle, window: &WebviewWindow, anchor: Option<Rect>) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let size: PhysicalSize<i32> = window
        .outer_size()
        .map(|s| PhysicalSize::new(s.width as i32, s.height as i32))
        .unwrap_or(PhysicalSize::new(
            (WIDTH * scale) as i32,
            (HEIGHT * scale) as i32,
        ));
    let icon = anchor.map(|r| {
        let p = r.position.to_physical::<i32>(scale);
        let s = r.size.to_physical::<i32>(scale);
        (p.x, p.y, s.width, s.height)
    });
    let monitor = match icon {
        Some((x, y, _, _)) => app.monitor_from_point(x as f64, y as f64).ok().flatten(),
        None => None,
    }
    .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let area = monitor.work_area();
    let (ax, ay) = (area.position.x, area.position.y);
    let (aw, ah) = (area.size.width as i32, area.size.height as i32);
    let (x, y) = match icon {
        None => (ax + aw - size.width - GAP, ay + ah - size.height - GAP),
        Some((ix, iy, iw, ih)) => {
            let centered_x = ix + iw / 2 - size.width / 2;
            let centered_y = iy + ih / 2 - size.height / 2;
            if ix + iw <= ax {
                (ax + GAP, centered_y) // taskbar on the left
            } else if ix >= ax + aw {
                (ax + aw - size.width - GAP, centered_y) // on the right
            } else if iy >= ay + ah / 2 {
                (centered_x, iy - size.height - GAP) // at the bottom
            } else {
                (centered_x, iy + ih + GAP) // at the top
            }
        }
    };
    let x = x.clamp(ax + GAP, (ax + aw - size.width - GAP).max(ax));
    let y = y.clamp(ay + GAP, (ay + ah - size.height - GAP).max(ay));
    let _ = window.set_position(PhysicalPosition::new(x, y));
}
