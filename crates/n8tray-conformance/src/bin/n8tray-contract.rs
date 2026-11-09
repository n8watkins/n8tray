//! `n8tray-contract`: validate contract documents, probe a running app, and
//! play the host with the test tray. See the kit's README.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use clap::{Parser, Subcommand};

use n8tray_conformance::{files, probe, procs};
use n8tray_contract::{Kind, parse_json, validate, validate_as};

#[derive(Parser)]
#[command(
    name = "n8tray-contract",
    version,
    about = "The N8 Tray contract's conformance tool."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate manifests, status documents, leases, action results and
    /// modular.json files (a folder checks every .json in it).
    Check {
        /// The kind of every file; detected per file when omitted.
        #[arg(long, value_parser = parse_kind)]
        kind: Option<Kind>,
        /// Print one JSON report per file, as a JSON array.
        #[arg(long)]
        json: bool,
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Check a running app against its manifest: security, status, resources, ownership.
    Probe {
        /// The app's manifest.
        manifest: PathBuf,
        /// The runtime folder (default: N8TRAY_RUN_DIR, else the per-OS folder).
        #[arg(long)]
        run_dir: Option<PathBuf>,
        /// HTTP timeout in milliseconds, instead of the manifest's.
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Also invoke this HTTP action.
        #[arg(long)]
        action: Option<String>,
        /// The component for --action.
        #[arg(long, requires = "action")]
        component: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// The test tray: write a host lease every 5 s, as N8 Tray does, until
    /// stopped (or for --for-seconds).
    Host {
        /// App ids to list in `hosting`, comma-separated (may be empty).
        #[arg(long, value_delimiter = ',', default_value = "")]
        hosting: Vec<String>,
        #[arg(long)]
        run_dir: Option<PathBuf>,
        /// Exit after this many seconds; the lease then goes stale because this process is gone.
        #[arg(long)]
        for_seconds: Option<u64>,
        /// Stop writing heartbeats after this many seconds but keep running: a hung host.
        #[arg(long)]
        freeze_after_seconds: Option<u64>,
        /// Write the lease once and exit.
        #[arg(long)]
        once: bool,
    },
    /// Print a JSON Schema: manifest, status, lease, action-result or modular-v1.
    Schema {
        #[arg(value_parser = parse_kind)]
        kind: Kind,
    },
}

fn parse_kind(name: &str) -> Result<Kind, String> {
    Kind::from_name(name).ok_or_else(|| {
        format!(
            "unknown kind; use one of {}",
            Kind::ALL.map(|k| k.name()).join(", ")
        )
    })
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check { kind, json, paths } => check(kind, json, &paths),
        Command::Probe {
            manifest,
            run_dir,
            timeout_ms,
            action,
            component,
            json,
        } => {
            let run_dir = match files::run_dir(run_dir.as_deref()) {
                Ok(dir) => dir,
                Err(e) => return usage(&e.to_string()),
            };
            let options = probe::Options {
                run_dir,
                timeout: timeout_ms.map(Duration::from_millis),
                action: action.map(|a| (a, component)),
            };
            let findings = probe::probe(&manifest, &options);
            if json {
                println!("{}", serde_json::to_string_pretty(&findings).unwrap());
            } else {
                for finding in &findings {
                    println!("{finding}");
                }
            }
            if probe::failed(&findings) {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Command::Host {
            hosting,
            run_dir,
            for_seconds,
            freeze_after_seconds,
            once,
        } => {
            let run_dir = match files::run_dir(run_dir.as_deref()) {
                Ok(dir) => dir,
                Err(e) => return usage(&e.to_string()),
            };
            host(
                &run_dir,
                hosting.into_iter().filter(|h| !h.is_empty()).collect(),
                for_seconds,
                freeze_after_seconds,
                once,
            )
        }
        Command::Schema { kind } => {
            print!("{}", kind.schema());
            ExitCode::SUCCESS
        }
    }
}

fn usage(message: &str) -> ExitCode {
    eprintln!("n8tray-contract: {message}");
    ExitCode::from(2)
}

fn check(kind: Option<Kind>, json: bool, paths: &[PathBuf]) -> ExitCode {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            match std::fs::read_dir(path) {
                Ok(entries) => {
                    let mut found: Vec<_> = entries
                        .filter_map(|e| e.ok().map(|e| e.path()))
                        .filter(|p| p.extension().is_some_and(|x| x == "json"))
                        .collect();
                    found.sort();
                    files.extend(found);
                }
                Err(e) => return usage(&format!("{}: {e}", path.display())),
            }
        } else {
            files.push(path.clone());
        }
    }
    let mut reports = Vec::new();
    let mut any_invalid = false;
    let mut unreadable = false;
    for file in &files {
        let name = file.file_name().and_then(|n| n.to_str());
        let value = match std::fs::read_to_string(file)
            .map_err(|e| e.to_string())
            .and_then(|t| parse_json(&t).map_err(|e| e.to_string()))
        {
            Ok(value) => value,
            Err(e) => {
                eprintln!("{}: {e}", file.display());
                unreadable = true;
                continue;
            }
        };
        let report = match kind {
            Some(kind) => validate_as(kind, &value, name),
            None => validate(&value, name),
        };
        any_invalid |= !report.is_valid();
        if json {
            reports.push(
                serde_json::json!({ "file": file, "valid": report.is_valid(), "report": report }),
            );
        } else {
            print_report(file, &report);
        }
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&reports).unwrap());
    }
    if unreadable {
        ExitCode::from(2)
    } else if any_invalid {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_report(file: &Path, report: &n8tray_contract::Report) {
    if report.is_valid() {
        println!("ok {} ({})", file.display(), report.kind);
    } else {
        println!("INVALID {} ({})", file.display(), report.kind);
        for issue in &report.issues {
            println!("  {issue}");
        }
    }
    for note in &report.notes {
        println!("  note: {note}");
    }
}

fn host(
    run_dir: &Path,
    hosting: Vec<String>,
    for_seconds: Option<u64>,
    freeze_after_seconds: Option<u64>,
    once: bool,
) -> ExitCode {
    if let Err(e) = files::ensure_run_dir(run_dir) {
        return usage(&format!("{}: {e}", run_dir.display()));
    }
    let mut table = procs::Table::new();
    table.refresh();
    let pid = std::process::id();
    let started_at = table
        .get(pid)
        .map(|p| p.started_at)
        .unwrap_or_else(Utc::now);
    let path = files::lease_path(run_dir);
    let began = Instant::now();
    println!(
        "test tray: pid {pid}, lease {}, hosting {hosting:?}",
        path.display()
    );
    loop {
        let frozen =
            freeze_after_seconds.is_some_and(|s| began.elapsed() >= Duration::from_secs(s));
        if !frozen {
            let lease = serde_json::json!({
                "contract": 2,
                "host": "n8tray-test",
                "version": env!("CARGO_PKG_VERSION"),
                "pid": pid,
                "pidStartedAt": started_at.to_rfc3339_opts(SecondsFormat::Millis, true),
                "heartbeat": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                "hosting": hosting,
            });
            if let Err(e) = files::write_atomic(
                &path,
                serde_json::to_string_pretty(&lease).unwrap().as_bytes(),
            ) {
                eprintln!("test tray: cannot write {}: {e}", path.display());
            }
        }
        if once || for_seconds.is_some_and(|s| began.elapsed() >= Duration::from_secs(s)) {
            return ExitCode::SUCCESS;
        }
        std::thread::sleep(Duration::from_secs(
            n8tray_contract::lease::WRITE_INTERVAL_SECONDS,
        ));
    }
}
