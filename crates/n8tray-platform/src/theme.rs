//! Whether the taskbar is light or dark, and a watch for when it changes, so
//! the tray icon can match it.
//!
//! On Windows the taskbar follows `SystemUsesLightTheme` under
//! HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize,
//! the "Windows mode" in Settings. The apps' mode (`AppsUseLightTheme`), which
//! a window's theme events follow, can differ from it, so it is not used.
//! Elsewhere there is no such signal yet: the answer is unknown and nothing
//! is watched.

/// The taskbar's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Taskbar {
    #[default]
    Dark,
    Light,
}

#[cfg(any(windows, test))]
const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
#[cfg(any(windows, test))]
const SYSTEM_USES_LIGHT_THEME: &str = "SystemUsesLightTheme";

impl Taskbar {
    /// From `SystemUsesLightTheme`: 0 is dark and any other number light.
    /// Without the value (Windows 10 before 1903, whose taskbar was always
    /// dark) the taskbar is dark.
    pub fn from_system_uses_light_theme(value: Option<u32>) -> Taskbar {
        match value {
            Some(0) | None => Taskbar::Dark,
            Some(_) => Taskbar::Light,
        }
    }
}

/// The taskbar's colour now; `None` where the platform cannot say.
pub fn taskbar() -> Option<Taskbar> {
    #[cfg(windows)]
    {
        Some(Taskbar::from_system_uses_light_theme(
            crate::win::theme::dword(PERSONALIZE, SYSTEM_USES_LIGHT_THEME),
        ))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Calls `changed` from a thread of its own each time the taskbar changes
/// colour, until the process ends. Returns whether a watch was started.
pub fn watch_taskbar(changed: impl Fn(Taskbar) + Send + 'static) -> bool {
    #[cfg(windows)]
    {
        crate::win::theme::watch_dword(
            PERSONALIZE,
            SYSTEM_USES_LIGHT_THEME,
            Box::new(move |value| changed(Taskbar::from_system_uses_light_theme(value))),
        )
    }
    #[cfg(not(windows))]
    {
        let _ = changed;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_value_names_the_taskbar() {
        assert_eq!(
            Taskbar::from_system_uses_light_theme(Some(0)),
            Taskbar::Dark
        );
        assert_eq!(
            Taskbar::from_system_uses_light_theme(Some(1)),
            Taskbar::Light
        );
        assert_eq!(
            Taskbar::from_system_uses_light_theme(Some(2)),
            Taskbar::Light
        );
        assert_eq!(Taskbar::from_system_uses_light_theme(None), Taskbar::Dark);
        assert_eq!(Taskbar::default(), Taskbar::Dark);
        assert!(PERSONALIZE.ends_with(r"Themes\Personalize"));
        assert_eq!(SYSTEM_USES_LIGHT_THEME, "SystemUsesLightTheme");
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_nothing_is_known_or_watched() {
        assert_eq!(taskbar(), None);
        assert!(!watch_taskbar(|_| {}));
    }

    #[cfg(windows)]
    #[test]
    fn the_real_value_is_read() {
        // Whatever the runner's mode is, it is an answer.
        assert!(taskbar().is_some());
    }

    /// The watch, on a key of its own (the real one belongs to the user).
    #[cfg(windows)]
    #[test]
    fn a_change_is_reported_once() {
        use std::sync::mpsc;
        use std::time::Duration;
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;

        let path = format!("Software\\N8TrayTests\\Theme{}", std::process::id());
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(&path).unwrap();
        key.set_value(SYSTEM_USES_LIGHT_THEME, &0u32).unwrap();
        assert_eq!(
            crate::win::theme::dword(&path, SYSTEM_USES_LIGHT_THEME),
            Some(0)
        );

        let (tx, rx) = mpsc::channel();
        assert!(crate::win::theme::watch_dword(
            &path,
            SYSTEM_USES_LIGHT_THEME,
            Box::new(move |value| {
                let _ = tx.send(Taskbar::from_system_uses_light_theme(value));
            }),
        ));
        // Another value under the key changes: nothing to report.
        key.set_value("AppsUseLightTheme", &1u32).unwrap();
        key.set_value(SYSTEM_USES_LIGHT_THEME, &1u32).unwrap();
        let first = rx.recv_timeout(Duration::from_secs(10));
        // Setting the same value again is not a change.
        key.set_value(SYSTEM_USES_LIGHT_THEME, &1u32).unwrap();
        let second = rx.recv_timeout(Duration::from_millis(500));
        let _ = hkcu.delete_subkey_all(&path);
        assert_eq!(first, Ok(Taskbar::Light));
        assert!(second.is_err(), "reported twice: {second:?}");
    }
}
