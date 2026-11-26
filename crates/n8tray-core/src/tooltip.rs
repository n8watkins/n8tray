//! The tray icon's hover text. The shell shows at most 127 characters; the
//! status line comes first (cut at 63), the notes (N8's memory, "Watching
//! only") come before a long list of apps, and the list gets the room that is
//! left (the PowerShell tray's rules).

pub const LIMIT: usize = 127;
const STATUS_LIMIT: usize = 63;
const MIN_RUNNING: usize = 24;

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(3)).collect();
    out = out.trim_end().to_string();
    out.push_str("...");
    out
}

/// `status` such as "N8 Tray: Healthy", `running` the names of running apps in
/// the main view, `more_running` how many apps outside it run (the snapshot's
/// `more.running`, said only as "N more running", D32), `notes` lines such as
/// "N8 310 MB in all".
pub fn format(status: &str, running: &[String], more_running: usize, notes: &[String]) -> String {
    let status = cut(status, STATUS_LIMIT);
    let mut notes: Vec<String> = notes.to_vec();
    let used = |notes: &[String]| {
        status.chars().count() + notes.iter().map(|m| m.chars().count() + 1).sum::<usize>()
    };
    let any = !running.is_empty() || more_running > 0;
    if any {
        // Drop notes from the end until the list has some room.
        while !notes.is_empty() && LIMIT.saturating_sub(used(&notes) + 1) < MIN_RUNNING {
            notes.pop();
        }
    }
    let mut lines = vec![status.clone()];
    if any {
        let room = LIMIT.saturating_sub(used(&notes) + 1);
        let more = format!("{more_running} more running");
        lines.push(match (running.is_empty(), more_running) {
            (_, 0) => cut(&format!("Running: {}", running.join(", ")), room),
            (true, _) => cut(&more, room),
            // The count stays whole when the names are cut.
            (false, _) => {
                let suffix = format!(" · {more}");
                let names = cut(
                    &format!("Running: {}", running.join(", ")),
                    room.saturating_sub(suffix.chars().count()),
                );
                format!("{names}{suffix}")
            }
        });
    }
    lines.extend(notes);
    cut(&lines.join("\n"), LIMIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_the_shell_limit() {
        let text = format(
            "N8 Tray: Healthy",
            &["N8 Hotkeys".into(), "VoiceDock".into()],
            0,
            &["N8 667 MB in all".into()],
        );
        assert_eq!(
            text,
            "N8 Tray: Healthy\nRunning: N8 Hotkeys, VoiceDock\nN8 667 MB in all"
        );
        let many: Vec<String> = (0..30).map(|i| format!("App number {i}")).collect();
        let long_status = "N8 Tray: Problem: ".to_string() + &"x".repeat(100);
        let notes = [
            "N8 1.9 GB in all".to_string(),
            "Watching only: the PowerShell N8 Tray manages apps".to_string(),
        ];
        let text = format(&long_status, &many, 0, &notes);
        assert!(text.chars().count() <= LIMIT, "{}", text.chars().count());
        assert!(text.lines().next().unwrap().chars().count() <= STATUS_LIMIT);
        assert!(text.contains("Running: App number 0"));
    }

    #[test]
    fn nothing_running() {
        assert_eq!(
            format("N8 Tray: Nothing running", &[], 0, &[]),
            "N8 Tray: Nothing running"
        );
    }

    #[test]
    fn unpinned_apps_are_counted_not_named() {
        let notes = ["N8 310 MB in all".to_string()];
        assert_eq!(
            format("N8 Tray: Healthy", &["N8 Hotkeys".into()], 2, &notes),
            "N8 Tray: Healthy\nRunning: N8 Hotkeys · 2 more running\nN8 310 MB in all"
        );
        assert_eq!(
            format("N8 Tray: Healthy", &[], 1, &notes),
            "N8 Tray: Healthy\n1 more running\nN8 310 MB in all"
        );

        // The `pinned` interface fixture's tooltip.
        assert_eq!(
            format(
                "N8 Tray: Needs attention",
                &["Lead Scraper".into(), "VoiceDock".into()],
                1,
                &["N8 768 MB in all".into()]
            ),
            "N8 Tray: Needs attention\nRunning: Lead Scraper, VoiceDock · 1 more running\nN8 768 MB in all"
        );

        // A long list is cut, the count is not.
        let many: Vec<String> = (0..30).map(|i| format!("App number {i}")).collect();
        let text = format("N8 Tray: Healthy", &many, 3, &notes);
        assert!(text.chars().count() <= LIMIT, "{}", text.chars().count());
        let line = text.lines().nth(1).unwrap();
        assert!(line.starts_with("Running: App number 0"), "{line}");
        assert!(line.ends_with("... · 3 more running"), "{line}");
    }
}
