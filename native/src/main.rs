//! N8 Tray's native shell (lane N0): the same host as the Tauri app, the
//! same tray icon and menu, and a Slint window with no WebView. It runs
//! next to the installed app as `n8tray-native.exe` until the switch, only
//! in a home of its own, with a single-instance name of its own
//! (docs/DECISIONS.md, D33).
//!
//! The crate denies unsafe code and each of its own modules forbids it;
//! only Slint's generated code (`ui`) may allow it.
#![windows_subsystem = "windows"]
// The shell runs on Windows only for now; elsewhere it is compiled and
// tested, not run.
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

mod args;
mod badge;
mod commands;
mod home;
mod life;
mod logs;
mod open;
mod recent;
mod shell;
mod tray;
mod view;
mod window;

/// Slint's generated code for `ui/main.slint`, in a module of its own.
mod ui {
    slint::include_modules!();
}

use std::time::{Duration, Instant};

use n8tray_core::folders::Folders;
use n8tray_core::host::{self, Options};
use n8tray_platform::single::{self, Instance, Request};
use tracing::{error, info, warn};

/// The renderer when `N8TRAY_RENDERER` names none (D33).
const DEFAULT_RENDERER: &str = "software";
/// The exit code when the home is not one of the native shell's own.
const EXIT_NOT_ISOLATED: i32 = 3;
/// The exit code after a panic on the UI thread.
const EXIT_PANIC: i32 = 101;

#[cfg(not(windows))]
fn main() {
    gallery();
    eprintln!("n8tray-native runs on Windows only for now; use the Tauri app (n8tray) here.");
    std::process::exit(2);
}

/// `--gallery` and `--gallery-png <folder>` (debug builds only): the widget
/// gallery instead of N8 Tray, which then exits (`view::gallery`). It needs
/// no home and touches nothing of N8 Tray's.
fn gallery() {
    #[cfg(debug_assertions)]
    {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if let Some(code) = view::gallery::from_args(&args) {
            std::process::exit(code);
        }
    }
}

