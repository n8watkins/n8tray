//! Folders only the current user can read (spec section 2).

use std::io;
use std::path::Path;

/// Creates a folder if needed and makes it readable by the current user only:
/// mode 0700 on Unix; on Windows, when this call created it, an ACL with only
/// the user and SYSTEM (inheritance from the parent removed).
pub fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    let existed = dir.exists();
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = existed;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(windows)]
    {
        if !existed {
            restrict_windows(dir)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn restrict_windows(dir: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let icacls = Path::new(&system_root).join("System32").join("icacls.exe");
    let user = std::env::var("USERNAME").map_err(io::Error::other)?;
    let status = std::process::Command::new(icacls)
        .arg(dir)
        .args(["/inheritance:r", "/grant:r"])
        .arg(format!("{user}:(OI)(CI)F"))
        .args(["/grant:r", "*S-1-5-18:(OI)(CI)F"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "icacls could not restrict {}",
            dir.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn creates_a_private_folder() {
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("run");
        super::ensure_private_dir(&dir).unwrap();
        assert!(dir.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        super::ensure_private_dir(&dir).unwrap();
    }
}
