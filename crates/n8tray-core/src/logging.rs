//! Log hygiene: status changes are logged once, and the same error at most
//! once per 5 minutes (the PowerShell tray's rules), so the log stays readable
//! during a long outage.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};

/// Says whether a message is due, per key.
#[derive(Debug, Default)]
pub struct Limiter {
    last: HashMap<String, (DateTime<Utc>, String)>,
}

impl Limiter {
    pub const WINDOW_MINUTES: i64 = 5;

    /// True when `message` under `key` was not logged in the last 5 minutes,
    /// or differs from what was.
    pub fn due(&mut self, key: &str, message: &str, now: DateTime<Utc>) -> bool {
        match self.last.get(key) {
            Some((at, last))
                if last == message && now - *at < Duration::minutes(Self::WINDOW_MINUTES) =>
            {
                false
            }
            _ => {
                self.last
                    .insert(key.to_string(), (now, message.to_string()));
                true
            }
        }
    }

    /// Forgets a key, so its next message is logged (the problem went away).
    pub fn clear(&mut self, key: &str) {
        self.last.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn once_per_five_minutes() {
        let mut limiter = Limiter::default();
        let now = Utc::now();
        assert!(limiter.due("a", "boom", now));
        assert!(!limiter.due("a", "boom", now + Duration::minutes(4)));
        assert!(limiter.due("a", "other", now + Duration::minutes(4)));
        assert!(limiter.due("b", "boom", now));
        assert!(limiter.due("a", "other", now + Duration::minutes(10)));
        limiter.clear("b");
        assert!(limiter.due("b", "boom", now));
    }
}
