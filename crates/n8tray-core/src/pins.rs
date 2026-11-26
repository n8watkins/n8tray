//! Pinned apps (D32): which apps the main view shows, the one "N more" line
//! for the rest, and what the headline and the tooltip count.
//!
//! Hiding is about what is seen, never about what runs: an unpinned app in
//! trouble is back in the main view until it clears, and the level, the tray
//! icon's dot and N8's memory total still count every app (the host computes
//! those from every hosted app, not from this).

use crate::model::{AppView, LevelName, MoreView};
use crate::supervisor::round1;
use crate::util::mb_text;

/// The apps the hub lists at all: shown in N8 Tray and not hidden.
pub fn in_hub(app: &AppView) -> bool {
    app.hosted && app.level != LevelName::Hidden
}

/// In the main view: listed in the hub, and pinned or in trouble.
pub fn in_main_view(hosted: bool, pinned: bool, level: LevelName) -> bool {
    hosted && level != LevelName::Hidden && (pinned || level >= LevelName::Attention)
}

/// What the headline, the tooltip and the "more" line count, from the hub's
/// apps in the list's order.
#[derive(Debug, Clone, PartialEq)]
pub struct Counts {
    /// Pinned apps that run, by name ("2 of 2 running" and the tooltip).
    pub running_names: Vec<String>,
    /// Pinned apps.
    pub total: u32,
    /// Unpinned apps that run, in the main view or not (the tooltip's "N more
    /// running").
    pub more_running: usize,
    /// The apps outside the main view; `None` when there are none.
    pub more: Option<MoreView>,
}

pub fn counts(hub: &[&AppView]) -> Counts {
    let pinned = || hub.iter().filter(|a| a.pinned);
    let outside: Vec<&&AppView> = hub.iter().filter(|a| !a.in_main_view).collect();
    let more = (!outside.is_empty()).then(|| {
        let count = outside.len() as u32;
        let running = outside.iter().filter(|a| a.running).count() as u32;
        let memory_mb = round1(
            outside
                .iter()
                .filter(|a| a.running)
                .filter_map(|a| a.memory_mb)
                .sum(),
        );
        MoreView {
            count,
            running,
            memory_mb,
            text: more_text(count, running, memory_mb),
        }
    });
    Counts {
        running_names: pinned()
            .filter(|a| a.running)
            .map(|a| a.name.clone())
            .collect(),
        total: pinned().count() as u32,
        more_running: hub.iter().filter(|a| !a.pinned && a.running).count(),
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

    fn app(name: &str, pinned: bool, running: bool, level: LevelName) -> AppView {
        AppView {
            id: name.to_lowercase(),
            name: name.into(),
            group: None,
            description: None,
            icon: None,
            version: "1.0.0".into(),
            level,
            state: if running { "ready" } else { "stopped" }.into(),
            words: String::new(),
            summary: String::new(),
            detail: None,
            running,
            should_run: running,
            enabled: true,
            hosted: true,
            pinned,
            in_main_view: in_main_view(true, pinned, level),
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
        }
    }

    #[test]
    fn the_main_view_is_pinned_apps_and_apps_in_trouble() {
        use LevelName::*;
        assert!(in_main_view(true, true, Ok));
        assert!(in_main_view(true, true, Off));
        assert!(!in_main_view(true, false, Ok));
        assert!(!in_main_view(true, false, Off));
        assert!(in_main_view(true, false, Attention));
        assert!(in_main_view(true, false, Problem));
        // Its own icon or hidden: not in the hub, so in no view.
        assert!(!in_main_view(false, true, Problem));
        assert!(!in_main_view(true, true, Hidden));
    }

    #[test]
    fn with_every_app_pinned_nothing_changes() {
        let a = app("A", true, true, LevelName::Ok);
        let b = app("B", true, false, LevelName::Off);
        let c = app("C", true, true, LevelName::Attention);
        let counts = counts(&[&a, &b, &c]);
        assert_eq!(counts.running_names, ["A", "C"]);
        assert_eq!(counts.total, 3);
        assert_eq!(counts.more_running, 0);
        assert_eq!(counts.more, None);
    }

    #[test]
    fn the_headline_counts_pinned_apps_only() {
        let a = app("A", true, true, LevelName::Ok);
        let b = app("B", true, true, LevelName::Ok);
        let c = app("C", false, true, LevelName::Ok);
        let d = app("D", false, false, LevelName::Off);
        let counts = counts(&[&a, &b, &c, &d]);
        assert_eq!(counts.running_names, ["A", "B"], "2 of 2 running");
        assert_eq!(counts.total, 2);
        assert_eq!(counts.more_running, 1);
    }

    #[test]
    fn the_more_line_in_each_case() {
        let a = app("A", true, true, LevelName::Ok);
        // Two unpinned, none running.
        let s1 = app("S1", false, false, LevelName::Off);
        let s2 = app("S2", false, false, LevelName::Off);
        let more = counts(&[&a, &s1, &s2]).more.unwrap();
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
        let r = app("R", false, true, LevelName::Ok);
        let more = counts(&[&a, &r, &s1]).more.unwrap();
        assert_eq!(more.count, 2);
        assert_eq!(more.running, 1);
        assert_eq!(more.memory_mb, 40.0);
        assert_eq!(more.text, "2 more · 1 running, 40 MB");

        // Running with no figure yet.
        let mut unmeasured = app("U", false, true, LevelName::Ok);
        unmeasured.memory_mb = None;
        let more = counts(&[&unmeasured]).more.unwrap();
        assert_eq!(more.text, "1 more · 1 running");

        // From 1000 MB the line reads GB, as the rest of N8 Tray does.
        assert_eq!(more_text(3, 2, 1950.0), "3 more · 2 running, 1.9 GB");
    }

    #[test]
    fn an_unpinned_app_in_trouble_is_in_the_main_view() {
        let a = app("A", true, true, LevelName::Ok);
        let trouble = app("T", false, true, LevelName::Problem);
        let quiet = app("Q", false, false, LevelName::Off);
        assert!(trouble.in_main_view);
        let counts = counts(&[&a, &trouble, &quiet]);
        // Not in the "more" line while it is in the main view...
        let more = counts.more.unwrap();
        assert_eq!((more.count, more.running), (1, 0));
        assert_eq!(more.text, "1 more");
        // ...nor in the headline, which counts pinned apps; the tooltip still
        // says it runs.
        assert_eq!(counts.running_names, ["A"]);
        assert_eq!(counts.total, 1);
        assert_eq!(counts.more_running, 1);
    }
}
