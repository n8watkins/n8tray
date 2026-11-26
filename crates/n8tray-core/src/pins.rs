//! Pinned apps (D32): which apps the main view shows, the one "N more" line
//! for the rest, and what the pinned headline and the tooltip count.
//!
//! Hiding is about what is seen, never about what runs: an unpinned app in
//! trouble is back in the main view until 30 s after it clears, and the level,
//! the tray icon's dot, N8's memory total and the snapshot's `running` and
//! `total` still count every app (the host computes those from every app in
//! the hub, not from this).

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};

use crate::model::{AppView, LevelName, MoreView};
use crate::supervisor::round1;
use crate::util::mb_text;

/// How long an app pulled into the main view by trouble stays there after the
/// trouble clears, so a flapping app does not jump in and out.
pub const HOLD_SECONDS: i64 = 30;

/// The apps the hub lists at all: shown in N8 Tray and not hidden.
pub fn in_hub(app: &AppView) -> bool {
    app.hosted && app.level != LevelName::Hidden
}

/// Trouble that pulls an unpinned app into the main view: a degraded or
/// failed state, a problem (such as stopped while it should run), or an active
/// resource alert. Starting and busy are normal work, though their level is
/// attention too, and a turned-off app is never in trouble.
pub fn in_trouble(app: &AppView) -> bool {
    app.enabled
        && (app.level == LevelName::Problem
            || matches!(app.state.as_str(), "degraded" | "failed")
            || app.alert.is_some())
}

/// In the main view: listed in the hub, and pinned, in trouble, or `held`
/// there after trouble.
pub fn in_main_view(app: &AppView, held: bool) -> bool {
    in_hub(app) && (app.pinned || in_trouble(app) || held)
}

/// When each app was last in trouble, for the 30 s hold. The host keeps one
/// and passes it the tick's clock.
#[derive(Debug, Default)]
pub struct TroubleHold {
    last_trouble: HashMap<String, DateTime<Utc>>,
}

impl TroubleHold {
    /// Sets every app's `in_main_view` at `now`, and forgets apps that are
    /// neither in trouble nor held any more.
    pub fn apply(&mut self, apps: &mut [AppView], now: DateTime<Utc>) {
        let mut kept = HashMap::new();
        for app in apps.iter_mut() {
            let last = if in_hub(app) && in_trouble(app) {
                Some(now)
            } else {
                self.last_trouble
                    .get(&app.id)
                    .copied()
                    .filter(|at| now - *at < Duration::seconds(HOLD_SECONDS))
            };
            app.in_main_view = in_main_view(app, last.is_some());
            if let Some(at) = last {
                kept.insert(app.id.clone(), at);
            }
        }
        self.last_trouble = kept;
    }
}

/// What the headline, the tooltip and the "more" line count, from the hub's
/// apps in the list's order, once `in_main_view` is set.
#[derive(Debug, Clone, PartialEq)]
pub struct Counts {
    /// Apps in the main view that run, by name (the tooltip names them).
    pub shown_running: Vec<String>,
    /// Pinned apps that run. An unpinned app pulled in by trouble is shown
    /// but not counted here.
    pub pinned_running: u32,
    /// Pinned apps.
    pub pinned_total: u32,
    /// "2 of 2 running", "Nothing pinned", or "No apps".
    pub pinned_text: String,
    /// The apps outside the main view; `None` when there are none. Its
    /// `running` is the tooltip's "N more running".
    pub more: Option<MoreView>,
}

/// The memory a row shows for an app: what N8 Tray measures plus what the app
/// reports for processes it cannot see (inside WSL).
pub fn row_mb(app: &AppView) -> f64 {
    app.memory_mb.unwrap_or(0.0) + app.namespaced_mb.unwrap_or(0.0)
}

pub fn counts(hub: &[&AppView]) -> Counts {
    let outside: Vec<&&AppView> = hub.iter().filter(|a| !a.in_main_view).collect();
    let more = (!outside.is_empty()).then(|| {
        let count = outside.len() as u32;
        let running = outside.iter().filter(|a| a.running).count() as u32;
        let memory_mb = round1(
            outside
                .iter()
                .filter(|a| a.running)
                .map(|a| row_mb(a))
                .sum(),
        );
        MoreView {
            count,
            running,
            memory_mb,
            text: more_text(count, running, memory_mb),
        }
    });
    let pinned_running = hub.iter().filter(|a| a.pinned && a.running).count() as u32;
    let pinned_total = hub.iter().filter(|a| a.pinned).count() as u32;
    Counts {
        shown_running: hub
            .iter()
            .filter(|a| a.in_main_view && a.running)
            .map(|a| a.name.clone())
            .collect(),
        pinned_running,
        pinned_total,
        pinned_text: match (hub.is_empty(), pinned_total) {
            (true, _) => "No apps".into(),
            (false, 0) => "Nothing pinned".into(),
            _ => format!("{pinned_running} of {pinned_total} running"),
        },
        more,
    }
}

