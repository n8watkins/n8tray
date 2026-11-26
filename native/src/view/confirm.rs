//! Actions that ask for a second click within 4 seconds (today's
//! ActionButton): which are armed, held by app, action and component, so a
//! refresh or a reordered list neither drops the armed state nor moves it to
//! another app. The window keeps one `Confirms` and a timer for
//! `next_expiry`; views ask `is_armed` when they fill and `press` on a click.
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long an armed action waits for its second click.
pub const WINDOW: Duration = Duration::from_secs(4);

/// What armed an action: the app, the action, and the component (or "").
pub type Key = (String, String, String);

pub fn key(app: &str, action: &str, component: Option<&str>) -> Key {
    (app.into(), action.into(), component.unwrap_or("").into())
}

/// What a click does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// The first click of an action that asks to confirm: it is armed now.
    Armed,
    /// Run the action.
    Run,
}

#[derive(Debug, Default)]
pub struct Confirms {
    armed: HashMap<Key, Instant>,
}

impl Confirms {
    /// A click on an action; `confirm` says whether it asks to.
    pub fn press(&mut self, key: Key, confirm: bool, now: Instant) -> Press {
        self.expire(now);
        if !confirm {
            return Press::Run;
        }
        if self.armed.remove(&key).is_some() {
            Press::Run
        } else {
            self.armed.insert(key, now + WINDOW);
            Press::Armed
        }
    }

    pub fn is_armed(&self, key: &Key, now: Instant) -> bool {
        self.armed.get(key).is_some_and(|&until| now < until)
    }

    /// Drops what waited too long; true when something was dropped (the
    /// view then fills again).
    pub fn expire(&mut self, now: Instant) -> bool {
        let before = self.armed.len();
        self.armed.retain(|_, until| now < *until);
        self.armed.len() != before
    }

    /// When the next armed action expires.
    pub fn next_expiry(&self) -> Option<Instant> {
        self.armed.values().min().copied()
    }

    /// Everything disarmed (the window was dropped).
    pub fn clear(&mut self) {
        self.armed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_click_within_4_seconds_runs_it() {
        let mut c = Confirms::default();
        let t = Instant::now();
        let stop = key("voicedock", "stop", None);
        assert_eq!(c.press(stop.clone(), true, t), Press::Armed);
        assert!(c.is_armed(&stop, t + Duration::from_secs(3)));
        assert_eq!(
            c.press(stop.clone(), true, t + Duration::from_secs(3)),
            Press::Run
        );
        assert!(!c.is_armed(&stop, t + Duration::from_secs(3)));
    }

    #[test]
    fn it_disarms_after_4_seconds_and_only_for_its_own_app() {
        let mut c = Confirms::default();
        let t = Instant::now();
        let a = key("a", "stop", None);
        let b = key("b", "stop", None);
        c.press(a.clone(), true, t);
        assert!(!c.is_armed(&b, t));
        assert_eq!(c.next_expiry(), Some(t + WINDOW));
        assert!(c.expire(t + WINDOW));
        assert!(!c.is_armed(&a, t + WINDOW));
        assert_eq!(
            c.press(a, true, t + WINDOW),
            Press::Armed,
            "armed again, not run"
        );
        assert_eq!(c.press(b, false, t), Press::Run);
    }

    #[test]
    fn components_are_armed_on_their_own() {
        let mut c = Confirms::default();
        let t = Instant::now();
        c.press(key("vd", "stop", Some("kokoro")), true, t);
        assert!(!c.is_armed(&key("vd", "stop", Some("piper")), t));
        assert!(c.is_armed(&key("vd", "stop", Some("kokoro")), t));
    }
}