#[cfg(windows)]
fn main() {
    gallery();
    let args = args::parse(std::env::args().skip(1));
    // Before anything is written, the log and `--import-settings` included.
    let home = match home::isolated_home(
        std::env::var_os("N8TRAY_HOME").as_deref(),
        home::real_base().as_deref(),
    ) {
        Ok(home) => home,
        Err(refusal) => {
            eprintln!("N8 Tray: {refusal}");
            std::process::exit(EXIT_NOT_ISOLATED);
        }
    };
    // Everything under that home, whatever N8TRAY_RUN_DIR and
    // N8TRAY_MODULES_DIR say: they could name the real folders.
    let folders = Folders::under(&home);
    let log_guard = logs::init(&folders.logs);
    for name in ["N8TRAY_RUN_DIR", "N8TRAY_MODULES_DIR"] {
        if std::env::var_os(name).is_some_and(|v| !v.is_empty()) {
            warn!("{name} is ignored: the native shell keeps everything under N8TRAY_HOME");
        }
    }
    // A panic on any thread goes to the log with where it happened (the app
    // has no console); the host thread then shows it and starts again.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        error!(
            "Panic on thread {}: {info}",
            thread.name().unwrap_or("unnamed")
        );
        default_hook(info);
    }));

    if let Some(source) = &args.import_settings {
        match n8tray_core::settings::import_powershell_file(source.as_ref(), &folders.settings_file)
        {
            Ok(imported) => {
                for note in imported.notes {
                    info!("Import: {note}");
                }
                info!("Imported {source} into {}", folders.settings_file.display());
            }
            Err(e) => {
                error!("Could not import {source}: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    if let Some(pid) = args.restart_after {
        wait_for_exit(pid, Duration::from_secs(15));
    }

    // `--exit` and `--hide` go to the running copy; with none running there
    // is nothing to do.
    let name = instance_name(&folders);
    if args.exit {
        single::send(&name, Request::Exit, Duration::from_secs(15));
        return;
    }
    if args.hide {
        single::send(&name, Request::Hide, Duration::ZERO);
        return;
    }

    if let Err(e) = select_backend() {
        error!("N8 Tray could not start its window system: {e}");
        std::process::exit(1);
    }
    // Taken once the backend is selected, so a request from a later copy
    // can be posted to the event loop from the start.
    let Some(instance) = Instance::acquire(&name, |request| {
        shell::defer(move |s| s.on_request(request));
    }) else {
        // Starting N8 Tray again opens its window.
        single::send(&name, Request::Show, Duration::ZERO);
        return;
    };
    if !instance.checked() {
        warn!("Could not tell whether another copy runs; starting as the first one");
    }

    shell::install(folders.clone());
    let mut options = Options::new(folders.clone());
    options.check_updates = true;
    // The native shell never touches the sign-in entry: the installed app
    // owns it until the switch (D33). `None` leaves it alone.
    options.autostart_command = None;
    let started = host::start(options, |event| shell::defer(move |s| s.on_event(event)));
    match started {
        Ok(host) => shell::with(|s| s.set_host(host)),
        Err(e) => {
            error!("N8 Tray cannot start: {e}");
            std::process::exit(1);
        }
    }
    // The tray icon needs the event loop; it is created on its first turn.
    shell::defer(|s| s.create_tray());
    if args.show {
        shell::defer(|s| s.show(None));
    }
    info!(
        "N8 Tray {} started (native shell, {} renderer, home {}){}",
        n8tray_core::VERSION,
        renderer(),
        home.display(),
        if args.autostart { " at sign-in" } else { "" }
    );

    // A panic on this thread (a bug in the shell) unwinds out of the event
    // loop to here, where nothing of the shell is borrowed any more: the
    // host is stopped and the tray icon removed as on a normal exit.
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        slint::run_event_loop_until_quit,
    ));
    let mut code = match ran {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            error!("The event loop ended with an error: {e}");
            1
        }
        Err(_) => {
            error!("The UI thread panicked; stopping the host and removing the tray icon");
            EXIT_PANIC
        }
    };
    // Take things apart here, while the other threads are alive: the host is
    // stopped and joined, the tray icon leaves the notification area, then
    // the single-instance name is released. Then leave without running the
    // thread-locals' destructors, which would do this during teardown.
    let closed = std::panic::catch_unwind(|| {
        shell::take().map(|shell| {
            let code = shell.exit_code();
            shell.close();
            code
        })
    });
    match closed {
        Ok(Some(shell_code)) if code == 0 => code = shell_code,
        Ok(_) => {}
        Err(_) => {
            error!("Closing the shell panicked too");
            code = EXIT_PANIC;
        }
    }
    drop(instance);
    info!("N8 Tray exited ({code})");
    drop(log_guard);
    std::process::exit(code);
}

/// `N8TRAY_RENDERER`: `software` or `skia-software`, for measuring. A
/// renderer this build lacks falls back to the default.
fn renderer() -> String {
    match std::env::var("N8TRAY_RENDERER") {
        Ok(name) if renderer_built(&name) => name,
        _ => DEFAULT_RENDERER.into(),
    }
}

/// Whether this build has the renderer (its Cargo feature).
#[allow(
    clippy::match_like_matches_macro,
    reason = "one line per renderer and its feature reads best"
)]
fn renderer_built(name: &str) -> bool {
    match name {
        "software" => cfg!(feature = "software"),
        "skia-software" => cfg!(feature = "skia-software"),
        _ => false,
    }
}

/// The winit backend with the chosen renderer. Every window is created off
/// screen, not activated, owned by a hidden window (so it never has a
/// taskbar button or an Alt-Tab entry, not even while it is created) and
/// without a frame; the shell then shows and places it through Win32.
fn select_backend() -> Result<(), slint::PlatformError> {
    #[cfg(windows)]
    let owner = n8tray_platform::window::create_owner();
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name(renderer())
        .with_winit_window_attributes_hook(move |attributes| {
            use slint::winit_030::winit::dpi::PhysicalPosition;
            let attributes = attributes
                .with_active(false)
                .with_resizable(false)
                .with_decorations(false)
                .with_position(PhysicalPosition::new(-32000, -32000));
            #[cfg(windows)]
            {
                use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
                let attributes = attributes.with_skip_taskbar(true);
                match owner {
                    Some(owner) => attributes.with_owner_window(owner),
                    None => attributes,
                }
            }
            #[cfg(not(windows))]
            attributes
        })
        .select()
}

