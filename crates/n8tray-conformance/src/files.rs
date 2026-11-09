//! Atomic writes, tokens and the runtime folder (spec sections 2, 5.2 and 9).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Writes `bytes` to `path` atomically: a temporary file in the same folder,
/// then a rename over the old file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let temp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    {
        let mut file = fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    restrict(&temp)?;
    fs::rename(&temp, path)
}

/// Makes a file readable and writable by the current user only (Unix: 0600).
/// On Windows the runtime folder's ACL does this (spec section 2).
pub fn restrict(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    let _ = path;
    Ok(())
}

/// Creates the runtime folder if needed, user-only on Unix (0700).
pub fn ensure_run_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Whether a file is readable by others than its owner (Unix only; always
/// `false` on Windows, where the folder's ACL is what counts).
pub fn readable_by_others(path: &Path) -> io::Result<bool> {
    let metadata = fs::metadata(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(metadata.permissions().mode() & 0o077 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        Ok(false)
    }
}

/// A new token: 32 random bytes as 64 lowercase hex characters (spec section 9).
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS random source works");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether a token file's content is a well-formed token.
pub fn is_token(text: &str) -> bool {
    let token = text.trim_end_matches(['\r', '\n']);
    token.len() == 64
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Compares two strings in time that depends only on their lengths.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// `<run dir>/<id>.token`.
pub fn token_path(run_dir: &Path, id: &str) -> PathBuf {
    run_dir.join(format!("{id}.token"))
}

/// `<run dir>/<id>.status.json`.
pub fn status_path(run_dir: &Path, id: &str) -> PathBuf {
    run_dir.join(format!("{id}.status.json"))
}

/// `<run dir>/host.json`.
pub fn lease_path(run_dir: &Path) -> PathBuf {
    run_dir.join(n8tray_contract::lease::FILE_NAME)
}

/// The runtime folder: `explicit`, else `N8TRAY_RUN_DIR` or the per-OS default.
pub fn run_dir(explicit: Option<&Path>) -> io::Result<PathBuf> {
    explicit
        .map(Path::to_path_buf)
        .or_else(n8tray_contract::paths::run_dir)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no runtime folder: set N8TRAY_RUN_DIR or pass --run-dir",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        let token = new_token();
        assert!(is_token(&token));
        assert!(is_token(&format!("{token}\n")));
        assert!(!is_token(&token.to_uppercase()));
        assert!(!is_token(&token[1..]));
        assert_ne!(token, new_token());
    }

    #[test]
    fn constant_time_comparison() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
    }

    #[test]
    fn atomic_write_replaces_and_restricts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.json");
        write_atomic(&path, b"1").unwrap();
        write_atomic(&path, b"2").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "2");
        assert!(!readable_by_others(&path).unwrap());
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no temporary file is left behind"
        );
    }
}
