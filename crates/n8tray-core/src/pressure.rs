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

#[cfg(test)]
mod tests {
    use super::*;

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
