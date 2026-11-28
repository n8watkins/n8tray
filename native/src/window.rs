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

use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use n8tray_core::host::Command;
use n8tray_core::model::{ActionView, AppView, HostSnapshot};
use n8tray_core::placement::{self, Rect};
use slint::ComponentHandle;
use tracing::{info, warn};

use crate::life::{Life, Step, WhenHidden};
use crate::ui::{FlyoutWindow, Nav, RowFocus, Theme, View};
use crate::view::confirm::{self, Press};
use crate::view::router::{self, Escape, Models, Router};

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
    ui: Option<FlyoutWindow>,
    hwnd: Option<isize>,
    /// The tray icon's rectangle, in physical pixels.
    anchor: Option<Rect>,
    wait: slint::Timer,
    retire: slint::Timer,
    trim: slint::Timer,
    /// Fires when an armed action stops waiting for its second click.
    disarm: slint::Timer,
    host: Option<Sender<Command>>,
    /// Which view shows, the armed actions and the icons.
    router: Router,
    /// The snapshot the window shows.
    latest: Option<HostSnapshot>,
    /// The window opened on a view: put the focus at its top once shown.
    pending_focus: bool,
    /// Where the window was last put (`set_rect`'s arguments), so a refresh
    /// that changes nothing moves nothing.
    placed: Option<(i32, i32, i32, i32)>,
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
            disarm: slint::Timer::default(),
            host: None,
            router: Router::default(),
            latest: None,
            pending_focus: false,
            placed: None,
        }
    }

    pub fn set_host(&mut self, host: Sender<Command>) {
        self.host = Some(host);
    }

    /// Fills the view that shows from the latest snapshot, with Compact and
    /// the height limit as the snapshot's settings say. When the view's app
    /// left the snapshot it falls back to the hub, and the focus moves to
    /// the hub's top.
    fn fill(&mut self) {
        let (Some(ui), Some(snapshot)) = (&self.ui, &self.latest) else {
            return;
        };
        crate::view::theme::apply_settings(&ui.global::<Theme>(), &snapshot.settings);
        if self.router.fill(ui, snapshot) {
            arrived(ui);
        }
    }

    /// The native window's handle, while it exists.
    pub fn native(&self) -> Option<isize> {
        self.hwnd
    }

    /// Runs `f` with the window's models, if the window exists (a reply to
    /// a view's request, `view::intent`).
    pub fn with_models(&self, f: impl FnOnce(&dyn Models)) {
        if let Some(ui) = &self.ui {
            f(ui);
        }
    }

    /// After the router changed the view: fill it, put the focus at its top,
    /// stop waiting for a second click, and fit the window to it.
    fn navigated(&mut self) {
        self.disarm.stop();
        self.fill();
        if let Some(ui) = &self.ui {
            arrived(ui);
        }
        self.place(Place::Fit);
    }

    /// A view asked for another (`Nav.go`).
    pub fn go(&mut self, view: View, id: &str) {
        if self.router.go(view, id) {
            self.navigated();
        }
    }

    /// Back (`Nav.back`).
    pub fn back(&mut self) {
        self.router.back();
        self.navigated();
    }

    /// Escape: back, or in the hub hide the window.
    pub fn escape(&mut self) {
        match self.router.escape() {
            Escape::Hide => self.hide(false),
            Escape::Back => self.navigated(),
        }
    }

    /// A click on an action of an app (or of one of its components): it
    /// runs, or it is armed for a second click when it asks to confirm. The
    /// armed state is the router's, by app, action and component, so it
    /// survives a refresh or a reordered list, and is shared by every view
    /// until the view changes or the window hides. Views reach it through
    /// `view::intent::send(Intent::Action { .. })`.
    pub fn press_action(&mut self, app: &str, action: &str, component: Option<&str>) {
        let Some(found_app) = self
            .latest
            .as_ref()
            .and_then(|s| s.apps.iter().find(|a| a.id == app))
        else {
            warn!("No app {app}");
            return;
        };
        let Some(found) = found_app.actions.iter().find(|x| x.id == action) else {
            warn!("No action {action} for {app}");
            return;
        };
        // As the button shows it (`view::action::action_ui`): an action that
        // is not possible now, or any action while the app is busy with
        // another, does nothing, even from a stale click or a key.
        if let Err(why) = pressable(found_app, found) {
            info!("{action} of {app} not run: {why}");
            return;
        }
        let now = Instant::now();
        let key = confirm::key(app, action, component);
        match self.router.confirms.press(key, found.confirm, now) {
            Press::Run => {
                if let Some(host) = &self.host {
                    let _ = host.send(crate::commands::module_action(
                        app.into(),
                        action.into(),
                        component.map(String::from),
                    ));
                }
            }
            Press::Armed => {}
        }
        self.fill();
        self.arm_disarm(now);
    }

    /// Fills again when the next armed action stops waiting.
    fn arm_disarm(&mut self, now: Instant) {
        if let Some(at) = self.router.confirms.next_expiry() {
            self.disarm.start(
                slint::TimerMode::SingleShot,
                at.saturating_duration_since(now) + Duration::from_millis(10),
                || {
                    crate::shell::with(|s| {
                        if s.hub.router.confirms.expire(Instant::now()) {
                            s.hub.fill();
                        }
                        s.hub.arm_disarm(Instant::now());
                    })
                },
            );
        }
    }

    /// Testing on a PC someone uses: shown without taking the focus.
    fn activate() -> bool {
        std::env::var_os("N8TRAY_NO_ACTIVATE").is_none()
    }

    /// A left click on the tray icon: open on the hub, or close when open.
    pub fn toggle(&mut self, anchor: Option<Rect>, snapshot: Option<&HostSnapshot>) {
        if anchor.is_some() {
            self.anchor = anchor;
        }
        let steps = self.life.toggle(Instant::now(), Self::activate());
        if steps.contains(&Step::Fill) {
            // Opening: every open shows the hub, as today (`hub.rs`).
            self.opening(View::Hub, "");
        }
        self.run(steps, snapshot);
        self.reveal();
    }

    /// Opens the window next to the tray icon (`anchor`, or where it was) on
    /// `view`, with nothing to go back to but the hub; shown again while
    /// open, it moves to `view` as today's window does.
    pub fn show_view(
        &mut self,
        view: View,
        id: &str,
        anchor: Option<Rect>,
        snapshot: Option<&HostSnapshot>,
    ) {
        if anchor.is_some() {
            self.anchor = anchor;
        }
        let steps = self.life.show(Self::activate());
        self.opening(view, id);
        self.run(steps, snapshot);
        self.reveal();
    }

    /// The window opens (or is shown again) on `view`.
    fn opening(&mut self, view: View, id: &str) {
        self.disarm.stop();
        self.router.open_at(view, id);
        self.pending_focus = true;
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

    /// Hides the window. Nothing stays armed: a second click after the
    /// window opens again never runs an action armed before.
    pub fn hide(&mut self, from_blur: bool) {
        let steps = self.life.hide(Instant::now(), from_blur);
        self.run(steps, None);
        self.disarm.stop();
        if self.router.confirms.next_expiry().is_some() {
            self.router.confirms.clear();
            self.fill();
        }
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
        self.latest = Some(snapshot.clone());
        if self.ui.is_some() && self.life.visible() {
            self.fill();
            self.place(Place::Fit);
        }
    }

    /// The display scale changed under a shown window.
    pub fn rescaled(&mut self) {
        if self.life.visible() {
            self.place(Place::Again);
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
                        self.router.new_window();
                        router::wire(&ui);
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
                    if let Some(snapshot) = snapshot {
                        self.latest = Some(snapshot.clone());
                    }
                    self.fill();
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
                    self.placed = None;
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
        // The theme and High Contrast as they are now, and Compact and the
        // height limit as the latest snapshot says, on every reveal.
        if let Some(ui) = &self.ui {
            let theme = ui.global::<Theme>();
            crate::view::theme::apply(&theme, &crate::view::theme::for_window(ui.window()));
            if let Some(snapshot) = &self.latest {
                crate::view::theme::apply_settings(&theme, &snapshot.settings);
            }
        }
        self.place(Place::Reveal);
        if activate {
            n8tray_platform::window::show_active(hwnd);
        } else {
            n8tray_platform::window::show_inactive(hwnd);
        }
        if let Some(ui) = &self.ui {
            if std::mem::take(&mut self.pending_focus) {
                arrived(ui);
            }
            ui.window().request_redraw();
        }
        // Moving between monitors of different scale can resize the window
        // behind our back: place it once more now that it is shown.
        self.place(Place::Again);
    }

    /// The native window exists: hide it at once (Slint showed it off
    /// screen), style it as a flyout and listen to it.
    fn adopt(&mut self, hwnd: isize) {
        n8tray_platform::window::hide(hwnd);
        n8tray_platform::window::style_flyout(hwnd);
        self.hwnd = Some(hwnd);
        self.life.native_found();
        let Some(ui) = &self.ui else { return };
        crate::view::theme::apply(
            &ui.global::<Theme>(),
            &crate::view::theme::for_window(ui.window()),
        );
        listen(ui);
        // D25: a frameless Tauri window kept 7 px invisible borders; say what
        // this one keeps, so the placement code path stays honest.
        let insets = n8tray_platform::window::frame_insets(hwnd);
        info!("The window's frame insets (left, top, right, bottom): {insets:?}");
    }

    /// Sizes the window to its content and places it next to the icon,
    /// inside the work area of the icon's monitor. A refresh that changes
    /// nothing moves nothing (`Place::Fit`).
    ///
    /// The top and the height change in one `SetWindowPos`, so a window by a
    /// bottom taskbar keeps its bottom on the taskbar as its content grows or
    /// shrinks; whether a frame at the old size shows in between is only
    /// visible on a real desktop (D36).
    fn place(&mut self, how: Place) {
        let log = how == Place::Reveal;
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
        // As tall as the view wants, up to the height-limit setting; the
        // view's scroll area takes the rest.
        let limit = f64::from(ui.global::<Theme>().get_height_limit());
        let content = f64::from(ui.get_content_height()).clamp(80.0, limit.max(80.0));
        let width = (WIDTH * scale).round() as i32;
        let wanted = (content * scale).round() as i32;
        let room = placement::place(width, wanted, anchor, screen, work, placement::GAP).max_height;
        let height = wanted.min(room);
        let spot = placement::place(width, height, anchor, screen, work, placement::GAP);
        // What users see is the visible frame; any invisible border the OS
        // keeps around it sits outside the spot (D25).
        let (l, t, r, b) = n8tray_platform::window::frame_insets(hwnd).unwrap_or((0, 0, 0, 0));
        let frame = (spot.x - l, spot.y - t, width + l + r, height + t + b);
        if how != Place::Fit || self.placed != Some(frame) {
            n8tray_platform::window::set_rect(hwnd, frame.0, frame.1, frame.2, frame.3);
            self.placed = Some(frame);
        }
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

/// Whether an app's action may run now: it is possible (`enabled`) and the
/// app is not busy with another (`pending`), as its button says.
fn pressable(app: &AppView, action: &ActionView) -> Result<(), &'static str> {
    if app.pending.is_some() {
        Err("the app is busy")
    } else if !action.enabled {
        Err("not possible now")
    } else {
        Ok(())
    }
}

/// Why the window is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// It is about to show: placed, and the spot logged.
    Reveal,
    /// Placed again even where it was (the OS may have resized it).
    Again,
    /// Its content may have changed: moved only if the spot did.
    Fit,
}

/// A new view shows: no row waits for the focus any more, and the focus is
/// at the view's top, where a screen reader says the view's title.
pub fn arrived(ui: &FlyoutWindow) {
    ui.global::<RowFocus>().invoke_cancel();
    ui.invoke_focus_top();
}

fn create() -> Result<FlyoutWindow, slint::PlatformError> {
    let started = Instant::now();
    let ui = FlyoutWindow::new()?;
    // Navigation, on a later turn: the views ask while Slint handles a click.
    let nav = ui.global::<Nav>();
    nav.on_go(|view, id| crate::shell::defer(move |s| s.hub.go(view, &id)));
    nav.on_back(|| crate::shell::defer(|s| s.hub.back()));
    nav.on_close(|| crate::shell::defer(|s| s.hub.hide(false)));
    // Slint creates the native window here, off screen and not activated
    // (the attributes in `main.rs`); `adopt` hides it right away.
    ui.show()?;
    info!(
        "The window was created in {} ms",
        started.elapsed().as_millis()
    );
    Ok(ui)
}

/// Focus loss, Escape, a close request and a scale change. Each is handled
/// on a later turn of the event loop: Win32 sends some of them while the
/// shell is busy showing or hiding the window.
fn listen(ui: &FlyoutWindow) {
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
            // Back, or in the hub hide the window (today's Escape).
            crate::shell::defer(|s| s.hub.escape());
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
fn native_handle(ui: &FlyoutWindow) -> Option<isize> {
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

    /// The shell's window on the headless platform, filled by a router.
    fn headless_window() -> (FlyoutWindow, Router, HostSnapshot) {
        crate::view::gallery::headless().unwrap();
        let ui = FlyoutWindow::new().unwrap();
        let snapshot = fixture("busy");
        let mut router = Router::default();
        router.fill(&ui, &snapshot);
        (ui, router, snapshot)
    }

    /// As today's `go` (`main.tsx`): a new view puts the keyboard focus at
    /// its top, where a screen reader hears the view's title, and Tab goes
    /// on from there to the view's controls.
    #[test]
    fn a_new_view_has_the_focus_at_its_top() {
        let (ui, mut router, snapshot) = headless_window();
        ui.show().unwrap();
        assert!(!ui.get_top_focused());
        assert!(router.go(View::Settings, ""));
        router.fill(&ui, &snapshot);
        arrived(&ui);
        assert!(ui.get_top_focused());
        assert_eq!(ui.global::<Nav>().get_title(), "Settings");
        assert_eq!(ui.global::<Nav>().get_view(), View::Settings);
        let tab: slint::SharedString = slint::platform::Key::Tab.into();
        ui.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: tab.clone() });
        ui.window()
            .dispatch_event(slint::platform::WindowEvent::KeyReleased { text: tab });
        assert!(!ui.get_top_focused(), "Tab goes on to the first control");
        // A request for a row no row took is dropped with the view.
        ui.global::<RowFocus>().invoke_to_id("voicedock".into(), 0);
        arrived(&ui);
        assert!(!ui.global::<RowFocus>().get_pending());
        ui.hide().unwrap();
    }

    /// A hub with no window yet, its host a channel the test reads.
    fn hub_with(snapshot: &HostSnapshot) -> (Hub, std::sync::mpsc::Receiver<Command>) {
        crate::view::gallery::headless().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut hub = Hub::new(WhenHidden::Drop);
        hub.set_host(tx);
        hub.latest = Some(snapshot.clone());
        (hub, rx)
    }

    /// An app with Stop, which asks to confirm.
    fn confirming_stop(s: &mut HostSnapshot) -> (String, String) {
        let app = s
            .apps
            .iter_mut()
            .find(|a| crate::view::format::row_action(a).is_some())
            .expect("an app with an action");
        app.pending = None;
        let id = app.id.clone();
        let action = crate::view::format::row_action(app).unwrap().id.clone();
        for a in &mut app.actions {
            if a.id == action {
                a.enabled = true;
                a.confirm = true;
            }
        }
        (id, action)
    }

    /// Hiding the window disarms: a second press after it opens again
    /// arms again, it does not run what was armed before.
    #[test]
    fn hiding_clears_armed_actions() {
        let mut s = fixture("busy");
        let (app, action) = confirming_stop(&mut s);
        let (mut hub, rx) = hub_with(&s);
        hub.press_action(&app, &action, None);
        let key = confirm::key(&app, &action, None);
        assert!(hub.router.confirms.is_armed(&key, Instant::now()));
        hub.hide(false);
        assert!(!hub.router.confirms.is_armed(&key, Instant::now()));
        hub.press_action(&app, &action, None);
        assert!(rx.try_recv().is_err(), "armed again, not run");
    }

    /// A press on an action that is not possible now, or while the app is
    /// busy, sends nothing and arms nothing.
    #[test]
    fn a_disabled_action_or_a_busy_app_runs_nothing() {
        let mut s = fixture("busy");
        let (app, action) = confirming_stop(&mut s);
        let key = confirm::key(&app, &action, None);
        let set = |s: &mut HostSnapshot, enabled: bool, pending: Option<&str>| {
            let a = s.apps.iter_mut().find(|a| a.id == app).unwrap();
            a.pending = pending.map(String::from);
            for x in &mut a.actions {
                x.confirm = false;
                if x.id == action {
                    x.enabled = enabled;
                }
            }
        };
        let mut disabled = s.clone();
        set(&mut disabled, false, None);
        let mut busy = s.clone();
        set(&mut busy, true, Some("Stopping"));
        for snapshot in [disabled, busy] {
            let (mut hub, rx) = hub_with(&snapshot);
            hub.press_action(&app, &action, None);
            assert!(rx.try_recv().is_err());
            assert!(!hub.router.confirms.is_armed(&key, Instant::now()));
        }
        let mut fine = s.clone();
        set(&mut fine, true, None);
        let (mut hub, rx) = hub_with(&fine);
        hub.press_action(&app, &action, None);
        assert!(matches!(rx.try_recv(), Ok(Command::Action { .. })));
    }

    /// Compact and the window height come from the snapshot's settings
    /// (D35), and say when they changed.
    #[test]
    fn compact_and_the_height_limit_follow_the_settings() {
        let (ui, _router, snapshot) = headless_window();
        let theme = ui.global::<Theme>();
        let mut settings = snapshot.settings.clone();
        settings.compact = false;
        settings.window_height = "short".into();
        settings.window_height_px = 400;
        crate::view::theme::apply_settings(&theme, &settings);
        assert!(!crate::view::theme::apply_settings(&theme, &settings));
        settings.compact = true;
        settings.window_height = "tall".into();
        settings.window_height_px = 600;
        assert!(crate::view::theme::apply_settings(&theme, &settings));
        assert!(theme.get_compact());
        assert_eq!(theme.get_height_limit(), 600.0);
        // The fixture with Compact on and the tall window.
        let tall = fixture("compactTall");
        assert!(tall.settings.compact);
        crate::view::theme::apply_settings(&theme, &tall.settings);
        assert_eq!(
            theme.get_height_limit(),
            tall.settings.window_height_px as f32
        );
    }

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
}
