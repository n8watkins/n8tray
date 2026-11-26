//! The window, in lane N0 a placeholder: created on demand off screen and
//! without activation, placed 2 px from the tray icon by
//! `n8tray_core::placement` inside the work area of the icon's monitor,
//! shown with or without activation as today, hidden when it loses focus or
//! on Escape, and dropped 10 s after it was hidden, then the working set
//! trimmed (D17). The rules are `crate::life`'s; this carries them out.
//!
//! Slint re-activates a window every time it shows it, so after the first
//! show (off screen, never activated) the window is shown and hidden only
//! through Win32 (`n8tray_platform::window`); Slint keeps thinking it is
//! visible until it is dropped.
#![forbid(unsafe_code)]

use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use n8tray_core::host::Command;
use n8tray_core::model::{HostSnapshot, LevelName};
use n8tray_core::placement::{self, Rect};
use slint::{ComponentHandle, ModelRc, VecModel};
use tracing::{info, warn};

use crate::life::{Life, Step, WhenHidden};
use crate::ui::{AppLine, HubWindow};

/// How long a hidden window is kept before it is dropped (D17).
pub const KEEP_HIDDEN: Duration = Duration::from_secs(10);
/// The window's width in logical pixels; its height follows its content.
const WIDTH: f64 = 380.0;
/// After a drop, how long to wait before trimming (what the window left
/// behind is freed by then).
const TRIM_AFTER_DROP: Duration = Duration::from_secs(2);
/// Between two looks for the native window.
const WAIT_TURN: Duration = Duration::from_millis(10);

pub fn when_hidden_from_env() -> WhenHidden {
    WhenHidden::parse(std::env::var("N8TRAY_WINDOW").ok().as_deref())
}

pub struct Hub {
    life: Life,
    ui: Option<HubWindow>,
    hwnd: Option<isize>,
    /// The tray icon's rectangle, in physical pixels.
    anchor: Option<Rect>,
    wait: slint::Timer,
    retire: slint::Timer,
    trim: slint::Timer,
    host: Option<Sender<Command>>,
}

impl Hub {
    pub fn new(when_hidden: WhenHidden) -> Hub {
        Hub {
            life: Life::new(when_hidden),
            ui: None,
            hwnd: None,
            anchor: None,
            wait: slint::Timer::default(),
            retire: slint::Timer::default(),
            trim: slint::Timer::default(),
            host: None,
        }
    }

    pub fn set_host(&mut self, host: Sender<Command>) {
        self.host = Some(host);
    }

    /// Testing on a PC someone uses: shown without taking the focus.
    fn activate() -> bool {
        std::env::var_os("N8TRAY_NO_ACTIVATE").is_none()
    }

    /// A left click on the tray icon: open, or close when open.
    pub fn toggle(&mut self, anchor: Option<Rect>, snapshot: Option<&HostSnapshot>) {
        if anchor.is_some() {
            self.anchor = anchor;
        }
        let steps = self.life.toggle(Instant::now(), Self::activate());
        self.run(steps, snapshot);
        self.reveal();
    }

    /// Opens the window next to the tray icon (`anchor`, or where it was).
    pub fn show(&mut self, anchor: Option<Rect>, snapshot: Option<&HostSnapshot>) {
        if anchor.is_some() {
            self.anchor = anchor;
        }
        let steps = self.life.show(Self::activate());
        self.run(steps, snapshot);
        self.reveal();
    }

    /// Shows the window once its native window exists (Slint creates it
    /// during its first show; it may take an event loop turn).
    pub fn reveal(&mut self) {
        if self.hwnd.is_none()
            && let Some(hwnd) = self.ui.as_ref().and_then(native_handle)
        {
            self.adopt(hwnd);
        }
        let steps = self.life.reveal();
        self.run(steps, None);
    }

    pub fn hide(&mut self, from_blur: bool) {
        let steps = self.life.hide(Instant::now(), from_blur);
        self.run(steps, None);
    }

    /// The window lost the focus: a shown window hides (D25).
    pub fn blurred(&mut self) {
        let steps = self.life.blurred(Instant::now());
        self.run(steps, None);
    }

