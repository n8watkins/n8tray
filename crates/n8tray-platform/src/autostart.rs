//! Start at sign-in. The host's setting is the one source of truth; the OS
//! entry follows it, is repaired at start, and an entry the host did not make
//! is left alone (the PowerShell tray's rule, kept).
//!
//! - Windows: the value `N8 Tray` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
//! - Linux: `$XDG_CONFIG_HOME/autostart/n8tray.desktop`.
//! - macOS: `~/Library/LaunchAgents/com.n8watkins.n8tray.plist` (SMAppService
//!   comes with the macOS phase).

use std::io;
use std::path::Path;
#[cfg(not(windows))]
use std::path::PathBuf;

/// What the OS has now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Autostart {
    /// No entry.
    Off,
    /// The host's entry, starting exactly this command.
    Ours,
    /// The host's entry, starting another command (an older install path):
    /// repairing it is safe.
    Stale(String),
    /// An entry under the host's name that the host did not make.
    Foreign(String),
}

/// Where the entry lives: the real place, or a test location.
#[derive(Debug, Clone)]
pub enum Location {
    /// The OS's place for the current user.
    User,
    /// Windows: a registry key under HKCU (tests). Elsewhere: a folder.
    #[cfg(windows)]
    RegistryKey(String),
    #[cfg(not(windows))]
    Folder(PathBuf),
}

/// The Run key value's name.
#[cfg(windows)]
const VALUE_NAME: &str = "N8 Tray";
/// The marker line in the files the host writes.
#[cfg(not(windows))]
const MARKER: &str = "X-N8Tray-Managed=true";

/// The command line the entry starts: the executable, quoted, then arguments.
pub fn command_for(exe: &Path, args: &[&str]) -> String {
    let mut command = format!("\"{}\"", exe.display());
    for arg in args {
        command.push(' ');
        command.push_str(arg);
    }
    command
}

pub fn state(location: &Location, command: &str) -> io::Result<Autostart> {
    imp::state(location, command)
}

/// Creates or repairs the entry. Refuses to replace a foreign one.
pub fn enable(location: &Location, command: &str) -> io::Result<()> {
    if let Autostart::Foreign(existing) = state(location, command)? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("a sign-in entry the host did not make exists: {existing}"),
        ));
    }
    imp::write(location, command)
}

