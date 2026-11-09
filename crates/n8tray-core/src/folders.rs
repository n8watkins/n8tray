//! Where the host keeps things (spec section 2, plan 2.1).
//!
//! | | Windows | macOS | Linux |
//! |---|---|---|---|
//! | base | `%LOCALAPPDATA%\N8Tray` | `~/Library/Application Support/N8Tray` | `$XDG_DATA_HOME/n8tray` |
//! | settings | `<base>\settings.json` | `<base>/settings.json` | `$XDG_CONFIG_HOME/n8tray/settings.json` |
//! | logs, history, status | `<base>\logs`, `history`, `status.json` | same | under `$XDG_STATE_HOME/n8tray` |
//!
//! `N8TRAY_HOME` puts everything under one folder (tests, and trying a build
//! next to an installed tray); `N8TRAY_RUN_DIR` and `N8TRAY_MODULES_DIR`
//! override the two folders apps share (spec section 2).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folders {
    pub modules: PathBuf,
    /// `None` on Linux without `XDG_RUNTIME_DIR`: nothing can be hosted.
    pub run: Option<PathBuf>,
    pub apps: PathBuf,
    pub tools: PathBuf,
    pub history: PathBuf,
    pub logs: PathBuf,
    pub settings_file: PathBuf,
    /// The host's own status, rewritten every 10 s and on every level change.
    pub status_file: PathBuf,
    /// True when the folders are not the user's real ones (an override is
    /// set): the host then ignores the PowerShell tray, which only ever uses
    /// the real runtime folder.
    pub isolated: bool,
}

impl Folders {
    /// Everything under one folder.
    pub fn under(base: &Path) -> Folders {
        Folders {
            modules: base.join("modules"),
            run: Some(base.join("run")),
            apps: base.join("apps"),
            tools: base.join("tools"),
            history: base.join("history"),
            logs: base.join("logs"),
            settings_file: base.join("settings.json"),
            status_file: base.join("status.json"),
            isolated: true,
        }
    }

    /// The folders for this user, with the environment's overrides.
    pub fn from_env() -> Option<Folders> {
        let env = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let mut folders = match env("N8TRAY_HOME") {
            Some(home) => Folders::under(&home),
            None => Folders::default_for_user()?,
        };
        if let Some(run) = env("N8TRAY_RUN_DIR") {
            folders.run = Some(run);
            folders.isolated = true;
        }
        if let Some(modules) = env("N8TRAY_MODULES_DIR") {
            folders.modules = modules;
            folders.isolated = true;
        }
        Some(folders)
    }

    fn default_for_user() -> Option<Folders> {
        let modules = n8tray_contract::paths::modules_dir()?;
        let run = n8tray_contract::paths::run_dir();
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)?;
        if cfg!(windows) || cfg!(target_os = "macos") {
            let base = modules.parent()?.to_path_buf();
            Some(Folders {
                modules,
                run,
                apps: base.join("apps"),
                tools: base.join("tools"),
                history: base.join("history"),
                logs: base.join("logs"),
                settings_file: base.join("settings.json"),
                status_file: base.join("status.json"),
                isolated: false,
            })
        } else {
            let xdg = |name: &str, default: &str| {
                std::env::var_os(name)
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(default))
                    .join("n8tray")
            };
            let data = xdg("XDG_DATA_HOME", ".local/share");
            let state = xdg("XDG_STATE_HOME", ".local/state");
            let config = xdg("XDG_CONFIG_HOME", ".config");
            Some(Folders {
                modules,
                run,
                apps: data.join("apps"),
                tools: data.join("tools"),
                history: state.join("history"),
                logs: state.join("logs"),
                settings_file: config.join("settings.json"),
                status_file: state.join("status.json"),
                isolated: false,
            })
        }
    }

    /// The base folder shown as "Open tray folder": where settings live.
    pub fn base(&self) -> &Path {
        self.settings_file.parent().unwrap_or(Path::new("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_one_folder() {
        let folders = Folders::under(Path::new("/t"));
        assert_eq!(folders.run.as_deref(), Some(Path::new("/t/run")));
        assert_eq!(folders.modules, Path::new("/t/modules"));
        assert_eq!(folders.base(), Path::new("/t"));
        assert!(folders.isolated);
    }
}
