//! The tray log: one file per day in the logs folder, the last 7 kept.

use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};

pub fn init(dir: &Path) -> Option<WorkerGuard> {
    std::fs::create_dir_all(dir).ok()?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("n8tray")
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    use tracing_subscriber::prelude::*;
    let file = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(false);
    tracing_subscriber::registry()
        .with(file.with_filter(tracing_subscriber::filter::LevelFilter::INFO))
        .with(crate::recent::RecentLayer)
        .init();
    Some(guard)
}

/// Today's log file, or the newest one.
pub fn latest(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("n8tray") && n.ends_with(".log"))
        })
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
}
