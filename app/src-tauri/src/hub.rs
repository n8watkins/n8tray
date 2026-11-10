//! The window: created hidden when it first opens and shown only once it has
//! drawn itself (so it appears in one piece), placed 2 px from the tray icon
//! (`n8tray_core::placement`), hidden when it closes or loses focus, and
//! destroyed 60 s after it was hidden, so the WebView's memory goes back to
//! the system while nobody looks (plan 3.6 and 9.2).

use std::time::{Duration, Instant};

use n8tray_core::host::Command;
use n8tray_core::placement::{self, Rect};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use tracing::{info, warn};

use crate::state::AppState;

pub const LABEL: &str = "hub";
/// How long a hidden window is kept before it is destroyed.
pub const KEEP_HIDDEN: Duration = Duration::from_secs(60);
/// The window's width in CSS pixels; its height follows its content.
const WIDTH: f64 = 380.0;
const START_HEIGHT: f64 = 420.0;
/// How long a new window may take to draw before it is shown anyway.
const DRAW_LIMIT: Duration = Duration::from_millis(2500);

#[derive(Default)]
pub struct HubState {
    generation: u64,
    /// When focus loss hid it: the tray click that caused it must not reopen it.
    hidden_by_blur: Option<Instant>,
    /// The window has drawn itself since it was created.
    drawn: bool,
    /// Show it as soon as it has drawn itself.
    show_on_draw: bool,
    /// The tray icon's rectangle, in physical pixels.
    anchor: Option<Rect>,
    /// The content's height in CSS pixels, as the window last reported it.
    content_height: Option<f64>,
    /// The view a new window opens on.
    view: Option<String>,
}

impl HubState {
    pub fn take_view(&mut self) -> Option<String> {
        self.view.take()
    }
}

/// A left click on the icon: open, or close when open.
pub fn toggle(app: &AppHandle, anchor: Option<tauri::Rect>) {
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
        _ => show(app, anchor, None),
    }
}

/// Opens the window on `view` (`hub` when `None`) next to the tray icon.
pub fn show(app: &AppHandle, anchor: Option<tauri::Rect>, view: Option<&str>) {
    let anchor = anchor.or_else(|| {
        app.tray_by_id(crate::tray::TRAY_ID)
            .and_then(|t| t.rect().ok().flatten())
    });
    let state = app.state::<AppState>();
    let existing = app.get_webview_window(LABEL);
    let drawn = {
        let mut hub = state.hub.lock().unwrap();
        hub.generation += 1;
        if let Some(rect) = anchor {
            hub.anchor = Some(physical(rect));
        }
        if existing.is_none() {
            hub.drawn = false;
            hub.content_height = None;
            hub.view = view.map(str::to_string);
        }
        hub.drawn
    };
    let window = match existing {
        Some(w) => {
            let _ = w.emit_to(LABEL, "navigate", view.unwrap_or("hub"));
            w
        }
        None => match create(app) {
            Ok(w) => w,
            Err(e) => {
                warn!("The window could not be created: {e}");
                return;
            }
        },
    };
    if drawn {
        reveal(app, &window);
    } else {
        state.hub.lock().unwrap().show_on_draw = true;
        let generation = state.hub.lock().unwrap().generation;
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(DRAW_LIMIT);
            let state = app.state::<AppState>();
            let late = {
                let mut hub = state.hub.lock().unwrap();
                let late = hub.generation == generation && hub.show_on_draw && !hub.drawn;
                if late {
                    hub.show_on_draw = false;
                }
                late
            };
            if late && let Some(window) = app.get_webview_window(LABEL) {
                warn!("The window did not draw within {DRAW_LIMIT:?}; shown anyway");
                reveal(&app, &window);
            }
        });
    }
}

/// The window reported its content height (CSS pixels) after drawing.
pub fn fit(app: &AppHandle, height: f64) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let state = app.state::<AppState>();
    let first = {
        let mut hub = state.hub.lock().unwrap();
        hub.content_height = Some(height.clamp(80.0, 4000.0));
        let first = !hub.drawn && hub.show_on_draw;
        hub.drawn = true;
        if first {
            hub.show_on_draw = false;
        }
        first
    };
    if first {
        reveal(app, &window);
    } else if window.is_visible().unwrap_or(false) {
        place(app, &window, false);
    }
}

fn reveal(app: &AppHandle, window: &WebviewWindow) {
    place(app, window, true);
    if std::env::var_os("N8TRAY_NO_ACTIVATE").is_some() && show_inactive(window) {
        // Testing on a PC someone uses: shown without taking the focus.
    } else {
        let _ = window.show();
        let _ = window.set_focus();
    }
    app.state::<AppState>().send(Command::WindowOpen(true));
}

#[cfg(windows)]
fn frame_insets(window: &WebviewWindow) -> Option<(i32, i32, i32, i32)> {
    let hwnd = window.hwnd().ok()?;
    n8tray_platform::window::frame_insets(hwnd.0 as isize)
}

#[cfg(not(windows))]
fn frame_insets(_window: &WebviewWindow) -> Option<(i32, i32, i32, i32)> {
    None
}

#[cfg(windows)]
fn show_inactive(window: &WebviewWindow) -> bool {
    window
        .hwnd()
        .is_ok_and(|h| n8tray_platform::window::show_inactive(h.0 as isize))
}

#[cfg(not(windows))]
fn show_inactive(_window: &WebviewWindow) -> bool {
    false
}

/// `N8TRAY_THEME=light|dark`: the window's theme instead of the system's,
/// for screenshots of both on one PC.
fn test_theme() -> Option<tauri::Theme> {
    match std::env::var("N8TRAY_THEME").ok()?.as_str() {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    }
}