    fn retire(&mut self, generation: u64) {
        let steps = self.life.retire(generation);
        self.run(steps, None);
    }

    /// The snapshot changed while the window is open.
    pub fn refresh(&mut self, snapshot: &HostSnapshot) {
        if let (Some(ui), true) = (&self.ui, self.life.visible()) {
            fill(ui, Some(snapshot));
            self.place(false);
        }
    }

    /// The display scale changed under a shown window.
    pub fn rescaled(&mut self) {
        if self.life.visible() {
            self.place(false);
        }
    }

    /// Drops the window now (the shell is exiting).
    pub fn close(&mut self) {
        let steps = self.life.close();
        self.run(steps, None);
    }

    /// Carries out the life's steps, in order.
    fn run(&mut self, steps: Vec<Step>, snapshot: Option<&HostSnapshot>) {
        for step in steps {
            match step {
                Step::StopTimers => {
                    self.wait.stop();
                    self.retire.stop();
                    self.trim.stop();
                }
                Step::Create => match create() {
                    Ok(ui) => {
                        self.ui = Some(ui);
                        self.hwnd = None;
                    }
                    Err(e) => {
                        warn!("The window could not be created: {e}");
                        self.life.create_failed();
                        return;
                    }
                },
                Step::Fill => {
                    if let Some(ui) = &self.ui {
                        fill(ui, snapshot);
                    }
                }
                Step::WaitForNative => {
                    self.wait
                        .start(slint::TimerMode::SingleShot, WAIT_TURN, || {
                            crate::shell::with(|s| s.hub.reveal())
                        });
                }
                Step::Reveal { activate } => self.reveal_native(activate),
                Step::Hide => {
                    if let Some(hwnd) = self.hwnd {
                        n8tray_platform::window::hide(hwnd);
                    }
                }
                Step::Open(open) => {
                    if let Some(host) = &self.host {
                        let _ = host.send(Command::WindowOpen(open));
                    }
                }
                Step::StartRetire { generation } => {
                    self.retire
                        .start(slint::TimerMode::SingleShot, KEEP_HIDDEN, move || {
                            crate::shell::with(|s| s.hub.retire(generation))
                        });
                }
                Step::Drop => {
                    if let Some(ui) = self.ui.take() {
                        let _ = ui.hide();
                        drop(ui);
                        if self.hwnd.is_some() {
                            info!("The window was dropped to free its memory");
                        } else {
                            warn!("The window had no native window after 2 s; dropped");
                        }
                    }
                    self.hwnd = None;
                }
                Step::StartTrim => {
                    self.trim
                        .start(slint::TimerMode::SingleShot, TRIM_AFTER_DROP, || {
                            n8tray_platform::memory::trim_own_working_set();
                        });
                }
                Step::TrimNow => {
                    info!("The window was hidden for {KEEP_HIDDEN:?}: kept, memory trimmed");
                    n8tray_platform::memory::trim_own_working_set();
                }
            }
        }
    }

    fn reveal_native(&mut self, activate: bool) {
        let Some(hwnd) = self.hwnd else { return };
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
    }

    /// The native window exists: hide it at once (Slint showed it off
    /// screen), style it as a flyout and listen to it.
    fn adopt(&mut self, hwnd: isize) {
        n8tray_platform::window::hide(hwnd);
        n8tray_platform::window::style_flyout(hwnd);
        self.hwnd = Some(hwnd);
        self.life.native_found();
        let Some(ui) = &self.ui else { return };
        crate::view::theme::apply!(
            ui.global::<crate::ui::Theme>(),
            crate::ui::SystemColors,
            crate::view::theme::for_window(ui.window())
        );
        listen(ui);
        // D25: a frameless Tauri window kept 7 px invisible borders; say what
        // this one keeps, so the placement code path stays honest.
        let insets = n8tray_platform::window::frame_insets(hwnd);
        info!("The window's frame insets (left, top, right, bottom): {insets:?}");
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
        if !monitor.scale_from_monitor {
            warn!(
                "The monitor's own scale is unknown; placing at the system's ({})",
                monitor.scale
            );
        }
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
}
