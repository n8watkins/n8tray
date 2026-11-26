//! The commands the window may call (capabilities/hub.json). Each one only
//! forwards an intent to the host or opens something the snapshot names; the
//! window can never pass a path, a URL or a command line.

use n8tray_core::host::Command;
use n8tray_core::model::HostSnapshot;
use tauri::{AppHandle, State};

use crate::state::AppState;

#[tauri::command]
pub fn get_snapshot(state: State<AppState>) -> Option<HostSnapshot> {
    state.latest().map(|s| (*s).clone())
}

#[tauri::command]
pub fn module_action(
    state: State<AppState>,
    id: String,
    action: String,
    component: Option<String>,
) {
    state.send(Command::Action {
        app: id,
        action,
        component,
    });
}

/// Opens an app's `open` entry (by key) or `menu` line (by index).
#[tauri::command]
pub fn open_item(
    state: State<AppState>,
    id: String,
    kind: String,
    key: String,
) -> Result<(), String> {
    let snapshot = state.latest().ok_or("not ready")?;
    let app = snapshot
        .apps
        .iter()
        .find(|a| a.id == id)
        .ok_or("no such app")?;
    match kind.as_str() {
        "open" => {
            let item = app
                .open
                .iter()
                .find(|o| o.key == key)
                .ok_or("no such entry")?;
            if !item.exists {
                return Err("it does not exist".into());
            }
            crate::open::open(&item.target);
        }
        "menu" => {
            let index: usize = key.parse().map_err(|_| "not an index")?;
            let item = app.menu.get(index).ok_or("no such line")?;
            match item.kind.as_str() {
                "action" => state.send(Command::Action {
                    app: id,
                    action: item.target.clone(),
                    component: None,
                }),
                _ if item.exists => crate::open::open(&item.target),
                _ => return Err("it does not exist".into()),
            }
        }
        _ => return Err("unknown kind".into()),
    }
    Ok(())
}

#[tauri::command]
pub fn hide_window(app: AppHandle) {
    crate::hub::hide(&app);
}

/// An app's icon as a `data:` URL, read by the host from the path its
/// manifest declared (spec 3.11): at most 256 KB, SVG, PNG or ICO. The
/// window shows it only as an image; `None` means the generic glyph.
#[tauri::command]
pub fn app_icon(state: State<AppState>, id: String) -> Option<String> {
    let snapshot = state.latest()?;
    let path = snapshot.apps.iter().find(|a| a.id == id)?.icon.clone()?;
    icon_data_url(std::path::Path::new(&path))
}

pub const ICON_MAX_BYTES: u64 = 256 * 1024;

pub fn icon_data_url(path: &std::path::Path) -> Option<String> {
    use base64::Engine;
    let mime = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        _ => return None,
    };
    if std::fs::metadata(path).ok()?.len() > ICON_MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Changes one setting. `key` is one of a fixed list; app settings name the
/// app. The host saves it and the next snapshot shows it.
#[tauri::command]
pub fn set_setting(
    state: State<AppState>,
    key: String,
    on: bool,
    app: Option<String>,
) -> Result<(), String> {
    let command = match (key.as_str(), app) {
        ("launchAtSignIn", None) => Command::SetLaunchAtSignIn(on),
        ("notifications", None) => Command::SetNotifications(on),
        ("checkUpdates", None) => Command::SetCheckUpdates(on),
        ("betaUpdates", None) => Command::SetUpdateChannel(if on {
            n8tray_core::settings::Channel::Beta
        } else {
            n8tray_core::settings::Channel::Stable
        }),
        ("enabled", Some(app)) => Command::SetEnabled { app, enabled: on },
        ("autoStart", Some(app)) => Command::SetAutoStart { app, on },
        ("hosted", Some(app)) => Command::SetOwnIcon { app, on: !on },
        _ => return Err(format!("unknown setting {key}")),
    };
    state.send(command);
    Ok(())
}

/// Reloads the settings file, or checks for an update now.
#[tauri::command]
pub fn host_command(state: State<AppState>, name: String) -> Result<(), String> {
    state.send(match name.as_str() {
        "reloadSettings" => Command::ReloadSettings,
        "checkUpdates" => Command::CheckUpdates,
        _ => return Err(format!("unknown command {name}")),
    });
    Ok(())
}

/// Opens one of N8 Tray's own files: `log`, `folder` or `settings`.
#[tauri::command]
pub fn open_host(target: String) -> Result<(), String> {
    let folders = n8tray_core::folders::Folders::from_env().ok_or("no home folder")?;
    let path = match target.as_str() {
        "log" => crate::logs::latest(&folders.logs).unwrap_or(folders.logs.clone()),
        "folder" => folders.base().to_path_buf(),
        "settings" if folders.settings_file.exists() => folders.settings_file.clone(),
        "settings" => folders.base().to_path_buf(),
        _ => return Err(format!("unknown target {target}")),
    };
    crate::open::open(&path.display().to_string());
    Ok(())
}

/// Installs the available update: downloads it, verifies its signature and
/// restarts N8 Tray on the new version. Progress comes in the snapshot.
#[tauri::command]
pub fn install_update(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let available = state.latest().is_some_and(|s| s.update.available.is_some());
    if !available {
        return Err("no update".into());
    }
    crate::updater::install(&app);
    Ok(())
}

/// Opens the release page of an available update (what changed in it).
#[tauri::command]
pub fn open_update(state: State<AppState>) -> Result<(), String> {
    let url = state
        .latest()
        .and_then(|s| s.update.url.clone())
        .ok_or("no update")?;
    crate::open::open(&url);
    Ok(())
}

/// An app's (or `n8tray`'s) memory and CPU from the history files: the last
/// 24 hours in 5-minute steps, or the last 7 days in hourly steps.
#[tauri::command]
pub fn history(id: String, range: String) -> Result<n8tray_core::model::HistoryView, String> {
    let folders = n8tray_core::folders::Folders::from_env().ok_or("no home folder")?;
    let (hours, points) = match range.as_str() {
        "day" => (24, 288),
        "week" => (168, 168),
        _ => return Err(format!("unknown range {range}")),
    };
    Ok(n8tray_core::history::read(
        &folders.history,
        &id,
        chrono::Utc::now(),
        hours,
        points,
    ))
}

/// The last warnings and errors, newest first.
#[tauri::command]
pub fn recent_warnings() -> Vec<n8tray_core::model::WarningView> {
    crate::recent::list()
}

/// The status as plain text, for Copy status.
#[tauri::command]
pub fn status_text(state: State<AppState>) -> Option<String> {
    state
        .latest()
        .map(|s| crate::report::status_text(&s, &crate::recent::list()))
}

/// The window drew itself `height` CSS pixels tall: size it, and on its
/// first draw place and show it, so it appears in one piece.
#[tauri::command]
pub fn fit_window(app: AppHandle, height: f64) {
    crate::hub::fit(&app, height);
}

/// Which view the window opens on (`hub` or `details`), once.
#[tauri::command]
pub fn initial_view(state: State<AppState>) -> String {
    state
        .hub
        .lock()
        .unwrap()
        .take_view()
        .unwrap_or_else(|| "hub".into())
}