/// Removes the host's entry; a foreign one is left alone.
pub fn disable(location: &Location, command: &str) -> io::Result<()> {
    match state(location, command)? {
        Autostart::Ours | Autostart::Stale(_) => imp::remove(location),
        Autostart::Off | Autostart::Foreign(_) => Ok(()),
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

    fn key_path(location: &Location) -> &str {
        match location {
            Location::User => RUN_KEY,
            Location::RegistryKey(path) => path,
        }
    }

    pub fn state(location: &Location, command: &str) -> io::Result<Autostart> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = match hkcu.open_subkey_with_flags(key_path(location), KEY_READ) {
            Ok(key) => key,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Autostart::Off),
            Err(e) => return Err(e),
        };
        let value: String = match key.get_value(VALUE_NAME) {
            Ok(value) => value,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Autostart::Off),
            Err(e) => return Err(e),
        };
        Ok(classify(&value, command))
    }

    /// Ours when it is exactly our command; stale when it starts another
    /// n8tray.exe; foreign otherwise.
    fn classify(value: &str, command: &str) -> Autostart {
        if value.eq_ignore_ascii_case(command) {
            return Autostart::Ours;
        }
        let exe = value
            .trim_start_matches('"')
            .split('"')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .replace('/', "\\");
        if exe.ends_with("\\n8tray.exe") || exe.ends_with("\\n8 tray.exe") {
            Autostart::Stale(value.to_string())
        } else {
            Autostart::Foreign(value.to_string())
        }
    }

    pub fn write(location: &Location, command: &str) -> io::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey_with_flags(key_path(location), KEY_READ | KEY_WRITE)?;
        key.set_value(VALUE_NAME, &command.to_string())
    }

    pub fn remove(location: &Location) -> io::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu.open_subkey_with_flags(key_path(location), KEY_READ | KEY_WRITE)?;
        key.delete_value(VALUE_NAME)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn classifies_entries() {
            let ours = "\"C:\\Apps\\N8 Tray\\n8tray.exe\" --autostart";
            assert_eq!(classify(ours, ours), Autostart::Ours);
            assert!(matches!(
                classify("\"C:\\Old\\n8tray.exe\" --autostart", ours),
                Autostart::Stale(_)
            ));
            assert!(matches!(
                classify("C:\\Something\\else.exe", ours),
                Autostart::Foreign(_)
            ));
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    fn file(location: &Location) -> io::Result<PathBuf> {
        let folder = match location {
            Location::Folder(folder) => folder.clone(),
            Location::User => default_folder()?,
        };
        Ok(folder.join(file_name()))
    }

    fn file_name() -> &'static str {
        if cfg!(target_os = "macos") {
            "com.n8watkins.n8tray.plist"
        } else {
            "n8tray.desktop"
        }
    }

    fn default_folder() -> io::Result<PathBuf> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
        Ok(if cfg!(target_os = "macos") {
            home.join("Library/LaunchAgents")
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
                .join("autostart")
        })
    }

    /// The entry's text for a command (the quoted executable and its arguments).
    pub fn render(command: &str) -> String {
        if cfg!(target_os = "macos") {
            let mut arguments = String::new();
            for part in split(command) {
                arguments.push_str(&format!("    <string>{}</string>\n", xml_escape(&part)));
            }
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- {MARKER} -->\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key>\n  <string>com.n8watkins.n8tray</string>\n  <key>ProgramArguments</key>\n  <array>\n{arguments}  </array>\n  <key>RunAtLoad</key>\n  <true/>\n</dict>\n</plist>\n"
            )
        } else {
            format!(
                "[Desktop Entry]\nType=Application\nName=N8 Tray\nComment=One tray for local apps\nExec={command}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n{MARKER}\n"
            )
        }
    }

    /// Splits a command into the quoted executable and the plain arguments after it.
    fn split(command: &str) -> Vec<String> {
        let mut parts = Vec::new();
        let rest = if let Some(stripped) = command.strip_prefix('"') {
            let end = stripped.find('"').unwrap_or(stripped.len());
            parts.push(stripped[..end].to_string());
            stripped.get(end + 1..).unwrap_or("")
        } else {
            command
        };
        parts.extend(rest.split_whitespace().map(str::to_string));
        parts
    }

    fn xml_escape(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    pub fn state(location: &Location, command: &str) -> io::Result<Autostart> {
        let path = file(location)?;
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Autostart::Off),
            Err(e) => return Err(e),
        };
        Ok(if !text.contains(MARKER) {
            Autostart::Foreign(path.display().to_string())
        } else if text == render(command) {
            Autostart::Ours
        } else {
            Autostart::Stale(path.display().to_string())
        })
    }

    pub fn write(location: &Location, command: &str) -> io::Result<()> {
        let path = file(location)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, render(command))
    }

    pub fn remove(location: &Location) -> io::Result<()> {
        std::fs::remove_file(file(location)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_location() -> (Location, Option<tempfile::TempDir>) {
        #[cfg(windows)]
        {
            (
                Location::RegistryKey(format!("Software\\N8TrayTests\\Run{}", std::process::id())),
                None,
            )
        }
        #[cfg(not(windows))]
        {
            let dir = tempfile::tempdir().unwrap();
            (Location::Folder(dir.path().to_path_buf()), Some(dir))
        }
    }

    #[test]
    fn enable_repair_and_disable() {
        let (location, _dir) = test_location();
        let command = command_for(Path::new("/apps/N8 Tray/n8tray.exe"), &["--autostart"]);
        assert_eq!(command, "\"/apps/N8 Tray/n8tray.exe\" --autostart");
        assert_eq!(state(&location, &command).unwrap(), Autostart::Off);
        enable(&location, &command).unwrap();
        assert_eq!(state(&location, &command).unwrap(), Autostart::Ours);

        let moved = command_for(Path::new("/elsewhere/n8tray.exe"), &["--autostart"]);
        assert!(matches!(
            state(&location, &moved).unwrap(),
            Autostart::Stale(_)
        ));
        enable(&location, &moved).unwrap();
        assert_eq!(
            state(&location, &moved).unwrap(),
            Autostart::Ours,
            "repaired"
        );

        disable(&location, &moved).unwrap();
        assert_eq!(state(&location, &moved).unwrap(), Autostart::Off);
        disable(&location, &moved).unwrap();
        #[cfg(windows)]
        if let Location::RegistryKey(path) = &location {
            let _ =
                winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).delete_subkey_all(path);
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn a_foreign_entry_is_left_alone() {
        let (location, dir) = test_location();
        let dir = dir.unwrap();
        let name = if cfg!(target_os = "macos") {
            "com.n8watkins.n8tray.plist"
        } else {
            "n8tray.desktop"
        };
        std::fs::write(dir.path().join(name), "made by hand").unwrap();
        let command = command_for(Path::new("/apps/n8tray"), &[]);
        assert!(matches!(
            state(&location, &command).unwrap(),
            Autostart::Foreign(_)
        ));
        assert!(enable(&location, &command).is_err());
        disable(&location, &command).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join(name)).unwrap(),
            "made by hand"
        );
    }
}
