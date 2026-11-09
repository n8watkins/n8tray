//! The update channel skeleton (plan section 10): which feed to read for the
//! stable or beta channel, when to look, and whether the feed names a newer
//! version. Installing comes later (Phase 5); today "Update available" opens
//! the release page, and nothing is downloaded without a click.

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

use crate::settings::Channel;

/// How often to look, after the first look at start.
pub const INTERVAL_HOURS: i64 = 24;
/// The first look waits this long after start, so it never slows sign-in.
pub const FIRST_DELAY_SECONDS: i64 = 60;

pub const RELEASES_PAGE: &str = "https://github.com/n8watkins/n8tray/releases";

/// The feed for a channel, in the format Tauri's updater reads.
pub fn feed_url(channel: Channel) -> &'static str {
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
        assert!(feed_url(Channel::Stable).ends_with("/latest.json"));
        assert!(feed_url(Channel::Beta).ends_with("/beta.json"));
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
