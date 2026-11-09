//! Resource alerts per app (plan 3.5): memory over the budget's warning for
//! `sustainSeconds` is attention, over its maximum for 10 s is a problem, CPU
//! over its warning for 2 minutes is attention. Recovery needs the figure 10%
//! below the threshold for 10 s. A problem notifies once per episode; CPU
//! never notifies, it only colors the row.

use chrono::{DateTime, Duration, Utc};
use n8tray_contract::manifest::Budget as ManifestBudget;

use crate::model::LevelName;
use crate::settings::BudgetOverride;

/// The thresholds in force for one app.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    pub warn_mb: f64,
    pub max_mb: f64,
    pub cpu_warn_percent: f64,
    pub sustain_seconds: i64,
}

impl Budget {
    /// The user's override over the manifest's budget over the defaults.
    pub fn resolve(manifest: Option<&ManifestBudget>, user: Option<&BudgetOverride>) -> Budget {
        let pick_u64 = |u: Option<u64>, m: Option<u64>, d: u64| u.or(m).unwrap_or(d) as f64;
        let pick_u32 = |u: Option<u32>, m: Option<u32>, d: u32| u.or(m).unwrap_or(d);
        Budget {
            warn_mb: pick_u64(
                user.and_then(|b| b.warn_mb),
                manifest.and_then(|b| b.warn_mb),
                ManifestBudget::DEFAULT_WARN_MB,
            ),
            max_mb: pick_u64(
                user.and_then(|b| b.max_mb),
                manifest.and_then(|b| b.max_mb),
                ManifestBudget::DEFAULT_MAX_MB,
            ),
            cpu_warn_percent: pick_u32(
                user.and_then(|b| b.cpu_warn_percent),
                manifest.and_then(|b| b.cpu_warn_percent),
                ManifestBudget::DEFAULT_CPU_WARN_PERCENT,
            ) as f64,
            sustain_seconds: pick_u32(
                user.and_then(|b| b.sustain_seconds),
                manifest.and_then(|b| b.sustain_seconds),
                60,
            ) as i64,
        }
    }
}

pub const HOLD_SECONDS: i64 = 10;
pub const CPU_SUSTAIN_SECONDS: i64 = 120;
const RECOVER: f64 = 0.9;

/// One condition held over time.
#[derive(Debug, Clone, Default)]
struct Held {
    on: bool,
    since: Option<DateTime<Utc>>,
}

