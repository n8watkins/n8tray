//! The status as plain text, for Copy status in the status details: what a
//! user pastes into a bug report. A copy of the Tauri app's
//! (`app/src-tauri/src/report.rs`), kept the same by a test, until the
//! switch removes that one.
#![forbid(unsafe_code)]

use std::fmt::Write as _;

use n8tray_core::model::{HostSnapshot, WarningView};

fn mb(value: Option<f64>) -> String {
    value.map_or_else(|| "-".into(), n8tray_core::util::mb_text)
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::tests::fixture;

    /// The body of `fn name` in a Rust file: from its signature to the first
    /// line that is only `}`.
    fn function(text: &str, name: &str) -> String {
        let start = text
            .find(&format!("fn {name}("))
            .unwrap_or_else(|| panic!("fn {name}"));
        let end = text[start..].find("\n}\n").expect("the function's end") + start;
        text[start..end].to_string()
    }

    /// Copy status says exactly what today's window copies.
    #[test]
    fn the_text_is_the_tauri_apps() {
        let ours = include_str!("report.rs");
        let todays = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/src-tauri/src/report.rs"),
        )
        .unwrap();
        for name in ["mb", "status_text"] {
            assert_eq!(function(ours, name), function(&todays, name), "fn {name}");
        }
    }

    #[test]
    fn it_names_each_app_and_the_warnings() {
        let s = fixture("busy");
        let warnings = vec![WarningView {
            at: "18:02".into(),
            level: "warn".into(),
            message: "VoiceDock did not answer".into(),
            count: 3,
        }];
        let text = status_text(&s, &warnings);
        assert!(text.starts_with(&format!("N8 Tray {}: {}\n", s.version, s.headline)));
        for app in &s.apps {
            assert!(text.contains(&format!("- {} ", app.name)), "{}", app.name);
        }
        assert!(text.ends_with("Recent warnings:\n  18:02 VoiceDock did not answer (x3)\n"));
        assert_eq!(mb(None), "-");
        assert_eq!(mb(Some(1950.0)), "1.9 GB");
    }
}
