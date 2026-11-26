//! One shell on the UI thread: the host's handle and latest snapshot, the
//! tray icon and the window. Everything that happens elsewhere (host
//! events, the taskbar's colour, tray and menu clicks, requests from a later
//! copy) is posted to the UI thread with `slint::invoke_from_event_loop`
//! and handled here, one at a time.

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use n8tray_core::folders::Folders;
use n8tray_core::host::{Command, Event, HostHandle};
use n8tray_core::model::HostSnapshot;
use n8tray_core::placement::Rect;
use n8tray_platform::single::Request;
use n8tray_platform::theme::{self, Taskbar};
use tracing::{info, warn};

use crate::commands::{self, Opened};
use crate::tray::{Clicked, Tray};
use crate::window::{Hub, WhenHidden};

/// Start-up touches far more memory than an idle tray keeps (D19).
const TRIM_AFTER_START: Duration = Duration::from_secs(30);

thread_local! {
    static SHELL: RefCell<Option<Shell>> = const { RefCell::new(None) };
}

/// Runs `f` on the shell, if there is one. Called only at the top of an
/// event loop turn (a posted closure or a timer), so it is never re-entered.
pub fn with(f: impl FnOnce(&mut Shell)) {
    SHELL.with(|cell| match cell.try_borrow_mut() {
        Ok(mut shell) => {
            if let Some(shell) = shell.as_mut() {
                f(shell);
            }
        }
        Err(_) => warn!("The shell was busy; a request was dropped"),
    });
}

/// Runs `f` on the shell on a later turn of the event loop, from any thread.
pub fn defer(f: impl FnOnce(&mut Shell) + Send + 'static) {
    if slint::invoke_from_event_loop(move || with(f)).is_err() {
        // The event loop is gone: N8 Tray is exiting.
    }
}

pub struct Shell {
    folders: Folders,
    host: Option<HostHandle>,
    latest: Option<Arc<HostSnapshot>>,
    tray: Option<Tray>,
    taskbar: Taskbar,
    pub hub: Hub,
    startup_trim: slint::Timer,
}

/// Puts a new shell on this thread.
pub fn install(folders: Folders) {
    let shell = Shell {
        folders,
        host: None,
        latest: None,
        tray: None,
        // Where the platform cannot say, the art for a dark taskbar.
        taskbar: theme::taskbar().unwrap_or_default(),
        hub: Hub::new(WhenHidden::from_env()),
        startup_trim: slint::Timer::default(),
    };
    SHELL.with(|cell| *cell.borrow_mut() = Some(shell));
}

/// Takes the shell off this thread, once the event loop has ended.
pub fn take() -> Option<Shell> {
    SHELL.with(|cell| cell.borrow_mut().take())
}

impl Shell {
    pub fn set_host(&mut self, host: HostHandle) {
        self.hub.set_host(host.sender());
        self.host = Some(host);
        self.startup_trim
            .start(slint::TimerMode::SingleShot, TRIM_AFTER_START, || {
                n8tray_platform::memory::trim_own_working_set();
            });
    }

    fn send(&self, command: Command) {
        if let Some(host) = &self.host {
            host.send(command);
        }
    }

