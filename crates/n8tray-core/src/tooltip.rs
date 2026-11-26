//! The tray icon's hover text. The shell shows at most 127 characters; the
//! status line comes first (cut at 63), memory comes before a long list of
//! apps, and the list gets the room that is left (the PowerShell tray's rules).

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

/// `status` such as "N8 Tray: Healthy", `running` the names of running pinned
/// apps, `more_running` how many unpinned apps run (named only as "N more
/// running", D32), `memory` lines such as "Windows 4.2/31.1 GB free".
pub fn format(status: &str, running: &[String], more_running: usize, memory: &[String]) -> String {
    let status = cut(status, STATUS_LIMIT);
    let mut memory: Vec<String> = memory.to_vec();
    let used = |memory: &[String]| {
        status.chars().count() + memory.iter().map(|m| m.chars().count() + 1).sum::<usize>()
    };
    let any = !running.is_empty() || more_running > 0;
    if any {
        // Drop memory lines from the end until the list has some room.
        while !memory.is_empty() && LIMIT.saturating_sub(used(&memory) + 1) < MIN_RUNNING {
            memory.pop();
        }
    }
    let mut lines = vec![status.clone()];
    if any {
        let room = LIMIT.saturating_sub(used(&memory) + 1);
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
    lines.extend(memory);
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
            &["Windows 4.2/31.1 GB free".into()],
        );
        assert_eq!(
            text,
            "N8 Tray: Healthy\nRunning: N8 Hotkeys, VoiceDock\nWindows 4.2/31.1 GB free"
        );
        let many: Vec<String> = (0..30).map(|i| format!("App number {i}")).collect();
        let long_status = "N8 Tray: Problem: ".to_string() + &"x".repeat(100);
        let text = format(&long_status, &many, 0, &["Windows 4.2/31.1 GB free".into()]);
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
        let memory = ["Windows 4.2/31.1 GB free · N8 310 MB".to_string()];
        assert_eq!(
            format("N8 Tray: Healthy", &["N8 Hotkeys".into()], 2, &memory),
            "N8 Tray: Healthy\nRunning: N8 Hotkeys · 2 more running\nWindows 4.2/31.1 GB free · N8 310 MB"
        );
        assert_eq!(
            format("N8 Tray: Healthy", &[], 1, &memory),
            "N8 Tray: Healthy\n1 more running\nWindows 4.2/31.1 GB free · N8 310 MB"
        );

        // A long list is cut, the count is not.
        let many: Vec<String> = (0..30).map(|i| format!("App number {i}")).collect();
        let text = format("N8 Tray: Healthy", &many, 3, &memory);
        assert!(text.chars().count() <= LIMIT, "{}", text.chars().count());
        let line = text.lines().nth(1).unwrap();
        assert!(line.starts_with("Running: App number 0"), "{line}");
        assert!(line.ends_with("... · 3 more running"), "{line}");
    }
}
