//! The PowerShell N8 Tray, which the Tauri host replaces: one host at a time
//! (spec 8.1).

/// The PowerShell tray's single-instance mutex.
pub const LEGACY_MUTEX: &str = "Local\\N8Tray_SingleInstance";

/// Whether the PowerShell N8 Tray runs in this session.
pub fn legacy_tray_running() -> bool {
    #[cfg(windows)]
    {
        crate::win::system::mutex_exists(LEGACY_MUTEX)
    }
    #[cfg(not(windows))]
    {
        false
    }
}
