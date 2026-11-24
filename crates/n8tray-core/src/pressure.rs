//! System memory levels with hysteresis and dwell (plan 3.5 and 5.5), the
//! PowerShell tray's rules: getting worse uses the thresholds as they are,
//! getting better needs the figure 10% above the threshold it crosses (in
//! below 3.0 GB, out above 3.3 GB), and a new level counts only after it held
//! for the dwell time, so a figure hovering at a threshold never flips the
//! level back and forth.
//!
//! The level shown dwells 10 s both ways. The level pushed to apps (spec 5.4)
//! enters after 10 s but recovers only after 60 s, so engines do not flap
//! between stopping and starting.

use chrono::{DateTime, Duration, Utc};

use crate::model::LevelName;

/// How far above a threshold free memory must be to count as recovered.
pub const RECOVER_MARGIN: f64 = 0.10;

/// One figure's level over time.
#[derive(Debug, Clone)]
pub struct Hysteresis {
    pub attention_below: f64,
    pub problem_below: f64,
    pub enter_dwell: Duration,
    pub recover_dwell: Duration,
    shown: Option<LevelName>,
    pending: Option<(LevelName, DateTime<Utc>)>,
}

fn rank(level: LevelName) -> u8 {
    match level {
        LevelName::Problem => 2,
        LevelName::Attention => 1,
        _ => 0,
    }
}

/// The raw level of a free-memory figure.
pub fn raw_level(free: f64, attention_below: f64, problem_below: f64) -> LevelName {
    if free < problem_below {
        LevelName::Problem
    } else if free < attention_below {
        LevelName::Attention
    } else {
        LevelName::Ok
    }
}

impl Hysteresis {
    pub fn new(
        attention_below: f64,
        problem_below: f64,
        enter_dwell: Duration,
        recover_dwell: Duration,
    ) -> Self {
        Hysteresis {
            attention_below,
            problem_below,
            enter_dwell,
            recover_dwell,
            shown: None,
            pending: None,
        }
    }

    /// The shown level today's tray uses: 10 s each way.
    pub fn display(attention_below: f64, problem_below: f64) -> Self {
        Self::new(
            attention_below,
            problem_below,
            Duration::seconds(10),
            Duration::seconds(10),
        )
    }

    /// The level pushed to apps: 10 s in, 60 s out.
    pub fn push(attention_below: f64, problem_below: f64) -> Self {
        Self::new(
            attention_below,
            problem_below,
            Duration::seconds(10),
            Duration::seconds(60),
        )
    }

    pub fn level(&self) -> Option<LevelName> {
        self.shown
    }

    /// Takes one reading and returns the level now. The first reading is
    /// shown at once.
    pub fn step(&mut self, free: f64, now: DateTime<Utc>) -> LevelName {
        let raw = raw_level(free, self.attention_below, self.problem_below);
        let Some(shown) = self.shown else {
            self.shown = Some(raw);
            return raw;
        };
        let target = if rank(raw) > rank(shown) {
            raw
        } else if rank(raw) < rank(shown) {
            let factor = 1.0 + RECOVER_MARGIN;
            let recovered = raw_level(
                free,
                self.attention_below * factor,
                self.problem_below * factor,
            );
            if rank(recovered) < rank(shown) {
                recovered
            } else {
                shown
            }
        } else {
            shown
        };
        if target == shown {
            self.pending = None;
            return shown;
        }
        let worse = rank(target) > rank(shown);
        // Moving the same way as the pending change keeps its clock, so
        // sliding from attention into problem is not delayed twice.
        let since = match self.pending {
            Some((p, since)) if (rank(p) > rank(shown)) == worse => since,
            _ => now,
        };
        self.pending = Some((target, since));
        let dwell = if worse {
            self.enter_dwell
        } else {
            self.recover_dwell
        };
        if now - since >= dwell {
            self.shown = Some(target);
            self.pending = None;
        }
        self.shown.unwrap_or(target)
    }

    /// Forgets the figure (it went away); the next reading is shown at once.
    pub fn reset(&mut self) {
        self.shown = None;
        self.pending = None;
    }
}

/// When to notify about low memory: once a problem has held for 5 minutes,
/// once per problem, and at most once an hour.
#[derive(Debug, Clone, Default)]
pub struct LowMemoryNotice {
    problem_since: Option<DateTime<Utc>>,
    sent_this_time: bool,
    last_sent: Option<DateTime<Utc>>,
}

impl LowMemoryNotice {
    pub const HOLD_MINUTES: i64 = 5;
    pub const GAP_MINUTES: i64 = 60;

