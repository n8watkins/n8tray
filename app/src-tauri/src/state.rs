//! What the tray, the window and the commands share.

use std::sync::{Arc, Mutex};

use n8tray_core::host::{Command, HostHandle};
use n8tray_core::model::HostSnapshot;
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct AppState {
    pub host: Mutex<Option<HostHandle>>,
    pub latest: Mutex<Option<Arc<HostSnapshot>>>,
    pub tray: Mutex<crate::tray::TrayState>,
    pub hub: Mutex<crate::hub::HubState>,
}

impl AppState {
    pub fn send(&self, command: Command) {
        if let Some(host) = self.host.lock().unwrap().as_ref() {
            host.send(command);
        }
    }

    pub fn latest(&self) -> Option<Arc<HostSnapshot>> {
        self.latest.lock().unwrap().clone()
    }
}

/// Stops the host (apps keep running); safe to call twice.
pub fn shutdown(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>()
        && let Some(host) = state.host.lock().unwrap().take()
    {
        host.shutdown();
    }
}
