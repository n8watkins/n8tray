//! The window, in lane N0 a placeholder: created on demand off screen and
//! without activation, placed 2 px from the tray icon by
//! `n8tray_core::placement` inside the work area of the icon's monitor,
//! shown with or without activation as today, hidden when it loses focus or
//! on Escape, and dropped 10 s after it was hidden, then the working set
//! trimmed (D17). The same rules as the Tauri app's `hub.rs`.
//!
//! Slint re-activates a window every time it shows it, so after the first
//! show (off screen, never activated) the window is shown and hidden only
//! through Win32 (`n8tray_platform::window`); Slint keeps thinking it is
//! visible until it is dropped.

use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use n8tray_core::host::Command;
use n8tray_core::model::{HostSnapshot, LevelName};
use n8tray_core::placement::{self, Rect};
use slint::{ComponentHandle, ModelRc, VecModel};
use tracing::{info, warn};

use crate::{AppLine, HubWindow};

/// How long a hidden window is kept before it is dropped (D17).
pub const KEEP_HIDDEN: Duration = Duration::from_secs(10);
/// The window's width in logical pixels; its height follows its content.
const WIDTH: f64 = 380.0;
/// A tray click this soon after a blur hid the window is the click that
/// caused the blur: it must not reopen it.
const REOPEN_GUARD: Duration = Duration::from_millis(400);
/// After a drop, how long to wait before trimming (what the window left
/// behind is freed by then).
const TRIM_AFTER_DROP: Duration = Duration::from_secs(2);

/// What happens to a window hidden for [`KEEP_HIDDEN`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhenHidden {
    /// Dropped, then the working set trimmed (the default, D17).
    Drop,
    /// Kept, and the working set trimmed (`N8TRAY_WINDOW=keep`, for measuring).
    Keep,
}

impl WhenHidden {
    pub fn from_env() -> WhenHidden {
        match std::env::var("N8TRAY_WINDOW").as_deref() {
            Ok("keep") => WhenHidden::Keep,
            _ => WhenHidden::Drop,
        }
    }
}

pub struct Hub {
    ui: Option<HubWindow>,
    hwnd: Option<isize>,
    visible: bool,
    /// Counts shows and hides, so a timer set for one hide does nothing
    /// after a later show.
    generation: u64,
    hidden_by_blur: Option<Instant>,
    /// The tray icon's rectangle, in physical pixels.
    anchor: Option<Rect>,
    /// Show with activation when the native window turns up.
    pending: Option<bool>,
    /// Waits for the native window after Slint's first show.
    wait: slint::Timer,
    waited: u32,
    retire: slint::Timer,
    trim: slint::Timer,
    when_hidden: WhenHidden,
    host: Option<Sender<Command>>,
}

impl Hub {
    pub fn new(when_hidden: WhenHidden) -> Hub {
        Hub {
            ui: None,
            hwnd: None,
            visible: false,
            generation: 0,
            hidden_by_blur: None,
            anchor: None,
            pending: None,
            wait: slint::Timer::default(),
            waited: 0,
            retire: slint::Timer::default(),
            trim: slint::Timer::default(),
            when_hidden,
            host: None,
        }
    }

    pub fn set_host(&mut self, host: Sender<Command>) {
        self.host = Some(host);
    }

