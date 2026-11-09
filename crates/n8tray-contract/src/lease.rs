//! The host lease (spec section 8), as N8 Tray 0.26.0 and later writes it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// How often the host rewrites the lease.
pub const WRITE_INTERVAL_SECONDS: u64 = 5;
/// A lease is fresh while its heartbeat is under this far from now, in either direction.
pub const FRESH_SECONDS: i64 = 15;
/// At start, how long an app waits for a stale lease that lists it to become fresh.
pub const START_GRACE_SECONDS: i64 = 30;
/// The lease's file name in the runtime folder.
pub const FILE_NAME: &str = "host.json";

/// The host lease.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lease {
    pub contract: u32,
    pub host: String,
    pub version: String,
    pub pid: u32,
    pub pid_started_at: DateTime<Utc>,
    pub heartbeat: DateTime<Utc>,
    pub hosting: Vec<String>,
}

/// What an app's own tray should be (spec 8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayMode {
    /// No icon: the host shows the app.
    Hosted,
    /// Not shown yet: at start, a stale lease that lists the app gets the grace period.
    Waiting,
    /// The app's own icon.
    Own,
}

impl Lease {
    /// Whether the heartbeat is within [`FRESH_SECONDS`] of `now`, in either direction.
    pub fn is_fresh(&self, now: DateTime<Utc>) -> bool {
        (now - self.heartbeat).num_milliseconds().abs() <= FRESH_SECONDS * 1000
    }

    /// Whether the lease lists `app_id`.
    pub fn lists(&self, app_id: &str) -> bool {
        self.hosting.iter().any(|id| id == app_id)
    }

    /// Whether the lease hosts `app_id` now: listed, fresh and the host alive.
    /// `host_alive` gets the pid and its expected start time.
    pub fn hosts(
        &self,
        app_id: &str,
        now: DateTime<Utc>,
        host_alive: impl Fn(u32, DateTime<Utc>) -> bool,
    ) -> bool {
        self.lists(app_id) && self.is_fresh(now) && host_alive(self.pid, self.pid_started_at)
    }
}

/// Reads a lease from the file's text: `None` for anything that is not a
/// lease (missing fields, half-written, not an object), as spec 8.2 requires.
pub fn parse(text: &str) -> Option<Lease> {
    serde_json::from_str(crate::strip_bom(text)).ok()
}

/// The tray mode for an app (spec 8.2). `settled` is true once the app has
/// been hosted or has shown its icon; `started_at` is when the app started.
pub fn tray_mode(
    lease: Option<&Lease>,
    app_id: &str,
    now: DateTime<Utc>,
    started_at: DateTime<Utc>,
    settled: bool,
    host_alive: impl Fn(u32, DateTime<Utc>) -> bool,
) -> TrayMode {
    match lease {
        Some(lease) if lease.hosts(app_id, now, host_alive) => TrayMode::Hosted,
        Some(lease)
            if !settled
                && lease.lists(app_id)
                && (now - started_at).num_seconds() < START_GRACE_SECONDS =>
        {
            TrayMode::Waiting
        }
        _ => TrayMode::Own,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn lease(heartbeat: DateTime<Utc>) -> Lease {
        Lease {
            contract: 2,
            host: "n8tray".into(),
            version: "0.27.0".into(),
            pid: 9124,
            pid_started_at: heartbeat - Duration::hours(1),
            heartbeat,
            hosting: vec!["voicedock".into()],
        }
    }

    #[test]
    fn freshness_matches_voicedocks_reader() {
        let now = Utc::now();
        let alive = |_, _| true;
        assert!(lease(now - Duration::seconds(14)).hosts("voicedock", now, alive));
        assert!(!lease(now - Duration::seconds(16)).hosts("voicedock", now, alive));
        assert!(
            lease(now + Duration::seconds(10)).hosts("voicedock", now, alive),
            "clock noise counts as fresh"
        );
        assert!(!lease(now + Duration::seconds(16)).hosts("voicedock", now, alive));
        assert!(
            !lease(now).hosts("voicedock", now, |_, _| false),
            "a dead host hosts nothing"
        );
        assert!(!lease(now).hosts("n8hotkeys", now, alive), "not listed");
    }

    #[test]
    fn tray_modes() {
        let now = Utc::now();
        let alive = |_, _| true;
        let fresh = lease(now);
        let stale = lease(now - Duration::seconds(60));
        assert_eq!(
            tray_mode(Some(&fresh), "voicedock", now, now, false, alive),
            TrayMode::Hosted
        );
        assert_eq!(
            tray_mode(Some(&stale), "voicedock", now, now, false, alive),
            TrayMode::Waiting
        );
        let later = now + Duration::seconds(31);
        assert_eq!(
            tray_mode(Some(&stale), "voicedock", later, now, false, alive),
            TrayMode::Own
        );
        assert_eq!(
            tray_mode(Some(&stale), "voicedock", now, now, true, alive),
            TrayMode::Own,
            "never waits again"
        );
        assert_eq!(
            tray_mode(None, "voicedock", now, now, false, alive),
            TrayMode::Own
        );
        assert_eq!(
            tray_mode(Some(&stale), "n8hotkeys", now, now, false, alive),
            TrayMode::Own
        );
    }

    #[test]
    fn parse_tolerates_bom_and_rejects_half_written() {
        let text = r#"{"contract":2,"host":"n8tray","version":"0.27.0","pid":1,"pidStartedAt":"2026-11-09T08:01:40.118Z","heartbeat":"2026-11-09T18:04:10.002Z","hosting":[]}"#;
        assert!(parse(&format!("\u{feff}{text}")).is_some());
        assert!(parse(&text[..text.len() - 10]).is_none());
        assert!(parse("[]").is_none());
        assert!(parse("").is_none());
    }
}