    /// The tray icon, its menu and its events, and the taskbar watch.
    pub fn create_tray(&mut self) {
        n8tray_platform::window::allow_dark_menus();
        match Tray::create(self.taskbar) {
            Ok(mut tray) => {
                if let Some(snapshot) = &self.latest {
                    tray.update(snapshot);
                }
                self.tray = Some(tray);
            }
            Err(e) => warn!("The tray icon could not be created: {e}"),
        }
        tray_icon::TrayIconEvent::set_event_handler(Some(|event| {
            if let tray_icon::TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                let rect = crate::tray::click_rect(rect);
                defer(move |s| s.toggle(Some(rect)));
            }
        }));
        tray_icon::menu::MenuEvent::set_event_handler(Some(|event: tray_icon::menu::MenuEvent| {
            let id = event.id.0;
            defer(move |s| s.on_menu(&id));
        }));
        let taskbar = self.taskbar;
        let watch = theme::watch_taskbar(|report| match report {
            Ok(taskbar) => defer(move |s| s.set_taskbar(taskbar)),
            Err(e) => warn!("The tray icon stopped following the taskbar's colour: {e}"),
        });
        match watch {
            Ok(true) => info!("The tray icon follows the taskbar's colour ({taskbar:?} now)"),
            Ok(false) => info!("The tray icon uses the art for a {taskbar:?} taskbar"),
            Err(e) => warn!("The tray icon cannot follow the taskbar's colour: {e}"),
        }
    }

    fn set_taskbar(&mut self, taskbar: Taskbar) {
        self.taskbar = taskbar;
        if let Some(tray) = &mut self.tray {
            tray.set_taskbar(taskbar);
        }
    }

    pub fn on_event(&mut self, event: Event) {
        match event {
            Event::Snapshot(snapshot) => {
                if let Some(tray) = &mut self.tray {
                    tray.update(&snapshot);
                }
                self.hub.refresh(&snapshot);
                self.latest = Some(snapshot);
            }
            Event::Notify { title, body } => {
                // Toasts come with a later lane; the Tauri app shows them
                // through its notification plugin.
                info!("Notification (not shown by the native shell yet): {title}: {body}");
            }
        }
    }

    /// A request from a later copy of N8 Tray.
    pub fn on_request(&mut self, request: Request) {
        match request {
            Request::Show => self.show(None),
            Request::Hide => self.hub.hide(false),
            Request::Exit => {
                info!("Asked to exit by another copy of N8 Tray");
                self.exit();
            }
        }
    }

    /// Opens the window by the tray icon.
    pub fn show(&mut self, anchor: Option<Rect>) {
        let anchor = anchor.or_else(|| self.tray.as_ref().and_then(Tray::rect));
        self.hub.show(anchor, self.latest.as_deref());
    }

    fn toggle(&mut self, anchor: Option<Rect>) {
        self.hub.toggle(anchor, self.latest.as_deref());
    }

    fn on_menu(&mut self, id: &str) {
        let snapshot = self.latest.clone();
        match crate::tray::clicked(id) {
            // The details view comes with a later lane.
            Clicked::Status => self.show(None),
            Clicked::SignIn => {
                let on = snapshot
                    .as_ref()
                    .is_some_and(|s| !s.settings.launch_at_sign_in);
                self.send(Command::SetLaunchAtSignIn(on));
            }
            Clicked::Settings => self.open_host("settings"),
            Clicked::Folder => self.open_host("folder"),
            Clicked::Log => self.open_host("log"),
            Clicked::Update => {
                // Installing in place is the Tauri updater's (D27) until a
                // later lane: here Install opens the release page.
                match commands::open_update(snapshot.as_deref()) {
                    Ok(url) => crate::open::open(&url),
                    Err(e) => warn!("Install update: {e}"),
                }
            }
            Clicked::Restart => self.restart(),
            Clicked::Exit => {
                info!("Exit from the tray menu; apps keep running");
                self.exit();
            }
            Clicked::Action { app, action } => {
                self.send(commands::module_action(app, action, None));
            }
            Clicked::Open { app, kind, key } => {
                match commands::open_item(snapshot.as_deref(), &app, &kind, &key) {
                    Ok(Opened::Target(target)) => crate::open::open(&target),
                    Ok(Opened::Command(command)) => self.send(command),
                    Err(e) => warn!("Cannot open {kind} {key} of {app}: {e}"),
                }
            }
            Clicked::Nothing => {}
        }
    }

    fn open_host(&self, target: &str) {
        match commands::open_host(&self.folders, target) {
            Ok(path) => crate::open::open(&path.display().to_string()),
            Err(e) => warn!("Cannot open {target}: {e}"),
        }
    }

    /// Stops the host (apps keep running) and ends the event loop.
    pub fn exit(&mut self) {
        self.shutdown();
        let _ = slint::quit_event_loop();
    }

    /// Stops the host; safe to call twice.
    pub fn shutdown(&mut self) {
        if let Some(host) = self.host.take() {
            host.shutdown();
        }
    }

    /// Starts a new copy that waits for this one, then exits.
    fn restart(&mut self) {
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => {
                warn!("Cannot restart: {e}");
                return;
            }
        };
        info!("Restarting N8 Tray");
        self.shutdown();
        let spawned = std::process::Command::new(exe)
            .args(["--restart-after", &std::process::id().to_string()])
            .spawn();
        match spawned {
            Ok(_) => {
                let _ = slint::quit_event_loop();
            }
            Err(e) => warn!("Cannot restart: {e}"),
        }
    }

    /// Takes the tray icon out of the notification area and drops the window.
    pub fn close(mut self) {
        self.shutdown();
        self.hub.close();
        drop(self.tray.take());
    }
}
