//! Paths in the contract: relative paths inside `installDir`, the fixed list
//! of placeholders (spec 3.2) and the folders of spec section 2.

use std::path::PathBuf;

/// The placeholders a path may start with (spec 3.2), and no others.
pub const PLACEHOLDERS: [&str; 5] = [
    "%LOCALAPPDATA%",
    "%USERPROFILE%",
    "%HOME%",
    "%N8TRAY_APPS%",
    "%N8TRAY_TOOLS%",
];

/// Why a relative path is not acceptable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativePathError {
    /// It starts with `/` or `\`.
    Rooted,
    /// It has a drive letter (`C:`) or a UNC prefix.
    Drive,
    /// It has a `..` segment.
    ParentSegment,
}

impl std::fmt::Display for RelativePathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Rooted => "starts with a separator",
            Self::Drive => "has a drive letter",
            Self::ParentSegment => "has a \"..\" segment",
        })
    }
}

/// Checks a path that must resolve inside `installDir` (spec 3.2).
pub fn check_relative(path: &str) -> Result<(), RelativePathError> {
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(RelativePathError::Rooted);
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(RelativePathError::Drive);
    }
    if path.split(['/', '\\']).any(|segment| segment == "..") {
        return Err(RelativePathError::ParentSegment);
    }
    Ok(())
}

/// Why a path with placeholders is not acceptable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaceholderError {
    /// A `%NAME%` that is not on the list.
    Unknown(String),
    /// A placeholder that is not at the very start.
    NotAtStart(String),
}

impl std::fmt::Display for PlaceholderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(name) => write!(f, "{name} is not one of {}", PLACEHOLDERS.join(", ")),
            Self::NotAtStart(name) => write!(f, "{name} may only start a path"),
        }
    }
}

/// Splits a path into its leading placeholder (if any) and the rest, and
/// rejects unknown placeholders and placeholders anywhere but the start.
pub fn split_placeholder(path: &str) -> Result<(Option<&'static str>, &str), PlaceholderError> {
    let mut leading = None;
    let mut rest = path;
    if let Some(known) = PLACEHOLDERS.iter().find(|p| path.starts_with(**p)) {
        leading = Some(*known);
        rest = &path[known.len()..];
    }
    if let Some(name) = find_placeholder(rest) {
        return Err(if PLACEHOLDERS.contains(&name) {
            PlaceholderError::NotAtStart(name.to_string())
        } else {
            PlaceholderError::Unknown(name.to_string())
        });
    }
    Ok((leading, rest))
}

/// The first `%NAME%` in `text`, where NAME is letters, digits and `_`.
fn find_placeholder(text: &str) -> Option<&str> {
    let mut search = 0;
    while let Some(start) = text[search..].find('%').map(|i| i + search) {
        let tail = &text[start + 1..];
        if let Some(end) = tail.find('%') {
            let name = &tail[..end];
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Some(&text[start..start + end + 2]);
            }
            search = start + 1;
        } else {
            return None;
        }
    }
    None
}

/// Whether a path is absolute on some OS: `/x`, `C:\x`, `C:/x` or `\\server\share`.
pub fn is_absolute_anywhere(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/'))
}

/// The runtime folder (spec section 2): `N8TRAY_RUN_DIR` when set, else the
/// per-OS default, or `None` on Linux without `XDG_RUNTIME_DIR`.
pub fn run_dir() -> Option<PathBuf> {
    if let Some(dir) = env_path("N8TRAY_RUN_DIR") {
        return Some(dir);
    }
    if cfg!(windows) {
        local_app_data().map(|d| d.join("N8Tray").join("run"))
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support/N8Tray/run"))
    } else {
        env_path("XDG_RUNTIME_DIR").map(|d| d.join("n8tray"))
    }
}

/// The modules folder (spec section 2): `N8TRAY_MODULES_DIR` when set, else
/// the per-OS default.
pub fn modules_dir() -> Option<PathBuf> {
    if let Some(dir) = env_path("N8TRAY_MODULES_DIR") {
        return Some(dir);
    }
    if cfg!(windows) {
        local_app_data().map(|d| d.join("N8Tray").join("modules"))
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support/N8Tray/modules"))
    } else {
        env_path("XDG_DATA_HOME")
            .or_else(|| home().map(|h| h.join(".local/share")))
            .map(|d| d.join("n8tray").join("modules"))
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn local_app_data() -> Option<PathBuf> {
    env_path("LOCALAPPDATA").or_else(|| home().map(|h| h.join("AppData").join("Local")))
}

fn home() -> Option<PathBuf> {
    env_path("HOME").or_else(|| env_path("USERPROFILE"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths() {
        assert!(check_relative("bin/app.exe").is_ok());
        assert!(check_relative("logs\\").is_ok());
        assert!(check_relative(".").is_ok());
        assert!(check_relative("a..b/c").is_ok());
        assert_eq!(check_relative("/etc/x"), Err(RelativePathError::Rooted));
        assert_eq!(check_relative("\\x"), Err(RelativePathError::Rooted));
        assert_eq!(check_relative("C:/x"), Err(RelativePathError::Drive));
        assert_eq!(check_relative("c:x"), Err(RelativePathError::Drive));
        assert_eq!(
            check_relative("a/../../x"),
            Err(RelativePathError::ParentSegment)
        );
        assert_eq!(
            check_relative("a\\..\\x"),
            Err(RelativePathError::ParentSegment)
        );
    }

    #[test]
    fn placeholders() {
        assert_eq!(
            split_placeholder("%HOME%/apps"),
            Ok((Some("%HOME%"), "/apps"))
        );
        assert_eq!(split_placeholder("C:\\apps"), Ok((None, "C:\\apps")));
        assert_eq!(split_placeholder("100% sure"), Ok((None, "100% sure")));
        assert_eq!(
            split_placeholder("%APPDATA%\\x"),
            Err(PlaceholderError::Unknown("%APPDATA%".into()))
        );
        assert_eq!(
            split_placeholder("C:\\%HOME%\\x"),
            Err(PlaceholderError::NotAtStart("%HOME%".into()))
        );
        assert_eq!(
            split_placeholder("%HOME%/%HOME%"),
            Err(PlaceholderError::NotAtStart("%HOME%".into()))
        );
        assert_eq!(
            split_placeholder("%HOME%/%FOO%"),
            Err(PlaceholderError::Unknown("%FOO%".into()))
        );
    }

    #[test]
    fn absolute_anywhere() {
        assert!(is_absolute_anywhere("/opt/x"));
        assert!(is_absolute_anywhere("C:\\x"));
        assert!(is_absolute_anywhere("d:/x"));
        assert!(is_absolute_anywhere("\\\\server\\share"));
        assert!(!is_absolute_anywhere("apps/x"));
        assert!(!is_absolute_anywhere("C:x"));
    }
}
