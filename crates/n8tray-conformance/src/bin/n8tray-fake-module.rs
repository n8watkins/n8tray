//! `n8tray-fake-module`: a small app that follows the N8 Tray contract, for
//! testing hosts and for adopters to compare against. It serves or writes its
//! status, reports its real resources, follows the host lease, and can
//! misbehave on request (eat memory, report a process it does not own, hide
//! its children).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, SecondsFormat, Utc};
use clap::{Parser, ValueEnum};
use serde_json::{Value, json};
use tiny_http::{Header, Response, Server};

use n8tray_conformance::{files, procs};
use n8tray_contract::lease::{self, TrayMode};

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Transport {
    Http,
    File,
}

#[derive(Parser, Clone)]
#[command(
    name = "n8tray-fake-module",
    version,
    about = "A fake app that follows the N8 Tray contract."
)]
struct Args {
    /// The app id.
    #[arg(long, default_value = "fake")]
    id: String,
    /// Serve status over HTTP or write a status file.
    #[arg(long, value_enum, default_value = "http")]
    status: Transport,
    /// The HTTP port on 127.0.0.1; 0 picks a free one.
    #[arg(long, default_value_t = 0)]
    port: u16,
    /// The runtime folder (default: N8TRAY_RUN_DIR, else the per-OS folder).
    #[arg(long)]
    run_dir: Option<PathBuf>,
    /// Write this app's manifest here once it is serving (with the real port).
    #[arg(long)]
    manifest_out: Option<PathBuf>,
    /// The app's reported state.
    #[arg(long, default_value = "ready")]
    state: String,
    /// Seconds between status updates.
    #[arg(long, default_value_t = 2)]
    interval_seconds: u32,
    /// Child processes to start (reported as workers).
    #[arg(long, default_value_t = 0)]
    children: u32,
    /// Do not report the children: the host must still count them.
    #[arg(long)]
    hide_children: bool,
    /// Allocate and touch this many MiB.
    #[arg(long, default_value_t = 0)]
    eat_mb: usize,
    /// Report this pid as a process it owns, which it does not: the host must reject it.
    #[arg(long)]
    lie_pid: Option<u32>,
    /// Have an own tray icon that hides while hosted: print "tray: <mode>" on every change.
    #[arg(long)]
    follow_lease: bool,
    /// How often to read the lease, in milliseconds (the contract says 5000).
    #[arg(long, default_value_t = 5000)]
    lease_poll_ms: u64,
    /// Exit after this many seconds.
    #[arg(long)]
    exit_after_seconds: Option<u64>,
    /// Internal: run as a child that exits when its parent is gone.
    #[arg(long, hide = true)]
    child_of: Option<u32>,
}

