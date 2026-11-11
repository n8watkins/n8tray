//! The update channel skeleton (plan section 10): which feed to read for the
//! stable or beta channel, when to look, and whether the feed names a newer
//! version. Installing is the app's job (Tauri's updater, which checks the
//! update's signature against N8 Tray's public key); the host keeps the
//! install's progress for the snapshot. Nothing is downloaded without a click.

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

use crate::settings::Channel;

/// How often to look, after the first look at start.
pub const INTERVAL_HOURS: i64 = 24;
/// The first look waits this long after start, so it never slows sign-in.
pub const FIRST_DELAY_SECONDS: i64 = 60;

pub const RELEASES_PAGE: &str = "https://github.com/n8watkins/n8tray/releases";

/// Replaces the feed of every channel, for tests: an update served from this
/// computer. The update must still carry a signature made with the key whose
/// public half is built into N8 Tray, so the override cannot install anything
/// unsigned, and release builds read only HTTPS feeds.
pub const FEED_ENV: &str = "N8TRAY_UPDATE_FEED";

/// The feed for a channel, in the format Tauri's updater reads, or the one
/// `N8TRAY_UPDATE_FEED` names.
pub fn feed_url(channel: Channel) -> String {
    match std::env::var(FEED_ENV) {
        Ok(url) if !url.trim().is_empty() => url.trim().to_string(),
        _ => default_feed_url(channel).to_string(),
    }
}

/// The published feed for a channel.
pub fn default_feed_url(channel: Channel) -> &'static str {
    match channel {
        Channel::Stable => {
            "https://github.com/n8watkins/n8tray/releases/latest/download/latest.json"
        }
        Channel::Beta => "https://github.com/n8watkins/n8tray/releases/download/beta/beta.json",
    }
}

/// The part of the feed this check reads.
#[derive(Debug, Deserialize)]
struct Feed {
    version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A newer version exists.
    Available {
        version: String,
        url: String,
    },
    UpToDate,
    /// The channel has no feed yet.
    NoFeed,
}

/// Reads a feed's text and compares it with `current`.
pub fn compare(status: u16, body: &str, current: &str) -> Result<Outcome, String> {
    if status == 404 {
        return Ok(Outcome::NoFeed);
    }
    if status != 200 {
        return Err(format!("the update feed answered HTTP {status}"));
    }
    let feed: Feed =
        serde_json::from_str(body).map_err(|e| format!("the update feed is unreadable: {e}"))?;
    let latest = semver::Version::parse(feed.version.trim_start_matches('v'))
        .map_err(|e| format!("the update feed's version is unreadable: {e}"))?;
    let current = semver::Version::parse(current).map_err(|e| e.to_string())?;
    Ok(if latest > current {
        Outcome::Available {
            url: format!("{RELEASES_PAGE}/tag/v{latest}"),
            version: latest.to_string(),
        }
    } else {
        Outcome::UpToDate
    })
}

/// Where installing an update stands. The app reports it to the host, which
/// shows it in the snapshot, the window and the tray menu.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Install {
    #[default]
    Idle,
    /// Downloading; the percentage once the size is known.
    Downloading(Option<u8>),
    /// Downloaded and verified; the installer runs and N8 Tray restarts.
    Installing,
    /// The last try failed, with why.
    Failed(String),
}

impl Install {
    /// The name the window reads: `idle`, `downloading`, `installing` or `failed`.
    pub fn name(&self) -> &'static str {
        match self {
            Install::Idle => "idle",
            Install::Downloading(_) => "downloading",
            Install::Installing => "installing",
            Install::Failed(_) => "failed",
        }
    }

    pub fn busy(&self) -> bool {
        matches!(self, Install::Downloading(_) | Install::Installing)
    }
}

/// The download's percentage from the bytes so far and the total, if known.
pub fn percent(done: u64, total: Option<u64>) -> Option<u8> {
    match total {
        Some(total) if total > 0 => Some((done.min(total) * 100 / total) as u8),
        _ => None,
    }
}

/// When the next look is due.
pub fn next_check(started: DateTime<Utc>, last: Option<DateTime<Utc>>) -> DateTime<Utc> {
    match last {
        None => started + Duration::seconds(FIRST_DELAY_SECONDS),
        Some(last) => last + Duration::hours(INTERVAL_HOURS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_have_their_own_feed() {
        assert!(default_feed_url(Channel::Stable).ends_with("/latest.json"));
        assert!(default_feed_url(Channel::Beta).ends_with("/beta.json"));
        assert!(default_feed_url(Channel::Stable).starts_with("https://"));
    }

    #[test]
    fn install_progress() {
        assert_eq!(percent(0, Some(200)), Some(0));
        assert_eq!(percent(100, Some(200)), Some(50));
        assert_eq!(percent(300, Some(200)), Some(100));
        assert_eq!(percent(100, None), None);
        assert_eq!(percent(100, Some(0)), None);
        assert!(Install::Downloading(None).busy());
        assert!(Install::Installing.busy());
        assert!(!Install::Failed("x".into()).busy());
        assert_eq!(Install::default().name(), "idle");
    }

    #[test]
    fn compares_versions() {
        let feed = r#"{ "version": "v1.2.0", "platforms": {} }"#;
        assert_eq!(
            compare(200, feed, "1.1.9").unwrap(),
            Outcome::Available {
                version: "1.2.0".into(),
                url: format!("{RELEASES_PAGE}/tag/v1.2.0")
            }
        );
        assert_eq!(compare(200, feed, "1.2.0").unwrap(), Outcome::UpToDate);
        assert_eq!(compare(404, "", "1.0.0").unwrap(), Outcome::NoFeed);
        assert!(compare(500, "", "1.0.0").is_err());
        assert!(compare(200, "{", "1.0.0").is_err());
    }

    #[test]
    fn schedule() {
        let start = Utc::now();
        assert_eq!(next_check(start, None), start + Duration::seconds(60));
        assert_eq!(next_check(start, Some(start)), start + Duration::hours(24));
    }
}