    /// Takes the shown level; true when a notice is due now. With notices
    /// off (`enabled` false) it still follows the level, so turning them on
    /// in the middle of a problem does not notify at once for an old one
    /// that was already due.
    pub fn step(&mut self, level: LevelName, enabled: bool, now: DateTime<Utc>) -> bool {
        if level != LevelName::Problem {
            self.problem_since = None;
            self.sent_this_time = false;
            return false;
        }
        let since = *self.problem_since.get_or_insert(now);
        let due = !self.sent_this_time
            && now - since >= Duration::minutes(Self::HOLD_MINUTES)
            && self
                .last_sent
                .is_none_or(|t| now - t >= Duration::minutes(Self::GAP_MINUTES));
        if !due {
            return false;
        }
        // Due once per problem whether or not it is shown.
        self.sent_this_time = true;
        if enabled {
            self.last_sent = Some(now);
        }
        enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_memory_notices_wait_5_minutes_and_come_at_most_hourly() {
        let mut notice = LowMemoryNotice::default();
        let t = |minutes: i64| at(0) + Duration::minutes(minutes);
        let problem = LevelName::Problem;

        // A problem must hold for 5 minutes.
        assert!(!notice.step(problem, true, t(0)));
        assert!(!notice.step(problem, true, t(4)));
        assert!(notice.step(problem, true, t(5)));
        assert!(!notice.step(problem, true, t(6)), "once per problem");
        assert!(!notice.step(problem, true, t(90)), "once per problem");

        // A short dip is never notified.
        assert!(!notice.step(LevelName::Attention, true, t(91)));
        assert!(!notice.step(problem, true, t(92)));
        assert!(!notice.step(LevelName::Ok, true, t(95)));

        // A new problem within the hour after the last notice: quiet.
        let mut notice = LowMemoryNotice::default();
        notice.step(problem, true, t(0));
        assert!(notice.step(problem, true, t(5)));
        notice.step(LevelName::Ok, true, t(10));
        notice.step(problem, true, t(20));
        assert!(!notice.step(problem, true, t(25)), "within the hour");
        assert!(!notice.step(problem, true, t(64)));
        assert!(notice.step(problem, true, t(65)), "an hour after the last");

        // Flapping all day sends at most one an hour (it was 20 to 30 a day).
        let mut notice = LowMemoryNotice::default();
        let mut sent = 0;
        for minute in 0..24 * 60 {
            let level = if minute % 12 < 7 {
                problem
            } else {
                LevelName::Ok
            };
            sent += notice.step(level, true, t(minute)) as u32;
        }
        assert!(sent <= 24, "{sent} in a day");
        assert!(sent > 0);

        // Off: never, and switching on later does not fire for an old problem.
        let mut notice = LowMemoryNotice::default();
        assert!(!notice.step(problem, false, t(0)));
        assert!(!notice.step(problem, false, t(10)));
        assert!(!notice.step(problem, true, t(11)));
    }

    fn at(s: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + s, 0).unwrap()
    }

    #[test]
    fn enters_below_3_after_10_s_and_leaves_above_3_3() {
        let mut h = Hysteresis::display(3.0, 1.5);
        assert_eq!(
            h.step(4.0, at(0)),
            LevelName::Ok,
            "the first reading shows at once"
        );
        assert_eq!(h.step(2.9, at(1)), LevelName::Ok, "not yet: dwell");
        assert_eq!(h.step(2.9, at(10)), LevelName::Ok);
        assert_eq!(h.step(2.9, at(11)), LevelName::Attention, "held 10 s");
        assert_eq!(
            h.step(3.2, at(30)),
            LevelName::Attention,
            "3.2 is not 10% above 3.0"
        );
        assert_eq!(h.step(3.2, at(60)), LevelName::Attention);
        assert_eq!(
            h.step(3.4, at(61)),
            LevelName::Attention,
            "recovery dwells too"
        );
        assert_eq!(h.step(3.4, at(71)), LevelName::Ok);
    }

    #[test]
    fn a_one_off_reading_never_shows() {
        let mut h = Hysteresis::display(3.0, 1.5);
        h.step(4.0, at(0));
        assert_eq!(h.step(1.0, at(1)), LevelName::Ok);
        assert_eq!(h.step(4.0, at(2)), LevelName::Ok);
        assert_eq!(h.step(4.0, at(20)), LevelName::Ok);
    }

    #[test]
    fn sliding_into_problem_keeps_the_clock() {
        let mut h = Hysteresis::display(3.0, 1.5);
        h.step(4.0, at(0));
        h.step(2.5, at(1));
        assert_eq!(h.step(1.0, at(6)), LevelName::Ok);
        assert_eq!(
            h.step(1.0, at(11)),
            LevelName::Problem,
            "10 s since it got worse"
        );
    }

    #[test]
    fn the_pushed_level_recovers_after_60_s() {
        let mut h = Hysteresis::push(3.0, 1.5);
        h.step(1.0, at(0));
        assert_eq!(h.level(), Some(LevelName::Problem));
        assert_eq!(h.step(5.0, at(1)), LevelName::Problem);
        assert_eq!(h.step(5.0, at(30)), LevelName::Problem);
        assert_eq!(h.step(5.0, at(61)), LevelName::Ok);
    }

    #[test]
    fn problem_recovers_to_attention_above_1_65() {
        let mut h = Hysteresis::display(3.0, 1.5);
        h.step(1.0, at(0));
        assert_eq!(h.step(1.6, at(20)), LevelName::Problem);
        h.step(1.7, at(21));
        assert_eq!(h.step(1.7, at(31)), LevelName::Attention);
    }
}