struct Shared {
    status: String,
    pressure: String,
    jobs: Vec<Value>,
    stop: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    if let Some(parent) = args.child_of {
        run_child(parent);
        return ExitCode::SUCCESS;
    }
    let run_dir = match files::run_dir(args.run_dir.as_deref()) {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("fake module: {e}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = files::ensure_run_dir(&run_dir) {
        eprintln!("fake module: {}: {e}", run_dir.display());
        return ExitCode::from(2);
    }

    let _memory: Vec<u8> = vec![1; args.eat_mb * 1024 * 1024];
    let mut children: Vec<Child> = (0..args.children)
        .map(|_| {
            Command::new(std::env::current_exe().expect("the program's path is known"))
                .args(["--child-of", &std::process::id().to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .spawn()
                .expect("a child process starts")
        })
        .collect();

    let shared = Arc::new(Mutex::new(Shared {
        status: String::new(),
        pressure: "ok".into(),
        jobs: Vec::new(),
        stop: false,
    }));
    let token = files::new_token();
    let mut port = 0;
    if args.status == Transport::Http {
        if let Err(e) = files::write_atomic(
            &files::token_path(&run_dir, &args.id),
            format!("{token}\n").as_bytes(),
        ) {
            eprintln!("fake module: cannot write the token: {e}");
            return ExitCode::from(2);
        }
        let server = match Server::http(("127.0.0.1", args.port)) {
            Ok(server) => server,
            Err(e) => {
                eprintln!("fake module: cannot listen on 127.0.0.1:{}: {e}", args.port);
                return ExitCode::from(2);
            }
        };
        port = server
            .server_addr()
            .to_ip()
            .map(|a| a.port())
            .unwrap_or(args.port);
        let shared = Arc::clone(&shared);
        let token = token.clone();
        std::thread::spawn(move || serve(server, port, &token, &shared));
    }

    let mut table = procs::Table::new();
    table.refresh();
    let me = std::process::id();
    let started_at = table.get(me).map(|p| p.started_at).unwrap_or_else(Utc::now);
    // A first status before the manifest appears, so a probe never sees an app without one.
    let mut first = true;
    let began = Instant::now();
    let mut tray: Option<TrayMode> = None;
    let mut settled = false;
    let mut next_lease = Instant::now();
    let mut next_status = Instant::now();

    loop {
        if Instant::now() >= next_status {
            table.refresh();
            let child_pids: Vec<u32> = children.iter().map(Child::id).collect();
            let status =
                status_document(&args, &table, me, started_at, &child_pids, first, &shared);
            first = false;
            if args.status == Transport::File {
                if let Err(e) =
                    files::write_atomic(&files::status_path(&run_dir, &args.id), status.as_bytes())
                {
                    eprintln!("fake module: cannot write the status file: {e}");
                }
            }
            shared.lock().unwrap().status = status;
            if let Some(path) = args.manifest_out.as_deref().filter(|p| !p.exists()) {
                if let Err(e) = write_manifest(path, &args, port) {
                    eprintln!("fake module: cannot write the manifest: {e}");
                    return ExitCode::from(2);
                }
                println!("fake module: ready, pid {me}, manifest {}", path.display());
            }
            next_status = Instant::now() + Duration::from_secs(args.interval_seconds as u64);
        }
        if args.follow_lease && Instant::now() >= next_lease {
            let text = std::fs::read_to_string(files::lease_path(&run_dir)).unwrap_or_default();
            let lease = lease::parse(&text);
            let mut alive_table = procs::Table::new();
            alive_table.refresh();
            let mode = lease::tray_mode(
                lease.as_ref(),
                &args.id,
                Utc::now(),
                started_at,
                settled,
                |pid, started| {
                    alive_table
                        .get(pid)
                        .is_some_and(|p| procs::same_start(started, p.started_at))
                },
            );
            if tray != Some(mode) {
                println!(
                    "tray: {}",
                    match mode {
                        TrayMode::Hosted => "hosted",
                        TrayMode::Waiting => "waiting",
                        TrayMode::Own => "own",
                    }
                );
                tray = Some(mode);
            }
            settled |= mode != TrayMode::Waiting;
            next_lease = Instant::now() + Duration::from_millis(args.lease_poll_ms);
        }
        if shared.lock().unwrap().stop
            || args
                .exit_after_seconds
                .is_some_and(|s| began.elapsed() >= Duration::from_secs(s))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    for child in &mut children {
        let _ = child.kill();
        let _ = child.wait();
    }
    ExitCode::SUCCESS
}

fn run_child(parent: u32) {
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let mut table = procs::Table::new();
        table.refresh();
        if table.get(parent).is_none() {
            return;
        }
    }
}

fn timestamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn process_entry(table: &procs::Table, pid: u32, role: &str, first: bool) -> Option<Value> {
    let p = table.get(pid)?;
    Some(json!({
        "pid": pid,
        "startedAt": timestamp(p.started_at),
        "parentPid": p.parent.unwrap_or(0),
        "role": role,
        "component": null,
        "rssMb": (p.rss_mb * 10.0).round() / 10.0,
        "cpuPercent": if first { Value::Null } else { json!((p.cpu_percent * 10.0).round() / 10.0) },
    }))
}

fn status_document(
    args: &Args,
    table: &procs::Table,
    me: u32,
    started_at: DateTime<Utc>,
    children: &[u32],
    first: bool,
    shared: &Mutex<Shared>,
) -> String {
    let mut processes: Vec<Value> = process_entry(table, me, "main", first)
        .into_iter()
        .collect();
    if !args.hide_children {
        processes.extend(
            children
                .iter()
                .filter_map(|&pid| process_entry(table, pid, "worker", first)),
        );
    }
    if let Some(pid) = args.lie_pid {
        processes.extend(process_entry(table, pid, "worker", first));
    }
    let shared = shared.lock().unwrap();
    let pressure = shared.pressure.clone();
    let features: Vec<&str> = if args.status == Transport::Http {
        vec!["progress", "pressure", "presence"]
    } else {
        vec![]
    };
    let mut status = json!({
        "contract": 2,
        "id": args.id,
        "version": env!("CARGO_PKG_VERSION"),
        "updatedAt": timestamp(Utc::now()),
        "pid": me,
        "pidStartedAt": timestamp(started_at),
        "features": features,
        "state": args.state,
        "summary": format!("Fake module, {} transport, pressure {pressure}", if args.status == Transport::Http { "HTTP" } else { "file" }),
        "components": [{ "id": "engine", "name": "Engine", "state": "ready" }],
        "resources": {
            "sampledAt": timestamp(Utc::now()),
            "intervalSeconds": args.interval_seconds,
            "processes": processes,
        },
        "jobs": shared.jobs,
    });
    if args.status == Transport::Http {
        status["presence"] = json!({ "lastSeenAt": null, "clients": 0 });
    }
    serde_json::to_string_pretty(&status).unwrap()
}

fn write_manifest(path: &Path, args: &Args, port: u16) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let install_dir = exe.parent().unwrap_or(Path::new("/")).display().to_string();
    let program = exe
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("n8tray-fake-module")
        .to_string();
    // Reproduce the arguments, minus where the manifest goes.
    let mut run_args = Vec::new();
    let mut skip = false;
    for arg in std::env::args().skip(1) {
        if skip {
            skip = false;
            continue;
        }
        if arg == "--manifest-out" {
            skip = true;
            continue;
        }
        if arg.starts_with("--manifest-out=") {
            continue;
        }
        run_args.push(arg);
    }
    let mut manifest = json!({
        "contract": 2,
        "id": args.id,
        "name": "Fake module",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "The N8 Tray conformance kit's fake module.",
        "installDir": install_dir,
        "run": { "program": program, "args": run_args },
        "trayIcon": if args.follow_lease { "hide-when-hosted" } else { "none" },
        "autoStartDefault": false,
    });
    if args.status == Transport::Http {
        manifest["status"] =
            json!({ "type": "http", "url": format!("http://127.0.0.1:{port}/n8/v2/status") });
        manifest["run"]["stop"] = json!({ "type": "http", "graceSeconds": 5 });
        manifest["actions"] = json!([
            { "id": "restart", "label": "Restart", "type": "http" },
            { "id": "refuse", "label": "Refuse", "type": "http" },
            { "id": "fail", "label": "Fail", "type": "http" },
            { "id": "engine.install", "label": "Set up {component}", "type": "http", "component": true, "progress": true },
        ]);
        manifest["lifecycle"] = json!({ "features": ["pressure", "presence"] });
    } else {
        manifest["status"] = json!({ "type": "file", "intervalSeconds": args.interval_seconds });
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    files::write_atomic(
        path,
        serde_json::to_string_pretty(&manifest).unwrap().as_bytes(),
    )
}

fn respond(request: tiny_http::Request, code: u16, body: Value) {
    let header = Header::from_bytes("Content-Type", "application/json").expect("a valid header");
    let _ = request.respond(
        Response::from_string(body.to_string())
            .with_status_code(code)
            .with_header(header),
    );
}

/// The control plane of spec 5.1, with the checks of spec section 9.
fn serve(server: Server, port: u16, token: &str, shared: &Mutex<Shared>) {
    let allowed_hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    for mut request in server.incoming_requests() {
        let header = |name: &str| {
            request
                .headers()
                .iter()
                .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
                .map(|h| h.value.as_str().to_string())
        };
        if header("Origin").is_some() {
            respond(
                request,
                403,
                json!({ "error": "requests with an Origin header are refused" }),
            );
            continue;
        }
        if !header("Host").is_some_and(|h| allowed_hosts.contains(&h)) {
            respond(request, 403, json!({ "error": "unexpected Host" }));
            continue;
        }
        let authorized = header("Authorization")
            .and_then(|v| v.strip_prefix("Bearer ").map(str::to_owned))
            .is_some_and(|given| files::constant_time_eq(&given, token));
        if !authorized {
            respond(request, 401, json!({ "error": "missing or wrong token" }));
            continue;
        }
        let method = request.method().as_str().to_string();
        let url = request.url().to_string();
        let mut body = String::new();
        let _ = request
            .as_reader()
            .take(64 * 1024)
            .read_to_string(&mut body);
        let body: Value = if body.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&body).unwrap_or(Value::Null)
        };

        match (method.as_str(), url.as_str()) {
            ("GET", "/n8/v2/status") => {
                let status = shared.lock().unwrap().status.clone();
                let header =
                    Header::from_bytes("Content-Type", "application/json").expect("a valid header");
                let _ = request.respond(Response::from_string(status).with_header(header));
            }
            ("POST", "/n8/v2/pressure") => match body.get("level").and_then(Value::as_str) {
                Some(level @ ("ok" | "attention" | "problem")) => {
                    shared.lock().unwrap().pressure = level.to_string();
                    respond(request, 200, json!({ "outcome": "ok" }));
                }
                _ => respond(
                    request,
                    400,
                    json!({ "error": "level must be ok, attention or problem" }),
                ),
            },
            ("POST", path) if path.starts_with("/n8/v2/actions/") => {
                let id = &path["/n8/v2/actions/".len()..];
                if body.is_null() {
                    respond(request, 400, json!({ "error": "the body is not JSON" }));
                    continue;
                }
                match id {
                    "restart" => respond(request, 200, json!({ "outcome": "ok" })),
                    "refuse" => respond(
                        request,
                        200,
                        json!({ "outcome": "refused", "message": "Refused on purpose." }),
                    ),
                    "fail" => respond(
                        request,
                        200,
                        json!({ "outcome": "failed", "message": "Failed on purpose." }),
                    ),
                    "stop" => {
                        shared.lock().unwrap().stop = true;
                        respond(request, 200, json!({ "outcome": "ok" }));
                    }
                    "engine.install" => match body.get("component").and_then(Value::as_str) {
                        Some("engine") => {
                            let mut shared = shared.lock().unwrap();
                            let job_id = format!("job-{}", shared.jobs.len() + 1);
                            shared.jobs.push(json!({
                                "id": job_id, "action": "engine.install", "component": "engine",
                                "state": "done", "percent": 100, "startedAt": timestamp(Utc::now()),
                            }));
                            drop(shared);
                            respond(request, 202, json!({ "jobId": job_id }));
                        }
                        _ => respond(request, 400, json!({ "error": "unknown component" })),
                    },
                    _ => respond(request, 404, json!({ "error": "unknown action" })),
                }
            }
            _ => respond(request, 404, json!({ "error": "not found" })),
        }
    }
}