    fn send(&self, command: Command) {
        if let Some(host) = &self.host {
            let _ = host.send(command);
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// A left click on the tray icon: open, or close when open.
    pub fn toggle(&mut self, anchor: Option<Rect>, snapshot: Option<&HostSnapshot>) {
        let just_hidden = self
            .hidden_by_blur
            .is_some_and(|t| t.elapsed() < REOPEN_GUARD);
        if self.visible {
            self.hide(false);
        } else if !just_hidden {
            self.show(anchor, snapshot);
        }
    }

    /// Opens the window next to the tray icon (`anchor`, or where it was).
    pub fn show(&mut self, anchor: Option<Rect>, snapshot: Option<&HostSnapshot>) {
        self.generation += 1;
        self.retire.stop();
        self.trim.stop();
        if anchor.is_some() {
            self.anchor = anchor;
        }
        if self.ui.is_none() {
            match create() {
                Ok(ui) => self.ui = Some(ui),
                Err(e) => {
                    warn!("The window could not be created: {e}");
                    return;
                }
            }
        }
        if let Some(ui) = &self.ui {
            fill(ui, snapshot);
        }
        // Testing on a PC someone uses: shown without taking the focus.
        let activate = std::env::var_os("N8TRAY_NO_ACTIVATE").is_none();
        self.pending = Some(activate);
        self.waited = 0;
        self.reveal_when_ready();
    }

    /// Shows the window once its native window exists (Slint creates it
    /// during its first show; it may take an event loop turn).
    pub fn reveal_when_ready(&mut self) {
        if self.hwnd.is_none() {
            let found = self.ui.as_ref().and_then(native_handle);
            match found {
                Some(hwnd) => self.adopt(hwnd),
                None if self.waited < 200 => {
                    self.waited += 1;
                    self.wait.start(
                        slint::TimerMode::SingleShot,
                        Duration::from_millis(10),
                        || crate::shell::with(|s| s.hub.reveal_when_ready()),
                    );
                    return;
                }
                None => {
                    warn!("The window has no native window after 2 s");
                    self.pending = None;
                    return;
                }
            }
        }
        let (Some(activate), Some(hwnd)) = (self.pending.take(), self.hwnd) else {
            return;
        };
        self.place(true);
        if activate {
            n8tray_platform::window::show_active(hwnd);
        } else {
            n8tray_platform::window::show_inactive(hwnd);
        }
        if let Some(ui) = &self.ui {
            ui.window().request_redraw();
        }
        // Moving between monitors of different scale can resize the window
        // behind our back: place it once more now that it is shown.
        self.place(false);
        if !self.visible {
            self.visible = true;
            self.send(Command::WindowOpen(true));
        }
    }

    /// The native window exists: hide it at once (Slint showed it off
    /// screen), style it as a flyout and listen to it.
    fn adopt(&mut self, hwnd: isize) {
        n8tray_platform::window::hide(hwnd);
        n8tray_platform::window::style_flyout(hwnd);
        self.hwnd = Some(hwnd);
        let Some(ui) = &self.ui else { return };
        ui.set_dark(dark(ui));
        listen(ui);
        // D25: a frameless Tauri window kept 7 px invisible borders; say what
        // this one keeps, so the placement code path stays honest.
        let insets = n8tray_platform::window::frame_insets(hwnd);
        info!("The window's frame insets (left, top, right, bottom): {insets:?}");
    }

    /// Hides the window; `from_blur` when focus loss hid it.
    pub fn hide(&mut self, from_blur: bool) {
        if !self.visible {
            return;
        }
        if from_blur {
            self.hidden_by_blur = Some(Instant::now());
        }
        if let Some(hwnd) = self.hwnd {
            n8tray_platform::window::hide(hwnd);
        }
        self.visible = false;
        self.generation += 1;
        self.send(Command::WindowOpen(false));
        let generation = self.generation;
        self.retire
            .start(slint::TimerMode::SingleShot, KEEP_HIDDEN, move || {
                crate::shell::with(|s| s.hub.retire(generation))
            });
    }

    /// The window was hidden for [`KEEP_HIDDEN`].
    fn retire(&mut self, generation: u64) {
        if generation != self.generation || self.visible {
            return;
        }
        match self.when_hidden {
            WhenHidden::Drop => {
                if let Some(ui) = self.ui.take() {
                    let _ = ui.hide();
                    drop(ui);
                    info!("The window was hidden for {KEEP_HIDDEN:?}: dropped to free its memory");
                }
                self.hwnd = None;
                self.trim
                    .start(slint::TimerMode::SingleShot, TRIM_AFTER_DROP, || {
                        n8tray_platform::memory::trim_own_working_set();
                    });
            }
            WhenHidden::Keep => {
                info!("The window was hidden for {KEEP_HIDDEN:?}: kept, memory trimmed");
                n8tray_platform::memory::trim_own_working_set();
            }
        }
    }

    /// The window lost the focus: a shown window hides (D25).
    pub fn blurred(&mut self) {
        if self.visible {
            self.hide(true);
        }
    }

    /// The snapshot changed while the window is open.
    pub fn refresh(&mut self, snapshot: &HostSnapshot) {
        if let (Some(ui), true) = (&self.ui, self.visible) {
            fill(ui, Some(snapshot));
            self.place(false);
        }
    }

    /// The display scale changed under a shown window.
    pub fn rescaled(&mut self) {
        if self.visible {
            self.place(false);
        }
    }

    /// Drops the window now (the shell is exiting).
    pub fn close(&mut self) {
        self.retire.stop();
        self.trim.stop();
        self.wait.stop();
        if let Some(ui) = self.ui.take() {
            let _ = ui.hide();
        }
        self.hwnd = None;
        self.visible = false;
    }

    /// Sizes the window to its content and places it next to the icon,
    /// inside the work area of the icon's monitor.
    fn place(&self, log: bool) {
        let (Some(hwnd), Some(ui)) = (self.hwnd, &self.ui) else {
            return;
        };
        let test = test_anchor(std::env::var("N8TRAY_ANCHOR").ok().as_deref());
        let anchor = test.map(|(rect, _)| rect).or(self.anchor);
        let monitor = anchor
            .and_then(|a| {
                n8tray_platform::window::monitor_at(a.x + a.width / 2, a.y + a.height / 2)
            })
            .or_else(n8tray_platform::window::primary_monitor);
        let Some(monitor) = monitor else {
            return;
        };
        let rect = |(x, y, w, h): (i32, i32, i32, i32)| Rect::new(x, y, w, h);
        let screen = rect(monitor.bounds);
        let work = test
            .and_then(|(_, work)| work)
            .unwrap_or(rect(monitor.work));
        // Without an icon (no tray rectangle yet): the corner of the work
        // area by the taskbar, bottom-right by default.
        let anchor = anchor.unwrap_or(Rect::new(work.right(), work.bottom(), 1, 1));
        let scale = monitor.scale;
        let content = f64::from(ui.get_content_height()).clamp(80.0, 4000.0);
        let width = (WIDTH * scale).round() as i32;
        let wanted = (content * scale).round() as i32;
        let room = placement::place(width, wanted, anchor, screen, work, placement::GAP).max_height;
        let height = wanted.min(room);
        let spot = placement::place(width, height, anchor, screen, work, placement::GAP);
        // What users see is the visible frame; any invisible border the OS
        // keeps around it sits outside the spot (D25).
        let (l, t, r, b) = n8tray_platform::window::frame_insets(hwnd).unwrap_or((0, 0, 0, 0));
        n8tray_platform::window::set_rect(
            hwnd,
            spot.x - l,
            spot.y - t,
            width + l + r,
            height + t + b,
        );
        if log {
            info!(
                "The window opens at {},{} ({width}x{height}, scale {scale}) by the {:?} taskbar; icon at {},{} ({}x{}), work area {},{} ({}x{})",
                spot.x,
                spot.y,
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
}

fn create() -> Result<HubWindow, slint::PlatformError> {
    let started = Instant::now();
    let ui = HubWindow::new()?;
    // Slint creates the native window here, off screen and not activated
    // (the attributes in `main.rs`); `adopt` hides it right away.
    ui.show()?;
    info!(
        "The window was created in {} ms",
        started.elapsed().as_millis()
    );
    Ok(ui)
}

fn level_name(level: LevelName) -> &'static str {
    match level {
        LevelName::Hidden => "hidden",
        LevelName::Off => "off",
        LevelName::Ok => "ok",
        LevelName::Attention => "attention",
        LevelName::Problem => "problem",
    }
}

/// The placeholder's text: the level, the headline and each app.
pub fn lines(snapshot: &HostSnapshot) -> (String, String, Vec<(String, String)>) {
    let apps = snapshot
        .apps
        .iter()
        .filter(|a| a.hosted && a.level != LevelName::Hidden)
        .map(|a| (a.name.clone(), a.pending.clone().unwrap_or(a.words.clone())))
        .collect();
    (
        level_name(snapshot.level).into(),
        snapshot.headline.clone(),
        apps,
    )
}

fn fill(ui: &HubWindow, snapshot: Option<&HostSnapshot>) {
    let Some(snapshot) = snapshot else {
        ui.set_level("off".into());
        ui.set_headline("Starting".into());
        ui.set_apps(ModelRc::default());
        return;
    };
    let (level, headline, apps) = lines(snapshot);
    ui.set_level(level.into());
    ui.set_headline(headline.into());
    let apps: Vec<AppLine> = apps
        .into_iter()
        .map(|(name, words)| AppLine {
            name: name.into(),
            words: words.into(),
        })
        .collect();
    ui.set_apps(ModelRc::from(Rc::new(VecModel::from(apps))));
}

/// `N8TRAY_THEME=light|dark`, else the system's app theme as the window
/// reports it, else dark.
fn dark(ui: &HubWindow) -> bool {
    use slint::winit_030::WinitWindowAccessor;
    use slint::winit_030::winit::window::Theme;
    match std::env::var("N8TRAY_THEME").as_deref() {
        Ok("light") => return false,
        Ok("dark") => return true,
        _ => {}
    }
    ui.window()
        .with_winit_window(|w| w.theme())
        .flatten()
        .is_none_or(|t| t == Theme::Dark)
}

/// Focus loss, Escape, a close request and a scale change. Each is handled
/// on a later turn of the event loop: Win32 sends some of them while the
/// shell is busy showing or hiding the window.
fn listen(ui: &HubWindow) {
    use slint::winit_030::winit::event::{ElementState, WindowEvent};
    use slint::winit_030::winit::keyboard::{Key, NamedKey};
    use slint::winit_030::{EventResult, WinitWindowAccessor};
    ui.window().on_winit_window_event(|_, event| match event {
        WindowEvent::Focused(false) => {
            crate::shell::defer(|s| s.hub.blurred());
            EventResult::Propagate
        }
        WindowEvent::KeyboardInput { event, .. }
            if event.state == ElementState::Pressed
                && event.logical_key == Key::Named(NamedKey::Escape) =>
        {
            crate::shell::defer(|s| s.hub.hide(false));
            EventResult::PreventDefault
        }
        WindowEvent::CloseRequested => {
            crate::shell::defer(|s| s.hub.hide(false));
            EventResult::PreventDefault
        }
        WindowEvent::ScaleFactorChanged { .. } => {
            crate::shell::defer(|s| s.hub.rescaled());
            EventResult::Propagate
        }
        _ => EventResult::Propagate,
    });
}

/// The native window's handle (an `HWND` on Windows).
fn native_handle(ui: &HubWindow) -> Option<isize> {
    use slint::winit_030::WinitWindowAccessor;
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    ui.window()
        .with_winit_window(|w| match w.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
            _ => None,
        })
        .flatten()
}

/// `N8TRAY_ANCHOR=x,y,w,h[;x,y,w,h]`: a tray icon rectangle, and optionally
/// a work area, in physical pixels, instead of the real ones. For testing
/// placement on taskbar edges the PC does not have, and for measuring.
pub fn test_anchor(value: Option<&str>) -> Option<(Rect, Option<Rect>)> {
    let rect = |text: &str| -> Option<Rect> {
        let n: Vec<i32> = text
            .split(',')
            .filter_map(|v| v.trim().parse().ok())
            .collect();
        (n.len() == 4).then(|| Rect::new(n[0], n[1], n[2], n[3]))
    };
    let mut parts = value?.split(';');
    let anchor = rect(parts.next()?)?;
    Some((anchor, parts.next().and_then(rect)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn a_test_anchor_names_an_icon_and_maybe_a_work_area() {
        assert_eq!(test_anchor(None), None);
        assert_eq!(
            test_anchor(Some("2400,1400,24,40")),
            Some((Rect::new(2400, 1400, 24, 40), None))
        );
        assert_eq!(
            test_anchor(Some("2400, 1400, 24, 40;0,0,2560,1392")),
            Some((
                Rect::new(2400, 1400, 24, 40),
                Some(Rect::new(0, 0, 2560, 1392))
            ))
        );
        assert_eq!(test_anchor(Some("1,2,3")), None);
        assert_eq!(test_anchor(Some("a,b,c,d")), None);
        assert_eq!(
            test_anchor(Some("1,2,3,4;nonsense")),
            Some((Rect::new(1, 2, 3, 4), None))
        );
    }

    #[test]
    fn the_placeholder_shows_the_level_headline_and_hosted_apps() {
        let s = fixture("trouble");
        let (level, headline, apps) = lines(&s);
        assert_eq!(level, level_name(s.level));
        assert_eq!(headline, s.headline);
        let hosted: Vec<_> = s
            .apps
            .iter()
            .filter(|a| a.hosted && a.level != LevelName::Hidden)
            .map(|a| a.name.clone())
            .collect();
        assert_eq!(
            apps.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
            hosted
        );
    }

    #[test]
    fn keeping_the_window_is_only_for_measuring() {
        // The default drops it (D17).
        if std::env::var_os("N8TRAY_WINDOW").is_none() {
            assert_eq!(WhenHidden::from_env(), WhenHidden::Drop);
        }
    }
}
