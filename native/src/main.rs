//! N8 Tray's native shell (lane N0): the same host as the Tauri app, the
//! same tray icon and menu, and a Slint window with no WebView. It runs
//! next to the installed app as `n8tray-native.exe` until the switch, with a
//! single-instance name of its own (docs/DECISIONS.md, D33).
#![windows_subsystem = "windows"]
// The shell runs on Windows only for now; elsewhere it is compiled and
// tested, not run.
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

mod args;
mod badge;
mod commands;
mod logs;
mod open;
mod recent;
mod shell;
mod tray;
mod window;

use std::time::{Duration, Instant};

use n8tray_core::folders::Folders;
use n8tray_core::host::{self, Options};
use n8tray_platform::single::{self, Instance, Request};
use tracing::{error, info, warn};

slint::include_modules!();

/// The renderer when `N8TRAY_RENDERER` names none (D33).
const DEFAULT_RENDERER: &str = "software";

#[cfg(not(windows))]
fn main() {
    eprintln!("n8tray-native runs on Windows only for now; use the Tauri app (n8tray) here.");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    let args = args::parse(std::env::args().skip(1));
    let Some(folders) = Folders::from_env() else {
        eprintln!("N8 Tray: no home folder; set HOME or USERPROFILE");
        std::process::exit(2);
    };
    let log_guard = logs::init(&folders.logs);
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
        "N8 Tray {} started (native shell, {} renderer){}",
        n8tray_core::VERSION,
        renderer(),
        if args.autostart { " at sign-in" } else { "" }
    );

    if let Err(e) = slint::run_event_loop_until_quit() {
        error!("The event loop ended with an error: {e}");
    }
    // Take things apart here, while the other threads are alive: the host is
    // stopped and joined, the tray icon leaves the notification area, then
    // the single-instance name is released. Then leave without running the
    // thread-locals' destructors, which would do this during teardown.
    if let Some(shell) = shell::take() {
        shell.close();
    }
    drop(instance);
    info!("N8 Tray exited");
    drop(log_guard);
    std::process::exit(0);
}

/// `N8TRAY_RENDERER`: `software` or `skia-software`, for measuring.
fn renderer() -> String {
    std::env::var("N8TRAY_RENDERER")
        .ok()
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| DEFAULT_RENDERER.into())
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
        // Never the Tauri app's single-instance name (its identifier).
        assert!(a.starts_with("N8TrayNative-"), "{a}");
        assert!(!a.contains("com.n8watkins.n8tray"));
        assert!(!a.contains('\\'), "a kernel object name has no backslash");
    }

    #[test]
    fn the_default_renderer_is_one_that_is_built() {
        assert!(["software", "skia-software"].contains(&DEFAULT_RENDERER));
    }
}
