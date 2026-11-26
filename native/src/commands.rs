//! What the tray menu and the window (`view::intent`) may ask for, as plain
//! functions with the Tauri app's validation (app/src-tauri/src/commands.rs).
//! The window only ever passes ids and keys, never a path, a URL or a
//! command line: each function turns them into a host command or into a
//! target the snapshot or N8 Tray's own folders name (D18).
#![forbid(unsafe_code)]

use std::path::PathBuf;

use n8tray_core::folders::Folders;
use n8tray_core::host::Command;
use n8tray_core::model::HostSnapshot;
use n8tray_core::settings::Channel;

/// What opening an app's entry comes to.
#[derive(Debug, Clone, PartialEq)]
pub enum Opened {
    /// A file, folder or URL the app's manifest declared, for the OS to open.
    Target(String),
    /// A `menu` line of kind `action`: an action for the host.
    Command(Command),
}

/// `start`, `stop`, `restart` or a declared action of an app.
pub fn module_action(id: String, action: String, component: Option<String>) -> Command {
    Command::Action {
        app: id,
        action,
        component,
    }
}

/// An app's `open` entry (by key) or `menu` line (by index).
pub fn open_item(
    snapshot: Option<&HostSnapshot>,
    id: &str,
    kind: &str,
    key: &str,
) -> Result<Opened, String> {
    let snapshot = snapshot.ok_or("not ready")?;
    let app = snapshot
        .apps
        .iter()
        .find(|a| a.id == id)
        .ok_or("no such app")?;
    match kind {
        "open" => {
            let item = app
                .open
                .iter()
                .find(|o| o.key == key)
                .ok_or("no such entry")?;
            if !item.exists {
                return Err("it does not exist".into());
            }
            Ok(Opened::Target(item.target.clone()))
        }
        "menu" => {
            let index: usize = key.parse().map_err(|_| "not an index")?;
            let item = app.menu.get(index).ok_or("no such line")?;
            match item.kind.as_str() {
                "action" => Ok(Opened::Command(Command::Action {
                    app: id.into(),
                    action: item.target.clone(),
                    component: None,
                })),
                _ if item.exists => Ok(Opened::Target(item.target.clone())),
                _ => Err("it does not exist".into()),
            }
        }
        _ => Err("unknown kind".into()),
    }
}

/// One setting. `key` is one of a fixed list; app settings name the app.
pub fn set_setting(key: &str, on: bool, app: Option<String>) -> Result<Command, String> {
    Ok(match (key, app) {
        ("launchAtSignIn", None) => Command::SetLaunchAtSignIn(on),
        ("notifications", None) => Command::SetNotifications(on),
        ("checkUpdates", None) => Command::SetCheckUpdates(on),
        ("betaUpdates", None) => {
            Command::SetUpdateChannel(if on { Channel::Beta } else { Channel::Stable })
        }
        ("enabled", Some(app)) => Command::SetEnabled { app, enabled: on },
        ("autoStart", Some(app)) => Command::SetAutoStart { app, on },
        ("hosted", Some(app)) => Command::SetOwnIcon { app, on: !on },
        // Not in the Tauri app's list on purpose: pinning (D32) is shown by
        // the native window only, so only its commands take it.
        ("pinned", Some(app)) => Command::SetPinned { app, pinned: on },
        _ => return Err(format!("unknown setting {key}")),
    })
}

/// Reloads the settings file, or checks for an update now.
pub fn host_command(name: &str) -> Result<Command, String> {
    match name {
        "reloadSettings" => Ok(Command::ReloadSettings),
        "checkUpdates" => Ok(Command::CheckUpdates),
        _ => Err(format!("unknown command {name}")),
    }
}

/// One of N8 Tray's own files: `log`, `folder` or `settings`.
pub fn open_host(folders: &Folders, target: &str) -> Result<PathBuf, String> {
    Ok(match target {
        "log" => crate::logs::latest(&folders.logs).unwrap_or(folders.logs.clone()),
        "folder" => folders.base().to_path_buf(),
        "settings" if folders.settings_file.exists() => folders.settings_file.clone(),
        "settings" => folders.base().to_path_buf(),
        _ => return Err(format!("unknown target {target}")),
    })
}