impl Held {
    /// `over` with the threshold as it is; `clear` with the recovery margin.
    fn step(&mut self, over: bool, clear: bool, enter: Duration, now: DateTime<Utc>) {
        let target = if self.on { !clear } else { over };
        if target == self.on {
            self.since = None;
            return;
        }
        let since = *self.since.get_or_insert(now);
        let dwell = if target {
            enter
        } else {
            Duration::seconds(HOLD_SECONDS)
        };
        if now - since >= dwell {
            self.on = target;
            self.since = None;
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AlertState {
    warn: Held,
    max: Held,
    cpu: Held,
    notified: bool,
}

/// What the alerts say now.
#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub level: LevelName,
    pub text: Option<String>,
    /// A notification to show now (once per episode).
    pub notify: Option<String>,
}

fn mb_text(mb: f64) -> String {
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

impl AlertState {
    pub fn step(
        &mut self,
        memory_mb: Option<f64>,
        cpu_percent: Option<f64>,
        budget: &Budget,
        now: DateTime<Utc>,
    ) -> Alert {
        let mem = memory_mb.unwrap_or(0.0);
        self.max.step(
            mem > budget.max_mb,
            mem < budget.max_mb * RECOVER,
            Duration::seconds(HOLD_SECONDS),
            now,
        );
        self.warn.step(
            mem > budget.warn_mb,
            mem < budget.warn_mb * RECOVER,
            Duration::seconds(budget.sustain_seconds.max(HOLD_SECONDS)),
            now,
        );
        if let Some(cpu) = cpu_percent {
            self.cpu.step(
                cpu > budget.cpu_warn_percent,
                cpu < budget.cpu_warn_percent * RECOVER,
                Duration::seconds(CPU_SUSTAIN_SECONDS),
                now,
            );
        }
        let (level, text) = if self.max.on {
            (
                LevelName::Problem,
                Some(format!("Over its {} limit", mb_text(budget.max_mb))),
            )
        } else if self.warn.on {
            (
                LevelName::Attention,
                Some(format!("Over its {} warning", mb_text(budget.warn_mb))),
            )
        } else if self.cpu.on {
            (
                LevelName::Attention,
                Some(format!(
                    "CPU over {:.0}% for 2 minutes",
                    budget.cpu_warn_percent
                )),
            )
        } else {
            (LevelName::Ok, None)
        };
        let notify = if level == LevelName::Problem && !self.notified {
            self.notified = true;
            text.clone()
        } else {
            None
        };
        if level == LevelName::Ok {
            self.notified = false;
        }
        Alert {
            level,
            text,
            notify,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + s, 0).unwrap()
    }

    fn budget() -> Budget {
        Budget {
            warn_mb: 500.0,
            max_mb: 1000.0,
            cpu_warn_percent: 50.0,
            sustain_seconds: 60,
        }
    }

    #[test]
    fn resolves_user_over_manifest_over_defaults() {
        let manifest = ManifestBudget {
            warn_mb: Some(900),
            max_mb: Some(1600),
            cpu_warn_percent: None,
            sustain_seconds: Some(30),
        };
        let user = BudgetOverride {
            max_mb: Some(2000),
            ..Default::default()
        };
        let b = Budget::resolve(Some(&manifest), Some(&user));
        assert_eq!(
            (b.warn_mb, b.max_mb, b.cpu_warn_percent, b.sustain_seconds),
            (900.0, 2000.0, 50.0, 30)
        );
        let d = Budget::resolve(None, None);
        assert_eq!((d.warn_mb, d.max_mb), (500.0, 1500.0));
    }

    #[test]
    fn memory_warning_after_sustain_then_problem_notifies_once() {
        let mut s = AlertState::default();
        let b = budget();
        assert_eq!(s.step(Some(600.0), None, &b, at(0)).level, LevelName::Ok);
        assert_eq!(s.step(Some(600.0), None, &b, at(59)).level, LevelName::Ok);
        let a = s.step(Some(600.0), None, &b, at(60));
        assert_eq!(a.level, LevelName::Attention);
        assert_eq!(a.text.as_deref(), Some("Over its 500 MB warning"));
        s.step(Some(1200.0), None, &b, at(61));
        let a = s.step(Some(1200.0), None, &b, at(71));
        assert_eq!(a.level, LevelName::Problem);
        assert_eq!(a.notify.as_deref(), Some("Over its 1000 MB limit"));
        assert_eq!(
            s.step(Some(1200.0), None, &b, at(80)).notify,
            None,
            "once per episode"
        );
        // 950 is not 10% below 1000: still a problem.
        assert_eq!(
            s.step(Some(950.0), None, &b, at(100)).level,
            LevelName::Problem
        );
        s.step(Some(100.0), None, &b, at(101));
        assert_eq!(s.step(Some(100.0), None, &b, at(111)).level, LevelName::Ok);
        s.step(Some(1200.0), None, &b, at(200));
        assert!(
            s.step(Some(1200.0), None, &b, at(210)).notify.is_some(),
            "a new episode notifies"
        );
    }

    #[test]
    fn cpu_only_colors_the_row() {
        let mut s = AlertState::default();
        let b = budget();
        s.step(Some(10.0), Some(80.0), &b, at(0));
        assert_eq!(
            s.step(Some(10.0), Some(80.0), &b, at(119)).level,
            LevelName::Ok
        );
        let a = s.step(Some(10.0), Some(80.0), &b, at(120));
        assert_eq!(a.level, LevelName::Attention);
        assert_eq!(a.notify, None);
    }
}
