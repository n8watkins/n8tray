//! What a view may ask of N8 Tray besides showing another view (`Nav`): the
//! one public way from a view's `wire` to the host, the OS and the clipboard
//! (kit, owned by the shell). Every function here is safe to call from a
//! Slint callback: it hands the work to the shell on a later turn
//! (`crate::shell::defer`), and a reply comes back on the UI thread.
//!
//! Views pass only ids and keys, never a path, a URL or a command line: an
//! app's id, an action's id, an `open` key or a `menu` index, a setting's
//! key, a host target's name. The shell turns them into a host command or a
//! target the snapshot or N8 Tray's own folders name, with the Tauri app's
//! validation (`crate::commands`), and refuses anything else (D18).
//!
//! - [`send`] one [`Intent`]: a setting, a host command, Start at sign-in,
//!   an app's action, opening a host target or an app's item, installing an
//!   update.
//! - [`history`]: an app's (or every app's) memory and CPU from the day files,
//!   read on a worker thread; the reply comes back on the UI thread.
//! - [`warnings`] and [`status_text`]: what the status details show and copy.
//! - [`copy_status`] and [`copy`]: text on the clipboard, with a reply that
//!   says whether it worked.
#![forbid(unsafe_code)]

use n8tray_core::folders::Folders;
use n8tray_core::host::Command;
use n8tray_core::model::{HistoryView, HostSnapshot, WarningView};

use super::router::Models;
use crate::commands::{self, Opened};

/// Something a view asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// An on-off setting: N8 Tray's (`launchAtSignIn`, `notifications`,
    /// `checkUpdates`, `betaUpdates`, `compact`) with no app, or an app's
    /// (`enabled`, `autoStart`, `hosted`, `pinned`) with its id
    /// (`commands::set_setting`).
    Setting {
        key: String,
        on: bool,
        app: Option<String>,
    },
    /// The window's height limit by preset name: `short`, `medium` or `tall`.
    WindowHeight(String),
    /// `reloadSettings` or `checkUpdates` (`commands::host_command`).
    HostCommand(String),
    /// The footer's "Start at sign-in" switch: turns it the other way, while
    /// the native shell may edit it (`tray::SIGN_IN_EDITABLE`, D33).
    ToggleSignIn,
    /// An action of an app, or of one of its components. One that asks to
    /// confirm is armed by the first press and runs on the second within 4 s
    /// (`view::confirm`); the armed state shows on the next fill.
    Action {
        app: String,
        action: String,
        component: Option<String>,
    },
    /// One of N8 Tray's own targets, by name.
    OpenHost(HostTarget),
    /// An app's `open` entry (`kind` `open`, by key) or `menu` line (`kind`
    /// `menu`, by index), as the tray menu opens them.
    OpenItem {
        app: String,
        kind: String,
        key: String,
    },
    /// Install the available update, or try again after it failed. Until the
    /// native shell installs in place (D27), the release page opens, as the
    /// tray menu's "Install update" does.
    InstallUpdate,
}

/// N8 Tray's own things a view may open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostTarget {
    /// The latest log file (or the logs folder).
    Log,
    /// N8 Tray's folder.
    Folder,
    /// The settings file (or N8 Tray's folder while there is none).
    SettingsFile,
    /// The available update's release page (what changed in it).
    ReleasePage,
}

impl HostTarget {
    /// By the name a view's `.slint` file passes: `log`, `folder`, `settings`
    /// or `release`.
    pub fn from_name(name: &str) -> Option<HostTarget> {
        Some(match name {
            "log" => HostTarget::Log,
            "folder" => HostTarget::Folder,
            "settings" => HostTarget::SettingsFile,
            "release" => HostTarget::ReleasePage,
            _ => return None,
        })
    }
}

/// What the shell does for an intent.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Send this to the host.
    Send(Command),
    /// Open this file, folder or URL (one the snapshot or N8 Tray's folders
    /// named, never one a view passed).
    Open(String),
    /// Press an app's action (`window::Hub::press_action`).
    Press {
        app: String,
        action: String,
        component: Option<String>,
    },
}