/// The release page of an available update (what changed in it).
pub fn open_update(snapshot: Option<&HostSnapshot>) -> Result<String, String> {
    snapshot
        .and_then(|s| s.update.url.clone())
        .ok_or_else(|| "no update".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> HostSnapshot {
        crate::tests::fixture(name)
    }

    #[test]
    fn open_entries_are_found_by_key_and_must_exist() {
        let s = fixture("healthy");
        let app = s
            .apps
            .iter()
            .find(|a| !a.open.is_empty())
            .expect("an app with entries");
        let entry = &app.open[0];
        let got = open_item(Some(&s), &app.id, "open", &entry.key);
        if entry.exists {
            assert_eq!(got, Ok(Opened::Target(entry.target.clone())));
        } else {
            assert_eq!(got, Err("it does not exist".into()));
        }
        assert_eq!(
            open_item(Some(&s), &app.id, "open", "no-such-key"),
            Err("no such entry".into())
        );
        let mut missing = s.clone();
        let target = missing.apps.iter_mut().find(|a| a.id == app.id).unwrap();
        target.open[0].exists = false;
        assert_eq!(
            open_item(Some(&missing), &app.id, "open", &entry.key),
            Err("it does not exist".into())
        );
    }

    #[test]
    fn only_ids_and_keys_get_through() {
        let s = fixture("healthy");
        let id = &s.apps[0].id;
        assert_eq!(open_item(None, id, "open", "x"), Err("not ready".into()));
        assert_eq!(
            open_item(Some(&s), "no-such-app", "open", "x"),
            Err("no such app".into())
        );
        // A path or a URL is never a key.
        for key in [
            "C:\\Windows\\System32\\cmd.exe",
            "https://example.com",
            "../..",
        ] {
            assert!(open_item(Some(&s), id, "open", key).is_err(), "{key}");
            assert!(open_item(Some(&s), id, "menu", key).is_err(), "{key}");
        }
        assert_eq!(
            open_item(Some(&s), id, "exec", "0"),
            Err("unknown kind".into())
        );
    }

    /// The healthy fixture with menu lines of every kind on its first app.
    fn fixture_with_menu() -> (HostSnapshot, String) {
        let mut s = fixture("healthy");
        let json = serde_json::json!([
            {"label": "Docs", "kind": "url", "target": "https://example.com/docs", "exists": true},
            {"label": "Reindex", "kind": "action", "target": "reindex", "exists": true},
            {"label": "Gone", "kind": "file", "target": "C:\\gone.txt", "exists": false}
        ]);
        s.apps[0].menu = serde_json::from_value(json).expect("menu lines");
        let id = s.apps[0].id.clone();
        (s, id)
    }

    fn menu_lines_on((s, id): (HostSnapshot, String)) {
        let app = s.apps.iter().find(|a| a.id == id).unwrap();
        for (index, line) in app.menu.iter().enumerate() {
            let got = open_item(Some(&s), &id, "menu", &index.to_string());
            match line.kind.as_str() {
                "action" => assert_eq!(
                    got,
                    Ok(Opened::Command(Command::Action {
                        app: id.clone(),
                        action: line.target.clone(),
                        component: None
                    }))
                ),
                _ if line.exists => assert_eq!(got, Ok(Opened::Target(line.target.clone()))),
                _ => assert_eq!(got, Err("it does not exist".into())),
            }
        }
        let past = app.menu.len().to_string();
        assert_eq!(
            open_item(Some(&s), &id, "menu", &past),
            Err("no such line".into())
        );
        assert_eq!(
            open_item(Some(&s), &id, "menu", "-1"),
            Err("not an index".into())
        );
    }

    #[test]
    fn menu_lines_are_found_by_index() {
        menu_lines_on(fixture_with_menu());
    }

    #[test]
    fn settings_are_a_fixed_list() {
        assert_eq!(
            set_setting("launchAtSignIn", true, None),
            Ok(Command::SetLaunchAtSignIn(true))
        );
        assert_eq!(
            set_setting("betaUpdates", true, None),
            Ok(Command::SetUpdateChannel(Channel::Beta))
        );
        assert_eq!(
            set_setting("betaUpdates", false, None),
            Ok(Command::SetUpdateChannel(Channel::Stable))
        );
        assert_eq!(
            set_setting("hosted", true, Some("voicedock".into())),
            Ok(Command::SetOwnIcon {
                app: "voicedock".into(),
                on: false
            })
        );
        assert_eq!(
            set_setting("pinned", false, Some("voicedock".into())),
            Ok(Command::SetPinned {
                app: "voicedock".into(),
                pinned: false
            })
        );
        // An app setting needs an app, a host setting takes none.
        assert!(set_setting("enabled", true, None).is_err());
        assert!(set_setting("notifications", true, Some("voicedock".into())).is_err());
        assert!(set_setting("anything", true, None).is_err());
        // The low-memory notification left with the PC's memory (D34).
        assert!(set_setting("memoryNotifications", true, None).is_err());
    }

    /// The keys are the Tauri app's (app/src-tauri/src/commands.rs) plus
    /// `pinned`, which only the native window shows (D32): a deliberate
    /// difference, not drift.
    #[test]
    fn the_keys_are_todays_plus_pinned() {
        let todays_host = [
            "launchAtSignIn",
            "notifications",
            "checkUpdates",
            "betaUpdates",
        ];
        let todays_app = ["enabled", "autoStart", "hosted"];
        for key in todays_host {
            assert!(set_setting(key, true, None).is_ok(), "{key}");
        }
        for key in todays_app {
            assert!(set_setting(key, true, Some("a".into())).is_ok(), "{key}");
        }
        for on in [true, false] {
            assert_eq!(
                set_setting("pinned", on, Some("a".into())),
                Ok(Command::SetPinned {
                    app: "a".into(),
                    pinned: on
                })
            );
        }
        assert!(
            set_setting("pinned", true, None).is_err(),
            "pinning names an app"
        );
    }

    #[test]
    fn host_commands_are_a_fixed_list() {
        assert_eq!(host_command("reloadSettings"), Ok(Command::ReloadSettings));
        assert_eq!(host_command("checkUpdates"), Ok(Command::CheckUpdates));
        assert!(host_command("shutdown").is_err());
    }

    #[test]
    fn host_targets_are_n8_trays_own_folders() {
        let dir = std::env::temp_dir().join(format!("n8tray-native-test-{}", std::process::id()));
        let folders = Folders::under(&dir);
        assert_eq!(open_host(&folders, "folder"), Ok(dir.clone()));
        // No settings file yet: its folder.
        assert_eq!(open_host(&folders, "settings"), Ok(dir.clone()));
        assert_eq!(open_host(&folders, "log"), Ok(folders.logs.clone()));
        assert!(open_host(&folders, "C:\\Windows").is_err());
    }

    #[test]
    fn an_update_page_needs_an_update() {
        let s = fixture("healthy");
        assert_eq!(open_update(None), Err("no update".into()));
        assert_eq!(open_update(Some(&s)).ok(), s.update.url.clone());
        assert_eq!(
            module_action("a".into(), "stop".into(), None),
            Command::Action {
                app: "a".into(),
                action: "stop".into(),
                component: None
            }
        );
    }
}
