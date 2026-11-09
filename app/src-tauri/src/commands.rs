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