/// `N8TRAY_ANCHOR=x,y,w,h[;x,y,w,h]`: a tray icon rectangle, and optionally
/// a work area, in physical pixels, instead of the real ones. For testing
/// placement on taskbar edges the PC does not have.
fn test_anchor() -> Option<(Rect, Option<Rect>)> {
    let value = std::env::var("N8TRAY_ANCHOR").ok()?;
    let rect = |text: &str| -> Option<Rect> {
        let n: Vec<i32> = text
            .split(',')
            .filter_map(|v| v.trim().parse().ok())
            .collect();
        (n.len() == 4).then(|| Rect::new(n[0], n[1], n[2], n[3]))
    };
    let mut parts = value.split(';');
    let anchor = rect(parts.next()?)?;
    Some((anchor, parts.next().and_then(rect)))
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
        hub.show_on_draw = false;
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
    // Arguments set for every WebView2 (msedgedriver's remote debugging
    // port, for WebDriver tests) would be replaced by ours: keep them.
    match std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") {
        Ok(given) if !given.trim().is_empty() => format!("{given} {WEBVIEW_ARGS}"),
        _ => WEBVIEW_ARGS.to_string(),
    }
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
        .inner_size(WIDTH, START_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .shadow(true)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .theme(test_theme())
        .build()?;
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        n8tray_platform::window::style_flyout(hwnd.0 as isize);
    }
    let handle = app.clone();
    window.on_window_event(move |event| match event {
        // Only a shown window hides on focus loss: a window still being
        // created can lose a focus it never had, and must still appear.
        WindowEvent::Focused(false)
            if handle
                .get_webview_window(LABEL)
                .is_some_and(|w| w.is_visible().unwrap_or(false)) =>
        {
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
        WindowEvent::ScaleFactorChanged { .. } => {
            if let Some(window) = handle.get_webview_window(LABEL)
                && window.is_visible().unwrap_or(false)
            {
                place(&handle, &window, false);
            }
        }
        _ => {}
    });
    info!(
        "The window was created in {} ms",
        started.elapsed().as_millis()
    );
    Ok(window)
}

fn physical(rect: tauri::Rect) -> Rect {
    // The tray reports physical pixels; a logical rectangle is taken at 1x.
    let p = rect.position.to_physical::<i32>(1.0);
    let s = rect.size.to_physical::<i32>(1.0);
    Rect::new(p.x, p.y, s.width, s.height)
}

/// Sizes the window to its content and places it next to the icon, inside
/// the work area of the icon's monitor.
fn place(app: &AppHandle, window: &WebviewWindow, log: bool) {
    let (mut anchor, content_height) = {
        let state = app.state::<AppState>();
        let hub = state.hub.lock().unwrap();
        (hub.anchor, hub.content_height.unwrap_or(START_HEIGHT))
    };
    let test = test_anchor();
    if let Some((rect, _)) = test {
        anchor = Some(rect);
    }
    let monitors = app.available_monitors().unwrap_or_default();
    let bounds = |m: &tauri::Monitor| {
        Rect::new(
            m.position().x,
            m.position().y,
            m.size().width as i32,
            m.size().height as i32,
        )
    };
    let monitor = anchor
        .and_then(|a| {
            let (cx, cy) = (a.x + a.width / 2, a.y + a.height / 2);
            monitors
                .iter()
                .find(|m| bounds(m).contains(cx, cy))
                .cloned()
        })
        .or_else(|| app.primary_monitor().ok().flatten())
        .or_else(|| monitors.first().cloned());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let screen = bounds(&monitor);
    let area = monitor.work_area();
    let work = test.and_then(|(_, work)| work).unwrap_or(Rect::new(
        area.position.x,
        area.position.y,
        area.size.width as i32,
        area.size.height as i32,
    ));
    // Without an icon (no tray rectangle yet): the corner of the work area
    // by the taskbar, bottom-right by default.
    let anchor = anchor.unwrap_or(Rect::new(work.right(), work.bottom(), 1, 1));
    let width = (WIDTH * scale).round() as i32;
    let wanted = (content_height * scale).round() as i32;
    // What users see is the visible frame: the inner size plus any border
    // the OS draws, inside invisible resize borders (Windows). Place that.
    let insets = frame_insets(window);
    let border = |outer: i32, inner: i32| (outer - inner).max(0);
    let (extra_w, extra_h) = match (insets, window.outer_size(), window.inner_size()) {
        (Some((l, t, r, b)), Ok(o), Ok(i)) => (
            border(o.width as i32 - l - r, i.width as i32),
            border(o.height as i32 - t - b, i.height as i32),
        ),
        _ => (0, 0),
    };
    let room = placement::place(
        width + extra_w,
        wanted + extra_h,
        anchor,
        screen,
        work,
        placement::GAP,
    )
    .max_height
        - extra_h;
    let height = wanted.min(room);
    let _ = window.set_size(PhysicalSize::new(width as u32, height as u32));
    let spot = placement::place(
        width + extra_w,
        height + extra_h,
        anchor,
        screen,
        work,
        placement::GAP,
    );
    let (left, top) = insets.map(|(l, t, _, _)| (l, t)).unwrap_or((0, 0));
    let _ = window.set_position(PhysicalPosition::new(spot.x - left, spot.y - top));
    let _ = window.emit_to(LABEL, "fit", height < wanted);
    if log {
        info!(
            "The window opens at {},{} ({}x{}, scale {scale}) by the {:?} taskbar; icon at {},{} ({}x{}), work area {},{} ({}x{})",
            spot.x,
            spot.y,
            width + extra_w,
            height + extra_h,
            spot.edge,
            anchor.x,
            anchor.y,
            anchor.width,
            anchor.height,
            work.x,
            work.y,
            work.width,
            work.height
        );
    }
}
