//! Opening a file, folder or URL with the OS's default handler. Only
//! targets the host resolved from a manifest or its own folders get here.

use std::process::{Command, Stdio};

use tracing::warn;

pub fn open(target: &str) {
    let mut command = if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let mut c = Command::new(std::path::Path::new(&root).join("explorer.exe"));
        c.arg(target);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg(target);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(target);
        c
    };
    let result = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Err(e) = result {
        warn!("Could not open {target}: {e}");
    }
}
