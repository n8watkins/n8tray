//! Demand rules (spec 3.7, plan 5.3): an app needed only while one of some
//! processes runs (a browser, for VoiceDock) is stopped after a while without
//! one, shown as sleeping, and started again when one appears.

use chrono::{DateTime, Duration, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Nothing,
    Stop,
    Start,
}

#[derive(Debug, Clone, Default)]
pub struct DemandState {
    last_seen: Option<DateTime<Utc>>,
    /// Stopped by the rule: sleeping, not stopped.
    pub idle: bool,
}

impl DemandState {
    /// Already sleeping: the app starts when a demand process appears.
    pub fn sleeping() -> DemandState {
        DemandState {
            last_seen: None,
            idle: true,
        }
    }

    /// One look at the process list. The time without a demand process
    /// counts from the first look.
    pub fn step(&mut self, present: bool, now: DateTime<Utc>, stop_after: Duration) -> Decision {
        if present {
            self.last_seen = Some(now);
            if self.idle {
                self.idle = false;
                return Decision::Start;
            }
            return Decision::Nothing;
        }
        let since = *self.last_seen.get_or_insert(now);
        if !self.idle && now - since >= stop_after {
            self.idle = true;
            return Decision::Stop;
        }
        Decision::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + s, 0).unwrap()
    }

    #[test]
    fn stops_after_the_quiet_time_and_starts_when_wanted() {
        let mut d = DemandState::default();
        let ten = Duration::minutes(10);
        assert_eq!(d.step(true, at(0), ten), Decision::Nothing);
        assert_eq!(d.step(false, at(60), ten), Decision::Nothing);
        assert_eq!(d.step(false, at(599), ten), Decision::Nothing);
        assert_eq!(d.step(false, at(600), ten), Decision::Stop);
        assert!(d.idle);
        assert_eq!(d.step(false, at(900), ten), Decision::Nothing);
        assert_eq!(d.step(true, at(901), ten), Decision::Start);
        assert_eq!(d.step(true, at(902), ten), Decision::Nothing);
    }

    #[test]
    fn counts_from_the_first_look() {
        let mut d = DemandState::default();
        let one = Duration::minutes(1);
        assert_eq!(d.step(false, at(0), one), Decision::Nothing);
        assert_eq!(d.step(false, at(60), one), Decision::Stop);
    }
}
