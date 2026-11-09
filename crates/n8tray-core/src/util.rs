//! Small helpers: atomic writes and timestamps.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use chrono::{DateTime, SecondsFormat, Utc};

/// Writes `bytes` to `path` atomically: a temporary file in the same folder,
/// then a rename over the old file. UTF-8 text is written without a BOM.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let temp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    {
        let mut file = fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
    }
    // Windows can refuse a rename over a file a reader has open; retry briefly.
    let mut attempt = 0;
    loop {
        match fs::rename(&temp, path) {
            Ok(()) => return Ok(()),
            Err(e) if attempt < 5 && cfg!(windows) => {
                attempt += 1;
                let _ = e;
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => {
                let _ = fs::remove_file(&temp);
                return Err(e);
            }
        }
    }
}

/// A timestamp as the contract writes them: UTC, milliseconds, `Z`.
pub fn timestamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Whether two creation times are the same process's: within 1 s (spec 7.2),
/// with a millisecond of slack for rounding.
pub fn same_start(a: DateTime<Utc>, b: DateTime<Utc>) -> bool {
    (a - b).num_milliseconds().abs() <= 1001
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_writes_leave_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("x.json");
        write_atomic(&path, b"1").unwrap();
        write_atomic(&path, b"2").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "2");
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn start_times_match_within_a_second() {
        let t = Utc::now();
        assert!(same_start(t, t + chrono::Duration::milliseconds(999)));
        assert!(same_start(t, t - chrono::Duration::milliseconds(1000)));
        assert!(!same_start(t, t + chrono::Duration::milliseconds(1500)));
    }
}