/// Turns an intent into what the shell does, or says why not.
pub fn resolve(
    intent: Intent,
    snapshot: Option<&HostSnapshot>,
    folders: &Folders,
) -> Result<Effect, String> {
    match intent {
        Intent::Setting { key, on, app } => commands::set_setting(&key, on, app).map(Effect::Send),
        Intent::WindowHeight(name) => commands::set_window_height(&name).map(Effect::Send),
        Intent::HostCommand(name) => commands::host_command(&name).map(Effect::Send),
        Intent::ToggleSignIn => crate::tray::sign_in_command(snapshot)
            .map(Effect::Send)
            .ok_or_else(|| "Start at sign-in stays with the installed N8 Tray (D33)".into()),
        Intent::Action {
            app,
            action,
            component,
        } => {
            let snapshot = snapshot.ok_or("not ready")?;
            let found = snapshot
                .apps
                .iter()
                .find(|a| a.id == app)
                .ok_or("no such app")?;
            if !found.actions.iter().any(|a| a.id == action) {
                return Err(format!("no action {action}"));
            }
            Ok(Effect::Press {
                app,
                action,
                component,
            })
        }
        Intent::OpenHost(HostTarget::ReleasePage) | Intent::InstallUpdate => {
            commands::open_update(snapshot).map(Effect::Open)
        }
        Intent::OpenHost(target) => {
            let name = match target {
                HostTarget::Log => "log",
                HostTarget::Folder => "folder",
                _ => "settings",
            };
            commands::open_host(folders, name).map(|p| Effect::Open(p.display().to_string()))
        }
        Intent::OpenItem { app, kind, key } => {
            match commands::open_item(snapshot, &app, &kind, &key)? {
                Opened::Target(target) => Ok(Effect::Open(target)),
                Opened::Command(command) => Ok(Effect::Send(command)),
            }
        }
    }
}

/// Asks the shell for `intent` on a later turn. What it cannot do goes to
/// the log as a warning; nothing comes back.
pub fn send(intent: Intent) {
    crate::shell::defer(move |s| s.intent(intent));
}

/// A history's span: today's "24 hours" and "7 days".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Day,
    Week,
}

impl Range {
    /// `day` or `week`, as `app/ui/src/api.ts` names them.
    pub fn from_name(name: &str) -> Option<Range> {
        match name {
            "day" => Some(Range::Day),
            "week" => Some(Range::Week),
            _ => None,
        }
    }

    /// The hours read and the points they are cut into: 24 hours in
    /// 5-minute steps, 7 days in hourly steps (today's `history` command).
    pub fn hours_and_points(self) -> (u32, usize) {
        match self {
            Range::Day => (24, 288),
            Range::Week => (168, 168),
        }
    }
}

/// Whose history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryOf {
    /// Every app shown in N8 Tray, then N8 Tray itself (today's History).
    All,
    /// One app, or `n8tray` for N8 Tray itself.
    App(String),
}

/// The ids a history reads, in the order to show them: for `All`, every
/// app shown in N8 Tray (`hosted`), then `n8tray`, as `History.tsx` lists
/// them; for one app, its id if the snapshot has it (or `n8tray`).
pub fn history_ids(snapshot: &HostSnapshot, of: &HistoryOf) -> Vec<String> {
    match of {
        HistoryOf::All => snapshot
            .apps
            .iter()
            .filter(|a| a.hosted)
            .map(|a| a.id.clone())
            .chain(std::iter::once("n8tray".to_string()))
            .collect(),
        HistoryOf::App(id) if id == "n8tray" || snapshot.apps.iter().any(|a| &a.id == id) => {
            vec![id.clone()]
        }
        HistoryOf::App(_) => Vec::new(),
    }
}

/// A history read, as it comes back to the view.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryReply {
    pub of: HistoryOf,
    pub range: Range,
    /// Each id's history, in `history_ids`' order.
    pub items: Vec<(String, HistoryView)>,
}

