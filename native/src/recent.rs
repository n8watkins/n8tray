//! The last warnings and errors from the tray log, kept in memory for the
//! status details ("Recent warnings"). A `tracing` layer next to the log
//! file's, so every warning anywhere in the host is seen once.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::Mutex;

use n8tray_core::model::WarningView;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

/// How many are kept.
const KEEP: usize = 20;

static RECENT: Mutex<VecDeque<WarningView>> = Mutex::new(VecDeque::new());

/// The kept warnings, newest first.
pub fn list() -> Vec<WarningView> {
    RECENT.lock().unwrap().iter().rev().cloned().collect()
}

/// Adds one; the same message again right after only counts up.
pub fn push(level: &str, message: String, at: String) {
    let mut recent = RECENT.lock().unwrap();
    if let Some(last) = recent.back_mut()
        && last.message == message
        && last.level == level
    {
        last.count += 1;
        last.at = at;
        return;
    }
    recent.push_back(WarningView {
        at,
        level: level.into(),
        message,
        count: 1,
    });
    while recent.len() > KEEP {
        recent.pop_front();
    }
}

pub struct RecentLayer;

impl<S: Subscriber> Layer<S> for RecentLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let level = match *event.metadata().level() {
            Level::ERROR => "error",
            Level::WARN => "warn",
            _ => return,
        };
        let mut message = Message(String::new());
        event.record(&mut message);
        let at = chrono::Local::now().format("%H:%M").to_string();
        push(level, message.0, at);
    }
}

struct Message(String);

impl Visit for Message {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats_count_up_and_the_newest_comes_first() {
        push("warn", "a".into(), "10:00".into());
        push("warn", "a".into(), "10:01".into());
        push("error", "b".into(), "10:02".into());
        let list = list();
        let a = list.iter().find(|w| w.message == "a").unwrap();
        assert_eq!((a.count, a.at.as_str()), (2, "10:01"));
        let b = list.iter().position(|w| w.message == "b").unwrap();
        let a = list.iter().position(|w| w.message == "a").unwrap();
        assert!(b < a);
    }
}
