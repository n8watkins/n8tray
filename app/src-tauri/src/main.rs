//! N8 Tray: the tray icon, its menu and the window, around `n8tray-core`'s
//! host. Rust owns every decision; the window only shows the host's snapshot
//! and sends the few commands its capability allows.
#![windows_subsystem = "windows"]

mod badge;
mod commands;
mod hub;
mod logs;
mod open;
mod recent;
mod report;
mod state;
mod tray;

use std::time::{Duration, Instant};

use n8tray_core::folders::Folders;
use n8tray_core::host::{self, Event, Options};
use n8tray_platform::autostart;
use tauri::{Emitter, Manager, RunEvent};
use tracing::{error, info, warn};

use crate::state::AppState;

/// Command-line options.
#[derive(Debug, Default)]
struct Args {
    /// Ask the running N8 Tray to exit (apps keep running).
    exit: bool,
    /// Started at sign-in.
    autostart: bool,
    /// Close the running N8 Tray's window.
    hide: bool,
    /// Open the window once started (WebDriver tests start N8 Tray this way).
    show: bool,
    /// Wait for this process to exit first (Restart N8 Tray).
    restart_after: Option<u32>,
    /// Import the PowerShell N8 Tray's settings file, then exit.
    import_settings: Option<String>,
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--exit" | "-Exit" => args.exit = true,
            "--autostart" => args.autostart = true,
            "--hide" => args.hide = true,
            "--show" => args.show = true,
            "--restart-after" => args.restart_after = it.next().and_then(|p| p.parse().ok()),
            "--import-settings" => args.import_settings = it.next(),
            _ => {}
        }
    }
    args
}

fn main() {
    let args = parse_args();
    let Some(folders) = Folders::from_env() else {
        eprintln!("N8 Tray: no home folder; set HOME or USERPROFILE");
        std::process::exit(2);
    };
    let _log_guard = logs::init(&folders.logs);

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

    // `--exit` with no N8 Tray running has nothing to do. With one running,
    // the single-instance plugin hands it the arguments and this copy ends.
    if (args.exit || args.hide) && host_is_free(&folders) {
        return;
    }

    let folders_for_setup = folders.clone();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // Starting N8 Tray again opens its window; `--hide` closes it and
            // `--exit` ends the running copy.
            if argv.iter().any(|a| a == "--exit" || a == "-Exit") {
                info!("Asked to exit by another copy of N8 Tray");
                state::shutdown(app);
                app.exit(0);
            } else if argv.iter().any(|a| a == "--hide") {
                hub::hide(app);
            } else {
                hub::show(app, None, None);
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::module_action,
            commands::open_item,
            commands::hide_window,
            commands::app_icon,
            commands::set_setting,
            commands::host_command,
            commands::open_host,
            commands::open_update,
            commands::history,
            commands::recent_warnings,
            commands::status_text,
            commands::fit_window,
            commands::initial_view,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            app.manage(AppState::default());
            tray::create(&handle)?;

            let mut options = Options::new(folders_for_setup.clone());
            options.check_updates = true;
            // A trial run in its own folders never touches the real sign-in entry.
            if !folders_for_setup.isolated {
                options.autostart_command = std::env::current_exe()
                    .ok()
                    .map(|exe| autostart::command_for(&exe, &["--autostart"]));
            }
            let events = handle.clone();
            let started = host::start(options, move |event| match event {
                Event::Snapshot(snapshot) => {
                    *events.state::<AppState>().latest.lock().unwrap() = Some(snapshot.clone());
                    let _ = events.emit_to("hub", "snapshot", &*snapshot);
                    tray::update(&events, &snapshot);
                }
                Event::Notify { title, body } => {
                    use tauri_plugin_notification::NotificationExt;
                    info!("Notification: {title}: {body}");
                    if let Err(e) = events
                        .notification()
                        .builder()
                        .title(&title)
                        .body(&body)
                        .show()
                    {
                        warn!("The notification could not be shown: {e}");
                    }
                }
            });
            match started {
                Ok(host) => {
                    *handle.state::<AppState>().host.lock().unwrap() = Some(host);
                    // Start-up touches far more memory than an idle tray keeps.
                    std::thread::spawn(|| {
                        std::thread::sleep(Duration::from_secs(30));
                        n8tray_platform::memory::trim_own_working_set();
                    });
                    if args.show {
                        hub::show(&handle, None, None);
                    }
                    info!(
                        "N8 Tray {} started{}",
                        n8tray_core::VERSION,
                        if args.autostart { " at sign-in" } else { "" }
                    );
                }
                Err(e) => {
                    error!("N8 Tray cannot start: {e}");
                    handle.exit(1);
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(e) => {
            error!("N8 Tray could not start its window system: {e}");
            std::process::exit(1);
        }
    };
    app.run(|app, event| {
        if let RunEvent::ExitRequested { code, api, .. } = event {
            // Closing the last window is not exiting: the tray stays.
            if code.is_none() {
                api.prevent_exit();
            } else {
                state::shutdown(app);
            }
        }
    });
}

/// Whether no N8 Tray holds the host lock.
fn host_is_free(folders: &Folders) -> bool {
    match &folders.run {
        Some(run) => matches!(n8tray_core::lease::HostLock::acquire(run), Ok(Some(_))),
        None => true,
    }
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
