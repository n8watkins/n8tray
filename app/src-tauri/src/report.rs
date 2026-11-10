//! The status as plain text, for Copy status in the status details: what a
//! user pastes into a bug report.

use std::fmt::Write as _;

use n8tray_core::model::{HostSnapshot, WarningView};

fn mb(value: Option<f64>) -> String {
    match value {
        Some(mb) if mb >= 1000.0 => format!("{:.1} GB", mb / 1024.0),
        Some(mb) => format!("{mb:.0} MB"),
        None => "-".into(),
    }
}

pub fn status_text(s: &HostSnapshot, warnings: &[WarningView]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "N8 Tray {}: {}", s.version, s.headline);
    if let Some(reason) = &s.reason {
        let _ = writeln!(out, "Reason: {reason}");
    }
    let _ = writeln!(
        out,
        "{} of {} running · {}",
        s.running, s.total, s.memory.text
    );
    let _ = writeln!(
        out,
        "This app: {} (private {}), window {} (private {})",
        mb(Some(s.tray.rss_mb)),
        mb(s.tray.private_mb),
        mb(Some(s.tray.window_mb)),
        mb(s.tray.window_private_mb),
    );
    for app in &s.apps {
        let pids: Vec<String> = app
            .processes
            .iter()
            .filter(|p| p.verdict == "verified")
            .map(|p| p.pid.to_string())
            .collect();
        let _ = writeln!(
            out,
            "- {} {}: {} ({}){}, {}, CPU {}{}",
            app.name,
            app.version,
            app.words,
            app.state,
            if app.hosted { "" } else { ", own icon" },
            mb(app.memory_mb),
            app.cpu_percent
                .map(|c| format!("{c:.1}%"))
                .unwrap_or_else(|| "-".into()),
            if pids.is_empty() {
                String::new()
            } else {
                format!(", pid {}", pids.join(" "))
            },
        );
    }
    for problem in &s.problems {
        let _ = writeln!(out, "Problem: {}: {}", problem.file, problem.message);
    }
    if !warnings.is_empty() {
        let _ = writeln!(out, "Recent warnings:");
        for w in warnings {
            let times = if w.count > 1 {
                format!(" (x{})", w.count)
            } else {
                String::new()
            };
            let _ = writeln!(out, "  {} {}{times}", w.at, w.message);
        }
    }
    out
}
