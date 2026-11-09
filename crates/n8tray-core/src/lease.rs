//! The host's side of the lease (spec section 8): `host.json` every 5 s,
//! atomically, never deleted; and `host.lock`, so one host runs at a time.
//!
//! The document is the one the PowerShell tray writes since 0.26.0, so every
//! app that reads that lease today reads this one.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use n8tray_contract::lease::{FILE_NAME, Lease, WRITE_INTERVAL_SECONDS};

use crate::util::write_atomic;

pub struct LeaseWriter {
    path: PathBuf,
    pid: u32,
    started_at: DateTime<Utc>,
    last: Option<DateTime<Utc>>,
    last_hosting: Vec<String>,
}

impl LeaseWriter {
    pub fn new(run_dir: &Path, pid: u32, started_at: DateTime<Utc>) -> LeaseWriter {
        LeaseWriter {
            path: run_dir.join(FILE_NAME),
            pid,
            started_at,
            last: None,
            last_hosting: Vec::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The lease as it would be written now.
    pub fn lease(&self, now: DateTime<Utc>, hosting: &[String]) -> Lease {
        Lease {
            contract: n8tray_contract::CONTRACT,
            host: "n8tray".into(),
            version: crate::VERSION.into(),
            pid: self.pid,
            pid_started_at: self.started_at,
            heartbeat: now,
            hosting: hosting.to_vec(),
        }
    }

    /// Writes the lease when 5 s passed since the last write, or at once when
    /// the hosted apps changed. Returns whether it wrote.
    pub fn tick(&mut self, now: DateTime<Utc>, hosting: &[String]) -> io::Result<bool> {
        let due = match self.last {
            None => true,
            Some(last) => {
                (now - last).num_milliseconds() >= (WRITE_INTERVAL_SECONDS * 1000) as i64
                    || hosting != self.last_hosting.as_slice()
            }
        };
        if !due {
            return Ok(false);
        }
        let lease = self.lease(now, hosting);
        let value = serde_json::json!({
            "contract": lease.contract,
            "host": lease.host,
            "version": lease.version,
            "pid": lease.pid,
            "pidStartedAt": crate::util::timestamp(lease.pid_started_at),
            "heartbeat": crate::util::timestamp(lease.heartbeat),
            "hosting": lease.hosting,
        });
        let mut text = serde_json::to_string_pretty(&value).map_err(io::Error::other)?;
        text.push('\n');
        write_atomic(&self.path, text.as_bytes())?;
        self.last = Some(now);
        self.last_hosting = hosting.to_vec();
        Ok(true)
    }
}

/// `host.lock` in the runtime folder, held for the host's whole run.
pub struct HostLock {
    _file: File,
}

impl HostLock {
    /// Takes the lock; `Ok(None)` when another host holds it.
    pub fn acquire(run_dir: &Path) -> io::Result<Option<HostLock>> {
        std::fs::create_dir_all(run_dir)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(run_dir.join("host.lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(HostLock { _file: file })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(e)) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use n8tray_contract::validate::{Kind, validate_as};

    #[test]
    fn writes_a_valid_lease_on_schedule() {
        let dir = tempfile::tempdir().unwrap();
        let start = Utc::now();
        let mut writer = LeaseWriter::new(dir.path(), 1234, start);
        let hosting = vec!["voicedock".to_string()];
        assert!(writer.tick(start, &hosting).unwrap());
        assert!(
            !writer
                .tick(start + chrono::Duration::seconds(2), &hosting)
                .unwrap()
        );
        assert!(
            writer
                .tick(start + chrono::Duration::seconds(3), &[])
                .unwrap(),
            "a change in hosting is written at once"
        );
        assert!(
            writer
                .tick(start + chrono::Duration::seconds(8), &[])
                .unwrap()
        );

        let text = std::fs::read_to_string(dir.path().join("host.json")).unwrap();
        let value = n8tray_contract::parse_json(&text).unwrap();
        let report = validate_as(Kind::Lease, &value, Some("host.json"));
        assert!(report.is_valid(), "{:?}", report.issues);
        let lease = n8tray_contract::lease::parse(&text).unwrap();
        assert_eq!(lease.pid, 1234);
        assert!(lease.is_fresh(start + chrono::Duration::seconds(10)));
    }

    #[test]
    fn one_host_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let first = HostLock::acquire(dir.path()).unwrap();
        assert!(first.is_some());
        assert!(HostLock::acquire(dir.path()).unwrap().is_none());
        drop(first);
        assert!(HostLock::acquire(dir.path()).unwrap().is_some());
    }
}