/// "2 more" while none of them runs, "2 more · 1 running, 40 MB" while some
/// do, and "2 more · 1 running" while their memory is not known yet.
pub fn more_text(count: u32, running: u32, memory_mb: f64) -> String {
    match running {
        0 => format!("{count} more"),
        _ if memory_mb > 0.0 => {
            format!("{count} more · {running} running, {}", mb_text(memory_mb))
        }
        _ => format!("{count} more · {running} running"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view as the supervisor makes it, for a state in the contract's
    /// vocabulary.
    fn app(name: &str, pinned: bool, state: &str) -> AppView {
        let (level, running) = match state {
            "ready" => (LevelName::Ok, true),
            "starting" | "busy" | "degraded" => (LevelName::Attention, true),
            "failed" => (LevelName::Problem, false),
            // Stopped while it should run.
            "wanted" => (LevelName::Problem, false),
            "unsupported" => (LevelName::Hidden, false),
            _ => (LevelName::Off, false),
        };
        let mut view = AppView {
            id: name.to_lowercase(),
            name: name.into(),
            group: None,
            description: None,
            icon: None,
            version: "1.0.0".into(),
            level,
            state: if state == "wanted" { "stopped" } else { state }.into(),
            words: String::new(),
            summary: String::new(),
            detail: None,
            running,
            should_run: running || state == "wanted",
            enabled: true,
            hosted: true,
            pinned,
            in_main_view: false,
            auto_start: false,
            pending: None,
            last_outcome: None,
            actions: vec![],
            components: vec![],
            open: vec![],
            menu: vec![],
            memory_mb: running.then_some(40.0),
            private_mb: None,
            cpu_percent: None,
            gpu: None,
            namespaced_mb: None,
            processes: vec![],
            alert: None,
            history: None,
            notes: vec![],
        };
        view.in_main_view = in_main_view(&view, false);
        view
    }

    fn hub(apps: &[AppView]) -> Vec<&AppView> {
        apps.iter().filter(|a| in_hub(a)).collect()
    }

    #[test]
    fn normal_work_is_not_trouble() {
        for state in ["ready", "starting", "busy", "stopped"] {
            let view = app("A", false, state);
            assert!(!in_trouble(&view), "{state}");
            assert!(!view.in_main_view, "{state} stays behind the more line");
        }
        // Starting and busy are attention, as degraded is: the state decides.
        assert_eq!(app("A", false, "starting").level, LevelName::Attention);
    }

    #[test]
    fn trouble_pulls_an_unpinned_app_in() {
        for state in ["degraded", "failed", "wanted"] {
            let view = app("A", false, state);
            assert!(in_trouble(&view), "{state}");
            assert!(view.in_main_view, "{state}");
        }
        let mut alert = app("A", false, "ready");
        alert.alert = Some("Over its 900 MB warning".into());
        alert.level = LevelName::Attention;
        assert!(in_main_view(&alert, false));

        // A turned-off app is never in trouble.
        let mut off = app("A", false, "wanted");
        off.enabled = false;
        off.level = LevelName::Off;
        assert!(!in_main_view(&off, false));
    }

    #[test]
    fn apps_not_in_the_hub_are_in_no_view() {
        let mut own = app("Own", true, "failed");
        own.hosted = false;
        assert!(!in_main_view(&own, true));
        let hidden = app("Hidden", true, "unsupported");
        assert!(!in_main_view(&hidden, true));
    }

    #[test]
    fn an_app_stays_in_the_main_view_30_s_after_its_trouble() {
        let mut hold = TroubleHold::default();
        let t0 = Utc::now();
        let at = |s: i64| t0 + Duration::seconds(s);
        let mut apps = vec![app("A", false, "degraded")];
        hold.apply(&mut apps, t0);
        assert!(apps[0].in_main_view);

        // Cleared by the next look at 5 s: shown until 30 s after it was last
        // seen in trouble.
        apps[0] = app("A", false, "ready");
        hold.apply(&mut apps, at(5));
        assert!(apps[0].in_main_view, "held");
        hold.apply(&mut apps, at(29));
        assert!(apps[0].in_main_view, "held");
        hold.apply(&mut apps, at(30));
        assert!(!apps[0].in_main_view, "the hold ran out");

        // Trouble again restarts the hold from its last moment.
        apps[0] = app("A", false, "failed");
        hold.apply(&mut apps, at(40));
        apps[0] = app("A", false, "ready");
        hold.apply(&mut apps, at(41));
        assert!(apps[0].in_main_view);
        hold.apply(&mut apps, at(70));
        assert!(!apps[0].in_main_view);

        // Starting is not trouble, so it starts no hold.
        apps[0] = app("A", false, "starting");
        hold.apply(&mut apps, at(80));
        apps[0] = app("A", false, "ready");
        hold.apply(&mut apps, at(81));
        assert!(!apps[0].in_main_view);
    }

    #[test]
    fn with_every_app_pinned_nothing_changes() {
        let apps = [
            app("A", true, "ready"),
            app("B", true, "stopped"),
            app("C", true, "busy"),
        ];
        let counts = counts(&hub(&apps));
        assert_eq!(counts.shown_running, ["A", "C"]);
        assert_eq!((counts.pinned_running, counts.pinned_total), (2, 3));
        assert_eq!(counts.pinned_text, "2 of 3 running");
        assert_eq!(counts.more, None);
    }

    #[test]
    fn the_pinned_headline_counts_pinned_apps_only() {
        let apps = [
            app("A", true, "ready"),
            app("B", true, "ready"),
            app("C", false, "ready"),
            app("D", false, "stopped"),
        ];
        let counts = counts(&hub(&apps));
        assert_eq!((counts.pinned_running, counts.pinned_total), (2, 2));
        assert_eq!(counts.pinned_text, "2 of 2 running");
        assert_eq!(counts.shown_running, ["A", "B"]);
        assert_eq!(counts.more.unwrap().running, 1);

        // Nothing pinned, or nothing at all: never "0 of 0 running".
        let apps = [app("C", false, "ready")];
        assert_eq!(counts_of(&apps).pinned_text, "Nothing pinned");
        assert_eq!(counts_of(&[]).pinned_text, "No apps");
    }

    fn counts_of(apps: &[AppView]) -> Counts {
        counts(&hub(apps))
    }

    #[test]
    fn the_more_line_in_each_case() {
        let pinned = app("A", true, "ready");
        let s1 = app("S1", false, "stopped");
        let s2 = app("S2", false, "stopped");
        let more = counts_of(&[pinned.clone(), s1.clone(), s2]).more.unwrap();
        assert_eq!(
            more,
            MoreView {
                count: 2,
                running: 0,
                memory_mb: 0.0,
                text: "2 more".into()
            }
        );

        // One of them runs: its memory is on the line.
        let r = app("R", false, "ready");
        let more = counts_of(&[pinned.clone(), r, s1]).more.unwrap();
        assert_eq!((more.count, more.running, more.memory_mb), (2, 1, 40.0));
        assert_eq!(more.text, "2 more · 1 running, 40 MB");

        // Running with no figure yet.
        let mut unmeasured = app("U", false, "ready");
        unmeasured.memory_mb = None;
        assert_eq!(
            counts_of(&[unmeasured]).more.unwrap().text,
            "1 more · 1 running"
        );

        // Memory inside WSL counts as the row shows it.
        let mut wsl = app("W", false, "ready");
        wsl.memory_mb = Some(0.0);
        wsl.namespaced_mb = Some(1230.0);
        let more = counts_of(&[wsl]).more.unwrap();
        assert_eq!(more.memory_mb, 1230.0);
        assert_eq!(more.text, "1 more · 1 running, 1.2 GB");

        // From 1000 MB the line reads GB, as the rest of N8 Tray does.
        assert_eq!(more_text(3, 2, 1950.0), "3 more · 2 running, 1.9 GB");
    }

    #[test]
    fn the_tooltip_names_what_is_shown_and_counts_the_rest() {
        let apps = [
            app("A", true, "ready"),
            // Pulled in by trouble: shown and named, not counted as pinned.
            app("T", false, "degraded"),
            app("R", false, "ready"),
            app("Q", false, "stopped"),
        ];
        let counts = counts_of(&apps);
        assert_eq!(counts.shown_running, ["A", "T"]);
        let more = counts.more.unwrap();
        assert_eq!((more.count, more.running), (2, 1), "R and Q");
        assert_eq!(more.text, "2 more · 1 running, 40 MB");
        assert_eq!((counts.pinned_running, counts.pinned_total), (1, 1));
    }

    #[test]
    fn a_turned_off_unpinned_app_is_behind_the_more_line() {
        let mut off = app("Off", false, "stopped");
        off.enabled = false;
        let more = counts_of(&[app("A", true, "ready"), off]).more.unwrap();
        assert_eq!((more.count, more.running), (1, 0));
    }
}