/// The single-instance name: one per N8 Tray home, and never the installed
/// Tauri app's, so both run side by side and two test homes never meet.
pub fn instance_name(folders: &Folders) -> String {
    let key = folders
        .run
        .as_deref()
        .unwrap_or_else(|| folders.base())
        .to_string_lossy()
        .to_lowercase();
    format!("N8TrayNative-{:016x}", fnv1a(key.as_bytes()))
}

/// FNV-1a, 64 bits: a short, stable name from a path.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Waits for a process to exit (the copy that is restarting).
fn wait_for_exit(pid: u32, limit: Duration) {
    let end = Instant::now() + limit;
    let mut processes = n8tray_platform::process::Processes::new();
    while Instant::now() < end {
        let running = processes
            .snapshot()
            .map(|s| s.processes.iter().any(|p| p.pid == pid))
            .unwrap_or(false);
        if !running {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    warn!("The previous N8 Tray (pid {pid}) did not exit within {limit:?}");
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use n8tray_core::model::HostSnapshot;

    use super::*;

    /// A snapshot from `native/fixtures/<name>.json`.
    pub fn fixture(name: &str) -> HostSnapshot {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(format!("{name}.json"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"))
    }

    pub fn fixture_names() -> Vec<String> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|e| {
                e.file_name()
                    .to_str()?
                    .strip_suffix(".json")
                    .map(String::from)
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn every_fixture_reads() {
        let names = fixture_names();
        assert!(names.len() >= 8, "{names:?}");
        for name in names {
            fixture(&name);
        }
    }

    #[test]
    fn each_home_has_its_own_instance_name() {
        let a = instance_name(&Folders::under(Path::new("/tmp/n8-a")));
        let b = instance_name(&Folders::under(Path::new("/tmp/n8-b")));
        assert_ne!(a, b);
        assert_eq!(a, instance_name(&Folders::under(Path::new("/tmp/n8-a"))));
        // Windows paths are not case sensitive.
        assert_eq!(
            instance_name(&Folders::under(Path::new("/TMP/N8-A"))),
            instance_name(&Folders::under(Path::new("/tmp/n8-a")))
        );
        // The kernel objects are `Local\<name>-instance` and the like: a
        // name of its own, without the separator a kernel name may not hold.
        assert_eq!(a.len(), "N8TrayNative-".len() + 16, "{a}");
        assert!(!a.contains('\\') && !a.contains('/'));
    }

    /// tauri-plugin-single-instance 2.4 names its objects after the app's
    /// identifier (`com.n8watkins.n8tray-sim`, `-sic`, `-siw`;
    /// app/src-tauri/tauri.conf.json): the native names never take that form.
    #[test]
    fn the_names_never_meet_the_installed_apps() {
        let conf: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/src-tauri/tauri.conf.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let identifier = conf["identifier"].as_str().unwrap();
        for home in ["/tmp/a", "C:\\Users\\x\\AppData\\Local\\N8Tray"] {
            let ours = instance_name(&Folders::under(Path::new(home)));
            for suffix in ["sim", "sic", "siw"] {
                assert_ne!(ours, format!("{identifier}-{suffix}"));
            }
            assert!(!ours.starts_with(identifier));
        }
    }

    #[test]
    fn the_default_renderer_is_compiled_in() {
        assert!(renderer_built(DEFAULT_RENDERER), "{DEFAULT_RENDERER}");
        assert!(!renderer_built("femtovg"));
        assert_eq!(
            renderer_built("skia-software"),
            cfg!(feature = "skia-software")
        );
    }
}
