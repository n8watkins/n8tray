//! Until the switch the native shell runs only in a home of its own
//! (`N8TRAY_HOME`), never in the installed N8 Tray's: there it would become
//! the host of the user's real apps, write their status, history and lease,
//! and the installed app would find the host lock taken at sign-in (D33).
//! This is checked before anything is written, the log included.
#![forbid(unsafe_code)]

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Why the native shell does not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// `N8TRAY_HOME` is not set (or empty).
    NotSet,
    /// `N8TRAY_HOME` is the installed N8 Tray's own folder.
    RealHome(PathBuf),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::NotSet => f.write_str(
                "n8tray-native runs only in a home of its own until it replaces N8 Tray: set N8TRAY_HOME to an empty folder (nothing was started or written)",
            ),
            Refusal::RealHome(path) => write!(
                f,
                "N8TRAY_HOME is the installed N8 Tray's own folder ({}); n8tray-native needs a home of its own (nothing was started or written)",
                path.display()
            ),
        }
    }
}

/// The installed N8 Tray's own folder (its settings, modules and run
/// folders are under it on Windows and macOS; D16).
pub fn real_base() -> Option<PathBuf> {
    let env = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(windows) {
        env("LOCALAPPDATA").map(|d| d.join("N8Tray"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support/N8Tray"))
    } else {
        env("XDG_DATA_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".local/share")))
            .map(|d| d.join("n8tray"))
    }
}

/// The isolated home to run in, from `N8TRAY_HOME`'s value, or why not.
pub fn isolated_home(value: Option<&OsStr>, real: Option<&Path>) -> Result<PathBuf, Refusal> {
    let value = value.filter(|v| !v.is_empty()).ok_or(Refusal::NotSet)?;
    let home = std::path::absolute(Path::new(value)).unwrap_or_else(|_| PathBuf::from(value));
    if let Some(real) = real
        && same_folder(&home, real)
    {
        return Err(Refusal::RealHome(home));
    }
    Ok(home)
}

/// Whether two paths name one folder: through links when both exist, else
/// by their text, without case and trailing separators (Windows).
fn same_folder(a: &Path, b: &Path) -> bool {
    if let (Ok(a), Ok(b)) = (a.canonicalize(), b.canonicalize()) {
        return a == b;
    }
    let text = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    text(a) == text(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_home_is_refused() {
        assert_eq!(isolated_home(None, None), Err(Refusal::NotSet));
        assert_eq!(
            isolated_home(Some(OsStr::new("")), None),
            Err(Refusal::NotSet)
        );
        assert!(Refusal::NotSet.to_string().contains("N8TRAY_HOME"));
    }

    #[test]
    fn the_installed_apps_folder_is_refused_however_it_is_written() {
        // Not created: compared by its text.
        let real = std::env::temp_dir().join(format!("N8Tray-real-{}", std::process::id()));
        let text = real.to_string_lossy().to_string();
        for given in [
            text.clone(),
            format!("{text}{}", std::path::MAIN_SEPARATOR),
            text.to_uppercase(),
        ] {
            assert!(
                matches!(
                    isolated_home(Some(OsStr::new(&given)), Some(&real)),
                    Err(Refusal::RealHome(_))
                ),
                "{given}"
            );
        }
    }

    #[test]
    fn an_existing_real_folder_is_refused_through_another_spelling() {
        let dir = std::env::temp_dir().join(format!("n8tray-native-home-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let roundabout = dir.join("sub").join("..");
        let got = isolated_home(Some(roundabout.as_os_str()), Some(&dir));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(matches!(got, Err(Refusal::RealHome(_))), "{got:?}");
    }

    #[test]
    fn a_home_of_its_own_is_used_as_an_absolute_path() {
        let real = Path::new("/real/N8Tray");
        let got = isolated_home(Some(OsStr::new("test-home")), Some(real)).unwrap();
        assert!(got.is_absolute(), "{got:?}");
        assert!(got.ends_with("test-home"));
        // Under the real folder is a folder of its own.
        assert!(isolated_home(Some(OsStr::new("/real/N8Tray/trial")), Some(real)).is_ok());
    }
}
