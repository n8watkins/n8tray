//! Installing an update, on a click (plan section 10). The host finds that
//! one exists; this downloads it from the channel's feed, and Tauri's updater
//! verifies its signature against the public key in tauri.conf.json before
//! anything runs. On Windows the NSIS installer then replaces N8 Tray in place
//! and starts it again; elsewhere N8 Tray restarts itself.

use std::sync::atomic::{AtomicBool, Ordering};

use n8tray_core::host::Command;
use n8tray_core::settings::Channel;
use n8tray_core::update::{self, Install};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;
use tracing::info;

use crate::state::AppState;

/// One install at a time.
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Starts installing the available update, unless one is being installed.
pub fn install(app: &AppHandle) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let channel = match app
        .state::<AppState>()
        .latest()
        .map(|s| s.update.channel.clone())
        .as_deref()
    {
        Some("beta") => Channel::Beta,
        _ => Channel::Stable,
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        report(&app, Install::Downloading(None));
        if let Err(e) = run(&app, update::feed_url(channel)).await {
            report(&app, Install::Failed(e));
            RUNNING.store(false, Ordering::SeqCst);
        }
    });
}

fn report(app: &AppHandle, install: Install) {
    app.state::<AppState>()
        .send(Command::UpdateInstall(install));
}

async fn run(app: &AppHandle, feed: String) -> Result<(), String> {
    let url = feed
        .parse()
        .map_err(|e| format!("the update feed {feed} is not a URL: {e}"))?;
    let before_exit = app.clone();
    // Feeds and downloads must be HTTPS; only the end-to-end test's build
    // allows plain HTTP (dangerousInsecureTransportProtocol in its --config).
    let updater = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(|e| e.to_string())?
        // The installer replaces N8 Tray's files: stop the host first, so
        // the lock and the lease are released (apps keep running).
        .on_before_exit(move || {
            info!("Closing for the update's installer");
            crate::state::shutdown(&before_exit);
        })
        .build()
        .map_err(|e| e.to_string())?;
    let Some(found) = updater.check().await.map_err(|e| e.to_string())? else {
        return Err("the feed has no newer version for this computer".into());
    };
    info!("Installing N8 Tray {} from {feed}", found.version);

    let progress = app.clone();
    let mut done: u64 = 0;
    let mut shown: Option<u8> = None;
    let installing = app.clone();
    found
        .download_and_install(
            move |chunk, total| {
                done += chunk as u64;
                let percent = update::percent(done, total);
                if percent != shown {
                    shown = percent;
                    report(&progress, Install::Downloading(percent));
                }
            },
            move || report(&installing, Install::Installing),
        )
        .await
        .map_err(|e| e.to_string())?;

    // Windows exits inside download_and_install (the installer restarts N8
    // Tray); macOS and Linux replaced the app in place and restart here.
    info!("Update installed; restarting");
    crate::state::shutdown(app);
    app.restart();
}