/// Reads the day files (on the worker thread).
pub fn read_history(
    dir: &std::path::Path,
    ids: &[String],
    range: Range,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<(String, HistoryView)> {
    let (hours, points) = range.hours_and_points();
    ids.iter()
        .map(|id| {
            (
                id.clone(),
                n8tray_core::history::read(dir, id, now, hours, points),
            )
        })
        .collect()
}

/// The callback a reply goes to, on the UI thread, with the window's models.
/// It runs inside the shell: it fills the view's model and asks the shell
/// for nothing (no `crate::shell::with`).
pub type Reply<T> = Box<dyn FnOnce(&dyn Models, T) + Send + 'static>;

/// Reads `of`'s history over `range` on a worker thread; `done` gets it on
/// the UI thread, if the window still exists then. A view that asked again
/// in between compares `of` and `range` with what it shows now.
pub fn history(
    of: HistoryOf,
    range: Range,
    done: impl FnOnce(&dyn Models, HistoryReply) + Send + 'static,
) {
    let done: Reply<HistoryReply> = Box::new(done);
    crate::shell::defer(move |s| s.history(of, range, done));
}

/// The last warnings and errors of the log, newest first (Recent warnings).
pub fn warnings() -> Vec<WarningView> {
    crate::recent::list()
}

/// The status as plain text, as Copy status copies it.
pub fn status_text(snapshot: &HostSnapshot) -> String {
    crate::report::status_text(snapshot, &warnings())
}

/// Copies the status (`status_text` of the latest snapshot); `done` gets
/// whether it worked ("Copied" or "Could not copy").
pub fn copy_status(done: impl FnOnce(&dyn Models, bool) + Send + 'static) {
    let done: Reply<bool> = Box::new(done);
    crate::shell::defer(move |s| s.copy_status(done));
}

/// Copies `text`; `done` gets whether it worked.
pub fn copy(text: String, done: impl FnOnce(&dyn Models, bool) + Send + 'static) {
    let done: Reply<bool> = Box::new(done);
    crate::shell::defer(move |s| s.copy(&text, done));
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use n8tray_core::settings::{Channel, WindowHeight};

    use super::*;
    use crate::tests::fixture;

    fn folders() -> Folders {
        Folders::under(Path::new("/tmp/n8tray-native-intent-test"))
    }

    fn resolved(intent: Intent, s: &HostSnapshot) -> Result<Effect, String> {
        resolve(intent, Some(s), &folders())
    }

    fn setting(key: &str, on: bool, app: Option<&str>) -> Intent {
        Intent::Setting {
            key: key.into(),
            on,
            app: app.map(String::from),
        }
    }

    /// Every command the six screens need, by key.
    #[test]
    fn settings_and_host_commands_become_host_commands() {
        let s = fixture("busy");
        let send = |i| resolved(i, &s);
        assert_eq!(
            send(setting("pinned", false, Some("voicedock"))),
            Ok(Effect::Send(Command::SetPinned {
                app: "voicedock".into(),
                pinned: false
            }))
        );
        assert_eq!(
            send(setting("enabled", true, Some("voicedock"))),
            Ok(Effect::Send(Command::SetEnabled {
                app: "voicedock".into(),
                enabled: true
            }))
        );
        assert_eq!(
            send(setting("autoStart", true, Some("voicedock"))),
            Ok(Effect::Send(Command::SetAutoStart {
                app: "voicedock".into(),
                on: true
            }))
        );
        assert_eq!(
            send(setting("hosted", false, Some("voicedock"))),
            Ok(Effect::Send(Command::SetOwnIcon {
                app: "voicedock".into(),
                on: true
            }))
        );
        assert_eq!(
            send(setting("notifications", false, None)),
            Ok(Effect::Send(Command::SetNotifications(false)))
        );
        assert_eq!(
            send(setting("betaUpdates", true, None)),
            Ok(Effect::Send(Command::SetUpdateChannel(Channel::Beta)))
        );
        assert_eq!(
            send(setting("compact", true, None)),
            Ok(Effect::Send(Command::SetCompact(true)))
        );
        assert_eq!(
            send(Intent::WindowHeight("tall".into())),
            Ok(Effect::Send(Command::SetWindowHeight(WindowHeight::Tall)))
        );
        assert_eq!(
            send(Intent::HostCommand("checkUpdates".into())),
            Ok(Effect::Send(Command::CheckUpdates))
        );
        assert_eq!(
            send(Intent::HostCommand("reloadSettings".into())),
            Ok(Effect::Send(Command::ReloadSettings))
        );
        assert!(send(Intent::HostCommand("shutdown".into())).is_err());
        assert!(send(Intent::WindowHeight("500".into())).is_err());
        assert!(send(setting("anything", true, None)).is_err());
        // While the installed N8 Tray owns the sign-in entry (D33).
        assert!(send(Intent::ToggleSignIn).is_err());
    }

    #[test]
    fn actions_name_an_app_and_one_of_its_actions() {
        let s = fixture("busy");
        let vd = s.apps.iter().find(|a| a.id == "voicedock").unwrap();
        let stop = crate::view::format::row_action(vd).unwrap().id.clone();
        assert_eq!(
            resolved(
                Intent::Action {
                    app: "voicedock".into(),
                    action: stop.clone(),
                    component: None
                },
                &s
            ),
            Ok(Effect::Press {
                app: "voicedock".into(),
                action: stop,
                component: None
            })
        );
        let bad = |app: &str, action: &str| Intent::Action {
            app: app.into(),
            action: action.into(),
            component: None,
        };
        assert!(resolved(bad("voicedock", "rm -rf"), &s).is_err());
        assert!(resolved(bad("nope", "stop"), &s).is_err());
        assert!(
            resolve(bad("voicedock", "stop"), None, &folders()).is_err(),
            "not ready"
        );
    }

    /// Views name targets; the paths and URLs come from the snapshot and
    /// N8 Tray's folders.
    #[test]
    fn targets_are_named_never_passed() {
        let s = fixture("updating");
        let f = folders();
        assert_eq!(
            resolved(Intent::OpenHost(HostTarget::Folder), &s),
            Ok(Effect::Open(f.base().display().to_string()))
        );
        assert_eq!(
            resolved(Intent::OpenHost(HostTarget::Log), &s),
            Ok(Effect::Open(f.logs.display().to_string()))
        );
        let release = s.update.url.clone();
        assert_eq!(
            resolved(Intent::OpenHost(HostTarget::ReleasePage), &s).ok(),
            release.clone().map(Effect::Open)
        );
        assert_eq!(
            resolved(Intent::InstallUpdate, &s).ok(),
            release.map(Effect::Open)
        );
        for name in ["log", "folder", "settings", "release"] {
            assert!(HostTarget::from_name(name).is_some(), "{name}");
        }
        for name in ["C:\\Windows", "https://example.com", "Log", ""] {
            assert_eq!(HostTarget::from_name(name), None, "{name}");
        }
        // An app's item, by key, through the tray menu's validation.
        let app = s.apps.iter().find(|a| a.open.iter().any(|o| o.exists));
        if let Some(app) = app {
            let entry = app.open.iter().find(|o| o.exists).unwrap();
            assert_eq!(
                resolved(
                    Intent::OpenItem {
                        app: app.id.clone(),
                        kind: "open".into(),
                        key: entry.key.clone()
                    },
                    &s
                ),
                Ok(Effect::Open(entry.target.clone()))
            );
        }
        assert!(
            resolved(
                Intent::OpenItem {
                    app: s.apps[0].id.clone(),
                    kind: "open".into(),
                    key: "C:\\Windows\\System32\\cmd.exe".into()
                },
                &s
            )
            .is_err()
        );
    }

    #[test]
    fn a_history_reads_todays_ids_and_ranges() {
        let s = fixture("busy");
        let all = history_ids(&s, &HistoryOf::All);
        let hosted: Vec<String> = s
            .apps
            .iter()
            .filter(|a| a.hosted)
            .map(|a| a.id.clone())
            .collect();
        assert_eq!(&all[..all.len() - 1], &hosted[..]);
        assert_eq!(all.last().map(String::as_str), Some("n8tray"));
        assert_eq!(
            history_ids(&s, &HistoryOf::App("voicedock".into())),
            ["voicedock"]
        );
        assert_eq!(history_ids(&s, &HistoryOf::App("n8tray".into())), ["n8tray"]);
        assert!(history_ids(&s, &HistoryOf::App("../../etc".into())).is_empty());
        assert_eq!(Range::from_name("day"), Some(Range::Day));
        assert_eq!(Range::from_name("week"), Some(Range::Week));
        assert_eq!(Range::from_name("month"), None);
        assert_eq!(Range::Day.hours_and_points(), (24, 288));
        assert_eq!(Range::Week.hours_and_points(), (168, 168));

        // Nothing recorded: one empty step series per id, in order.
        let dir = std::env::temp_dir().join(format!("s0b-history-{}", std::process::id()));
        let ids = vec!["voicedock".to_string(), "n8tray".to_string()];
        let read = read_history(&dir, &ids, Range::Day, chrono::Utc::now());
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].0, "voicedock");
        assert_eq!(read[0].1.memory_mb.len(), 288);
        assert!(read[0].1.memory_mb.iter().all(|m| *m == 0.0));
    }

    #[test]
    fn the_status_text_is_copy_statuss() {
        let s = fixture("healthy");
        let text = status_text(&s);
        assert!(text.starts_with(&format!("N8 Tray {}: {}", s.version, s.headline)));
    }

    /// Writes the real clipboard, so only where asked (`N8TRAY_CLIPBOARD_TEST`,
    /// set by CI's Windows runner): never on a PC someone is using.
    #[test]
    fn text_goes_on_the_clipboard() {
        if std::env::var_os("N8TRAY_CLIPBOARD_TEST").is_none() {
            return;
        }
        let text = format!("N8 Tray clipboard test {} · é", std::process::id());
        n8tray_platform::clipboard::set_text(&text).unwrap();
        assert_eq!(n8tray_platform::clipboard::text(), Some(text));
    }
}
