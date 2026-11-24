//! The host end to end against the conformance kit's fake module: real
//! processes, real status files and HTTP, real Job Objects on Windows, in
//! temporary folders.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use n8tray_core::folders::Folders;
use n8tray_core::host::{self, Command as HostCommand, Event, HostHandle, Options};
use n8tray_core::model::{AppView, HostSnapshot, LevelName};
use serde_json::json;
use tempfile::TempDir;

/// CI runners can be slow; nothing here normally takes more than a few seconds.
const DEADLINE: Duration = Duration::from_secs(60);

/// Builds the fake module once and returns its path.
fn fake_module() -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let output = Command::new(cargo)
            .args([
                "build",
                "--locked",
                "-p",
                "n8tray-conformance",
                "--bin",
                "n8tray-fake-module",
                "--message-format=json",
            ])
            .stderr(Stdio::inherit())
            .output()
            .expect("cargo runs");
        assert!(output.status.success(), "the fake module builds");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter_map(|v| v["executable"].as_str().map(PathBuf::from))
            .find(|p| p.file_stem().is_some_and(|s| s == "n8tray-fake-module"))
            .expect("cargo names the fake module's executable")
    })
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct Tray {
    _dir: TempDir,
    folders: Folders,
    host: Option<HostHandle>,
    latest: Arc<Mutex<Option<Arc<HostSnapshot>>>>,
    notices: Arc<Mutex<Vec<String>>>,
}

impl Tray {
    fn start() -> Tray {
        let dir = tempfile::tempdir().unwrap();
        let folders = Folders::under(dir.path());
        std::fs::create_dir_all(&folders.modules).unwrap();
        let mut tray = Tray {
            _dir: dir,
            folders,
            host: None,
            latest: Arc::new(Mutex::new(None)),
            notices: Arc::new(Mutex::new(Vec::new())),
        };
        tray.start_host();
        tray
    }

    fn start_host(&mut self) {
        let mut options = Options::new(self.folders.clone());
        options.tick = Duration::from_millis(200);
        options.sample_open = Duration::from_millis(500);
        options.sample_closed = Duration::from_millis(500);
        let latest = Arc::clone(&self.latest);
        let notices = Arc::clone(&self.notices);
        let host = host::start(options, move |event| match event {
            Event::Snapshot(s) => *latest.lock().unwrap() = Some(s),
            Event::Notify { title, body } => {
                notices.lock().unwrap().push(format!("{title}: {body}"))
            }
        })
        .expect("the host starts");
        self.host = Some(host);
    }

    fn stop_host(&mut self) {
        if let Some(host) = self.host.take() {
            host.shutdown();
        }
    }

    fn run_dir(&self) -> PathBuf {
        self.folders.run.clone().unwrap()
    }

    fn send(&self, command: HostCommand) {
        self.host.as_ref().unwrap().send(command);
    }

    fn action(&self, app: &str, action: &str, component: Option<&str>) {
        self.send(HostCommand::Action {
            app: app.into(),
            action: action.into(),
            component: component.map(str::to_string),
        });
    }

    fn write_manifest(&self, id: &str, manifest: serde_json::Value) {
        std::fs::write(
            self.folders.modules.join(format!("{id}.json")),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    /// Waits until the snapshot satisfies `check`, and returns it.
    fn wait(&self, what: &str, check: impl Fn(&HostSnapshot) -> bool) -> Arc<HostSnapshot> {
        let end = Instant::now() + DEADLINE;
        while Instant::now() < end {
            if let Some(s) = self.latest.lock().unwrap().clone()
                && check(&s)
            {
                return s;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let last = self.latest.lock().unwrap().clone();
        panic!(
            "timed out waiting for {what}; last snapshot: {}",
            serde_json::to_string_pretty(&last.as_deref()).unwrap()
        );
    }

    fn wait_app(&self, id: &str, what: &str, check: impl Fn(&AppView) -> bool) -> AppView {
        let snapshot = self.wait(what, |s| s.apps.iter().any(|a| a.id == id && check(a)));
        snapshot.apps.iter().find(|a| a.id == id).unwrap().clone()
    }

    fn lease(&self) -> Option<n8tray_contract::lease::Lease> {
        let text = std::fs::read_to_string(self.run_dir().join("host.json")).ok()?;
        n8tray_contract::lease::parse(&text)
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        // End every process the host attributed to an app, so no fake outlives a test.
        let pids: Vec<u32> = self
            .latest
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| {
                s.apps
                    .iter()
                    .flat_map(|a| a.processes.iter())
                    .filter(|p| p.verdict == "verified" || p.verdict == "unreported")
                    .map(|p| p.pid)
                    .collect()
            })
            .unwrap_or_default();
        self.stop_host();
        for pid in pids {
            let _ = n8tray_platform::signal::force_stop(pid);
        }
    }
}

/// A manifest whose `run` starts the fake module.
fn fake_manifest(
    id: &str,
    status: serde_json::Value,
    args: &[&str],
    extra: serde_json::Value,
) -> serde_json::Value {
    let exe = fake_module();
    let mut args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    args.extend([
        "--id".into(),
        id.into(),
        "--exit-after-seconds".into(),
        "300".into(),
    ]);
    let mut manifest = json!({
        "contract": 2,
        "id": id,
        "name": format!("Fake {id}"),
        "version": "1.0.0",
        "installDir": exe.parent().unwrap().display().to_string(),
        "status": status,
        "run": { "program": exe.file_name().unwrap().to_string_lossy(), "args": args },
    });
    if let (Some(m), Some(e)) = (manifest.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            if k == "run" {
                for (rk, rv) in v.as_object().unwrap() {
                    m["run"][rk] = rv.clone();
                }
            } else {
                m.insert(k.clone(), v.clone());
            }
        }
    }
    manifest
}

/// A fake module started by the test, not the host, with its output lines.
struct External {
    child: Child,
    lines: Receiver<String>,
}

impl External {
    fn start(run_dir: &Path, args: &[&str]) -> External {
        let mut command = Command::new(fake_module());
        command.args(args);
        External::spawn(command, run_dir)
    }

    /// Like [`External::start`], ignoring the polite stop request (SIGTERM;
    /// on Windows a windowless fake ignores the close request anyway).
    fn start_stubborn(run_dir: &Path, args: &[&str]) -> External {
        #[cfg(unix)]
        let command = {
            // An ignored signal stays ignored across exec.
            let mut command = Command::new("sh");
            command
                .arg("-c")
                .arg("trap '' TERM; exec \"$0\" \"$@\"")
                .arg(fake_module())
                .args(args);
            command
        };
        #[cfg(windows)]
        let command = {
            let mut command = Command::new(fake_module());
            command.args(args);
            command
        };
        External::spawn(command, run_dir)
    }

    fn spawn(mut command: Command, run_dir: &Path) -> External {
        let mut child = command
            .args(["--exit-after-seconds", "300"])
            .env("N8TRAY_RUN_DIR", run_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        External { child, lines }
    }

    fn expect_line(&self, wanted: &str, within: Duration) {
        let end = Instant::now() + within;
        let mut seen = Vec::new();
        while Instant::now() < end {
            if let Ok(line) = self.lines.recv_timeout(Duration::from_millis(200)) {
                if line == wanted {
                    return;
                }
                seen.push(line);
            }
        }
        panic!("the fake module never printed {wanted:?}; it printed {seen:?}");
    }
}

impl Drop for External {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn starts_an_http_app_runs_its_declared_actions_and_stops_it() {
    let tray = Tray::start();
    let port = free_port().to_string();
    tray.write_manifest(
        "fakehttp",
        fake_manifest(
            "fakehttp",
            json!({ "type": "http", "url": format!("http://127.0.0.1:{port}/n8/v2/status"), "intervalSeconds": 1, "timeoutMs": 1000 }),
            &["--status", "http", "--port", &port, "--interval-seconds", "1"],
            json!({
                "run": { "stop": { "type": "http", "graceSeconds": 5 } },
                "actions": [
                    { "id": "refuse", "label": "Refuse", "type": "http" },
                    { "id": "fail", "label": "Fail", "type": "http" },
                    { "id": "engine.install", "label": "Set up {component}", "type": "http", "component": true, "progress": true }
                ],
                "autoStartDefault": true
            }),
        ),
    );

    // Started with the host (autoStartDefault), then ready with a verified tree.
    let app = tray.wait_app("fakehttp", "the HTTP app to be ready", |a| {
        a.state == "ready"
    });
    assert_eq!(app.level, LevelName::Ok);
    assert!(app.should_run && app.running);
    assert!(app.memory_mb.is_some_and(|m| m > 0.0));
    assert!(app.processes.iter().any(|p| p.verdict == "verified"));
    assert_eq!(app.components.len(), 1);
    let lease = tray.lease().expect("the lease is written");
    assert!(lease.hosting.contains(&"fakehttp".to_string()));
    assert_eq!(lease.pid, std::process::id());

    // Declared actions answer with their outcome; anything else is refused.
    tray.action("fakehttp", "refuse", None);
    tray.wait_app("fakehttp", "a refused outcome", |a| {
        a.last_outcome
            .as_ref()
            .is_some_and(|o| o.action == "refuse" && o.outcome == "refused")
    });
    tray.action("fakehttp", "fail", None);
    tray.wait_app("fakehttp", "a failed outcome", |a| {
        a.last_outcome
            .as_ref()
            .is_some_and(|o| o.action == "fail" && o.outcome == "failed")
    });
    tray.action("fakehttp", "engine.install", Some("engine"));
    tray.wait_app("fakehttp", "the job to start", |a| {
        a.last_outcome
            .as_ref()
            .is_some_and(|o| o.action == "engine.install" && o.outcome == "ok")
    });
    tray.action("fakehttp", "engine.install", Some("nope"));
    tray.wait_app("fakehttp", "an unknown component refused", |a| {
        a.last_outcome
            .as_ref()
            .is_some_and(|o| o.action == "engine.install" && o.outcome == "refused")
    });
    tray.action("fakehttp", "delete-everything", None);
    tray.wait_app("fakehttp", "an undeclared action refused", |a| {
        a.last_outcome
            .as_ref()
            .is_some_and(|o| o.action == "delete-everything" && o.outcome == "refused")
    });

    // Stop through POST /n8/v2/actions/stop; not restarted, not a problem.
    tray.action("fakehttp", "stop", None);
    let app = tray.wait_app("fakehttp", "the app to stop", |a| {
        !a.running && a.pending.is_none()
    });
    assert_eq!(app.state, "stopped");
    assert_eq!(app.level, LevelName::Off);
    assert!(!app.should_run);
    std::thread::sleep(Duration::from_secs(6));
    let app = tray.wait_app("fakehttp", "the app to stay stopped", |_| true);
    assert!(!app.running, "a stop is not a crash");

    // Start again by hand.
    tray.action("fakehttp", "start", None);
    tray.wait_app("fakehttp", "the app to be ready again", |a| {
        a.state == "ready"
    });
}

#[test]
fn counts_hidden_children_and_restarts_after_a_crash() {
    let tray = Tray::start();
    tray.write_manifest(
        "fakefile",
        fake_manifest(
            "fakefile",
            json!({ "type": "file", "intervalSeconds": 1 }),
            &[
                "--status",
                "file",
                "--children",
                "2",
                "--hide-children",
                "--interval-seconds",
                "1",
            ],
            json!({}),
        ),
    );
    let app = tray.wait_app("fakefile", "the app to be found", |_| true);
    assert_eq!(app.state, "stopped");
    assert!(!app.should_run, "autoStartDefault is false");

    tray.action("fakefile", "start", None);
    // The workers are counted although the app hides them. (On Windows each
    // also gets a conhost.exe, which is the app's too and counted as well.)
    let app = tray.wait_app("fakefile", "two unreported workers", |a| {
        a.state == "ready"
            && a.processes
                .iter()
                .filter(|p| p.verdict == "unreported" && p.name.starts_with("n8tray-fake-mod"))
                .count()
                == 2
    });
    let root = app
        .processes
        .iter()
        .find(|p| p.verdict == "verified")
        .expect("the main process is verified")
        .pid;

    // A crash: the host restarts it after the first backoff (5 s).
    n8tray_platform::signal::force_stop(root).unwrap();
    let app = tray.wait_app("fakefile", "the restart to be planned", |a| {
        a.summary.starts_with("Exited; restarting")
    });
    assert_eq!(app.level, LevelName::Attention);
    tray.wait_app("fakefile", "the app to run again", |a| {
        a.state == "ready"
            && a.processes
                .iter()
                .any(|p| p.verdict == "verified" && p.pid != root)
    });
}

#[test]
fn rejects_a_process_the_app_does_not_own() {
    let tray = Tray::start();
    // A process that is not the fake's: a second, unrelated fake.
    let other = External::start(&tray.run_dir(), &["--id", "bystander", "--status", "file"]);
    let lie = other.child.id().to_string();
    tray.write_manifest(
        "liar",
        fake_manifest(
            "liar",
            json!({ "type": "file", "intervalSeconds": 1 }),
            &[
                "--status",
                "file",
                "--lie-pid",
                &lie,
                "--interval-seconds",
                "1",
            ],
            json!({ "autoStartDefault": true }),
        ),
    );
    let app = tray.wait_app("liar", "the lie to be rejected", |a| {
        a.state == "ready" && a.processes.iter().any(|p| p.pid.to_string() == lie)
    });
    let claimed = app
        .processes
        .iter()
        .find(|p| p.pid.to_string() == lie)
        .unwrap();
    assert!(
        claimed.verdict == "rejected" || claimed.verdict == "unverified",
        "the claimed process is never counted: {claimed:?}"
    );
    // The bystander lives in the same folder as the fake, so step 4 (location)
    // may call it unverified; either way it is not counted.
    let counted: f64 = app
        .processes
        .iter()
        .filter(|p| p.verdict == "verified" || p.verdict == "unreported")
        .filter_map(|p| p.rss_mb)
        .sum();
    assert!((app.memory_mb.unwrap() - counted).abs() < 0.2);
}

#[test]
fn a_status_naming_a_program_from_elsewhere_is_not_adopted() {
    let tray = Tray::start();
    // The app is installed in a folder of its own, but a status document in
    // its name comes from a program somewhere else (here the fake module in
    // the build folder): it must not become the app's root process.
    let install = tempfile::tempdir().unwrap();
    let mut impostor = External::start(
        &tray.run_dir(),
        &[
            "--id",
            "placed",
            "--status",
            "file",
            "--interval-seconds",
            "1",
        ],
    );
    let pid = impostor.child.id();
    tray.write_manifest(
        "placed",
        json!({
            "contract": 2,
            "id": "placed",
            "name": "Placed",
            "version": "1.0.0",
            "installDir": install.path().display().to_string(),
            "status": { "type": "file", "intervalSeconds": 1 },
            "run": { "program": "placed", "stop": { "type": "terminate", "graceSeconds": 2 } },
        }),
    );
    let app = tray.wait_app("placed", "the status to be read", |a| {
        a.state == "ready" && a.processes.iter().any(|p| p.pid == pid)
    });
    let shown = app.processes.iter().find(|p| p.pid == pid).unwrap();
    assert_eq!(shown.verdict, "rejected", "{shown:?}");
    assert!(
        app.processes
            .iter()
            .all(|p| p.verdict != "verified" && p.verdict != "unreported"),
        "nothing of the impostor is counted: {:?}",
        app.processes
    );

    // A stop has nothing of the app's to act on, and the impostor lives on.
    tray.action("placed", "stop", None);
    std::thread::sleep(Duration::from_secs(4));
    assert!(
        matches!(impostor.child.try_wait(), Ok(None)),
        "the impostor was not touched"
    );
}

#[test]
fn an_app_with_its_own_icon_hides_it_while_hosted() {
    let mut tray = Tray::start();
    let manifest = tray.folders.modules.join("iconapp.json");
    let app = External::start(
        &tray.run_dir(),
        &[
            "--id",
            "iconapp",
            "--status",
            "http",
            "--follow-lease",
            "--lease-poll-ms",
            "200",
            "--manifest-out",
            &manifest.display().to_string(),
        ],
    );
    // The app found running is adopted from its status and listed in the lease.
    app.expect_line("tray: hosted", DEADLINE);
    let view = tray.wait_app("iconapp", "the adopted app", |a| {
        a.state == "ready" && a.processes.iter().any(|p| p.verdict == "verified")
    });
    assert!(view.running && view.memory_mb.is_some());
    assert!(
        tray.lease()
            .unwrap()
            .hosting
            .contains(&"iconapp".to_string())
    );

    // "Use its own icon": no longer hosted, so the app shows its icon.
    tray.send(HostCommand::SetOwnIcon {
        app: "iconapp".into(),
        on: true,
    });
    app.expect_line("tray: own", DEADLINE);
    tray.send(HostCommand::SetOwnIcon {
        app: "iconapp".into(),
        on: false,
    });
    app.expect_line("tray: hosted", DEADLINE);

    // The host quits: the lease goes stale and the icon comes back.
    tray.stop_host();
    app.expect_line("tray: own", Duration::from_secs(30));
}

#[test]
fn forces_a_stop_only_after_the_grace_period() {
    let tray = Tray::start();
    tray.write_manifest(
        "stubborn",
        fake_manifest(
            "stubborn",
            json!({ "type": "file", "intervalSeconds": 1 }),
            &["--status", "file", "--children", "1", "--interval-seconds", "1"],
            json!({ "run": { "stop": { "type": "terminate", "graceSeconds": 2 } }, "autoStartDefault": true }),
        ),
    );
    let app = tray.wait_app("stubborn", "the app and its child", |a| {
        a.state == "ready"
            && a.processes
                .iter()
                .filter(|p| p.verdict == "verified")
                .count()
                == 2
    });
    let pids: Vec<u32> = app.processes.iter().map(|p| p.pid).collect();
    tray.action("stubborn", "stop", None);
    let app = tray.wait_app("stubborn", "the stop to finish", |a| {
        a.last_outcome.as_ref().is_some_and(|o| o.action == "stop")
    });
    assert_eq!(app.last_outcome.as_ref().unwrap().outcome, "ok");
    if cfg!(windows) {
        // A windowless app ignores the close request; the Job Object ends it all.
        assert_eq!(
            app.last_outcome.as_ref().unwrap().message.as_deref(),
            Some("Stopped by force after the grace period")
        );
    }
    // Every process of the app is gone, children included.
    let end = Instant::now() + DEADLINE;
    loop {
        let mut processes = n8tray_platform::process::Processes::new();
        let snapshot = processes.snapshot().unwrap();
        let alive: Vec<u32> = pids
            .iter()
            .copied()
            .filter(|pid| snapshot.processes.iter().any(|p| p.pid == *pid))
            .collect();
        if alive.is_empty() {
            break;
        }
        assert!(Instant::now() < end, "still running: {alive:?}");
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// A tray hosting one running fake app. Each test needs its own `id`: tests
/// run in parallel, and on Windows the app's Job Object is named after its
/// id (`Local\N8Tray.App.<id>`), so two tests with one id share a job.
fn tray_with_a_running_app(id: &str) -> Tray {
    let tray = Tray::start();
    tray.write_manifest(
        id,
        fake_manifest(
            id,
            json!({ "type": "file", "intervalSeconds": 1 }),
            &["--status", "file", "--interval-seconds", "1"],
            json!({ "autoStartDefault": true }),
        ),
    );
    tray.wait_app(id, "the app to be ready", |a| a.state == "ready");
    tray
}

#[test]
fn a_failure_in_the_host_is_shown_and_the_host_starts_again() {
    let tray = tray_with_a_running_app("steady");
    tray.send(HostCommand::FailForTest);
    let snapshot = tray.wait("the failure to be shown", |s| {
        s.level == LevelName::Problem
            && s.reason
                .as_deref()
                .is_some_and(|r| r.contains("app monitor") && r.contains("restarted"))
    });
    assert_eq!(snapshot.headline, "Problem");
    assert!(snapshot.tooltip.contains("Problem"), "{}", snapshot.tooltip);
    assert!(
        tray.notices
            .lock()
            .unwrap()
            .iter()
            .any(|n| n.contains("app monitor")),
        "a notification says so"
    );

    // The new host finds the running app again (its own snapshots list the
    // running apps in the tooltip) and acts on commands.
    tray.wait("the new host's snapshot", |s| {
        s.tooltip.contains("Running: Fake steady") && s.tooltip.contains("restarted")
    });
    tray.wait_app("steady", "the app to be found again", |a| {
        a.state == "ready" && a.processes.iter().any(|p| p.verdict == "verified")
    });
    tray.action("steady", "stop", None);
    let app = tray.wait_app("steady", "the stop's outcome", |a| {
        a.last_outcome.as_ref().is_some_and(|o| o.action == "stop")
    });
    let outcome = app.last_outcome.unwrap();
    assert_eq!(outcome.outcome, "ok", "{outcome:?}");
    tray.wait_app("steady", "the app to stop", |a| {
        a.state == "stopped" && !a.running
    });
    let snapshot = tray.wait("the snapshot after the stop", |_| true);
    assert_eq!(
        snapshot.level,
        LevelName::Problem,
        "the failure stays visible for a while"
    );

    // What the user chose stands across a restart: the app they stopped is
    // not started again by its start-with-N8-Tray setting.
    tray.send(HostCommand::FailForTest);
    let end = Instant::now() + DEADLINE;
    while tray
        .notices
        .lock()
        .unwrap()
        .iter()
        .filter(|n| n.contains("app monitor"))
        .count()
        < 2
    {
        assert!(Instant::now() < end, "the second failure is shown");
        std::thread::sleep(Duration::from_millis(100));
    }
    // Long enough for an unwanted start to show (the fake reports in 1 s).
    std::thread::sleep(Duration::from_secs(4));
    let app = tray.wait_app("steady", "the app", |_| true);
    assert!(!app.running && !app.should_run, "{app:?}");
}

#[test]
fn a_host_that_keeps_failing_says_it_stopped() {
    let tray = tray_with_a_running_app("failing");
    for _ in 0..3 {
        // Each failure waits for the restarted host before the next.
        tray.send(HostCommand::FailForTest);
        std::thread::sleep(Duration::from_millis(1500));
    }
    let snapshot = tray.wait("the host to say it stopped", |s| {
        s.level == LevelName::Problem
            && s.reason
                .as_deref()
                .is_some_and(|r| r.contains("app monitor stopped"))
    });
    assert!(snapshot.tooltip.contains("stopped"), "{}", snapshot.tooltip);
}

#[test]
fn small_figures_read_right() {
    let tray = Tray::start();
    let snapshot = tray.wait("a sampled snapshot", |s| s.tray.rss_mb > 0.0);
    let json = serde_json::to_string(&*snapshot).unwrap();
    assert!(!json.contains("-0.0"), "no negative zeros: {json}");
    // The tray alone is tens of MB: said in MB, not as "0.0 GB".
    let n8 = snapshot.memory.n8_mb;
    assert!(n8 > 0.0 && n8 < 1000.0, "{n8}");
    let text = &snapshot.memory.text;
    let said: f64 = text
        .strip_suffix(" MB")
        .and_then(|t| t.rsplit_once("· N8 "))
        .and_then(|(_, n)| n.parse().ok())
        .unwrap_or_else(|| panic!("the N8 figure in MB: {text}"));
    assert!((said - n8).abs() <= 1.0, "{text} for {n8} MB");
}

#[test]
fn an_app_found_running_is_asked_to_stop_but_never_forced() {
    let tray = Tray::start();
    // Started outside the host (by hand, or by another tray), and it ignores
    // the polite request.
    let mut found = External::start_stubborn(
        &tray.run_dir(),
        &[
            "--id",
            "found",
            "--status",
            "file",
            "--interval-seconds",
            "1",
        ],
    );
    let pid = found.child.id();
    tray.write_manifest(
        "found",
        fake_manifest(
            "found",
            json!({ "type": "file", "intervalSeconds": 1 }),
            &["--status", "file", "--interval-seconds", "1"],
            json!({ "run": { "stop": { "type": "terminate", "graceSeconds": 2 } } }),
        ),
    );
    tray.wait_app("found", "the running app to be adopted", |a| {
        a.state == "ready"
            && a.processes
                .iter()
                .any(|p| p.pid == pid && p.verdict == "verified")
    });
    tray.action("found", "stop", None);
    let app = tray.wait_app("found", "the stop to give up", |a| {
        a.last_outcome.as_ref().is_some_and(|o| o.action == "stop")
    });
    let outcome = app.last_outcome.unwrap();
    assert_eq!(outcome.outcome, "failed", "{outcome:?}");
    assert!(
        outcome
            .message
            .as_deref()
            .is_some_and(|m| m.contains("force-stops only apps it started")),
        "{outcome:?}"
    );
    std::thread::sleep(Duration::from_secs(1));
    assert!(
        matches!(found.child.try_wait(), Ok(None)),
        "the app N8 Tray did not start is still running"
    );
}

#[test]
fn reports_manifests_it_cannot_use() {
    let tray = Tray::start();
    std::fs::write(tray.folders.modules.join("broken.json"), "{ nope").unwrap();
    let snapshot = tray.wait("the problem row", |s| !s.problems.is_empty());
    assert!(snapshot.problems[0].message.starts_with("Manifest invalid"));
    assert_eq!(snapshot.level, LevelName::Attention);
    // Fixed: the problem goes away (the folder is watched).
    std::fs::remove_file(tray.folders.modules.join("broken.json")).unwrap();
    tray.wait("no problem rows", |s| s.problems.is_empty());
}

#[test]
fn one_host_at_a_time() {
    let tray = Tray::start();
    let second = host::start(Options::new(tray.folders.clone()), |_| {});
    assert!(matches!(second, Err(host::StartError::AnotherHost)));
    // The host writes its own status file.
    let end = Instant::now() + DEADLINE;
    while !tray.folders.status_file.exists() {
        assert!(Instant::now() < end, "the status file appears");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Starts a tray whose settings are written first.
fn tray_with_settings(settings: serde_json::Value) -> Tray {
    let dir = tempfile::tempdir().unwrap();
    let folders = Folders::under(dir.path());
    std::fs::create_dir_all(&folders.modules).unwrap();
    std::fs::write(&folders.settings_file, settings.to_string()).unwrap();
    let mut tray = Tray {
        _dir: dir,
        folders,
        host: None,
        latest: Arc::new(Mutex::new(None)),
        notices: Arc::new(Mutex::new(Vec::new())),
    };
    tray.start_host();
    tray
}

#[test]
fn pushes_memory_pressure_and_measures_cpu_and_history() {
    // Thresholds no computer meets: the first reading is a problem at once.
    let tray = tray_with_settings(json!({
        "memory": { "attentionBelowGiB": 200000, "problemBelowGiB": 100000 }
    }));
    let port = free_port().to_string();
    tray.write_manifest(
        "pressured",
        fake_manifest(
            "pressured",
            json!({ "type": "http", "url": format!("http://127.0.0.1:{port}/n8/v2/status"), "intervalSeconds": 1, "timeoutMs": 1000 }),
            &["--status", "http", "--port", &port, "--interval-seconds", "1"],
            json!({ "lifecycle": { "features": ["pressure"] }, "autoStartDefault": true }),
        ),
    );
    let snapshot = tray.wait("the problem level", |s| {
        s.memory.level == LevelName::Problem && s.pressure == LevelName::Problem
    });
    assert_eq!(snapshot.level, LevelName::Problem);
    // The app hears it (its summary repeats the level it got).
    tray.wait_app("pressured", "the pushed level", |a| {
        a.summary.ends_with("pressure problem")
    });
    assert!(
        !tray
            .notices
            .lock()
            .unwrap()
            .iter()
            .any(|n| n.starts_with("Low memory")),
        "low-memory notifications are off unless turned on"
    );
    // CPU needs two samples; history fills in 10 s steps.
    let app = tray.wait_app("pressured", "CPU and history", |a| {
        a.cpu_percent.is_some()
            && a.history
                .as_ref()
                .is_some_and(|h| h.memory_mb.iter().any(|m| *m > 0.0))
    });
    assert_eq!(app.history.unwrap().memory_mb.len(), 90);
    let snapshot = tray.wait("the tray's own CPU", |s| s.tray.cpu_percent.is_some());
    assert!(snapshot.tray.history.is_some());
}

#[test]
fn alerts_when_an_app_goes_over_its_budget() {
    let tray = Tray::start();
    tray.write_manifest(
        "hungry",
        fake_manifest(
            "hungry",
            json!({ "type": "file", "intervalSeconds": 1 }),
            &[
                "--status",
                "file",
                "--eat-mb",
                "64",
                "--interval-seconds",
                "1",
            ],
            json!({
                "resources": { "budget": { "warnMb": 10, "maxMb": 20, "sustainSeconds": 10 } },
                "autoStartDefault": true
            }),
        ),
    );
    let app = tray.wait_app("hungry", "the over-limit alert", |a| {
        a.alert.is_some() && a.level == LevelName::Problem
    });
    assert_eq!(app.alert.as_deref(), Some("Over its 20 MB limit"));
    let notices = tray.notices.lock().unwrap().clone();
    assert_eq!(
        notices
            .iter()
            .filter(|n| n.contains("Over its 20 MB limit"))
            .count(),
        1,
        "once per episode: {notices:?}"
    );
}

/// Waits for a file the started script writes and returns its text.
#[cfg(windows)]
fn wait_for_file(path: &Path) -> String {
    let end = Instant::now() + DEADLINE;
    while Instant::now() < end {
        if let Ok(text) = std::fs::read_to_string(path)
            && !text.trim().is_empty()
        {
            return text;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("the script never wrote {}", path.display());
}

/// Starts a script app the way the installed ones are (an `interpreter` and a
/// `script` in a folder of its own) and returns what the script wrote: the
/// folder it found itself in and its working folder.
#[cfg(windows)]
fn run_script_app(id: &str, interpreter: &str, file: &str, script: &str) -> String {
    let tray = Tray::start();
    let install = tempfile::tempdir().unwrap();
    std::fs::write(install.path().join(file), script).unwrap();
    tray.write_manifest(
        id,
        json!({
            "contract": 2,
            "id": id,
            "name": format!("Script {id}"),
            "version": "1.0.0",
            "installDir": install.path().display().to_string(),
            "status": { "type": "file", "intervalSeconds": 5 },
            "run": { "interpreter": interpreter, "script": file, "args": [] },
        }),
    );
    tray.wait_app(id, "the app to be found", |_| true);
    tray.action(id, "start", None);
    let text = wait_for_file(&install.path().join("started.txt"));
    // The script exits at once; give the host a moment to notice before the
    // install folder goes away.
    std::thread::sleep(Duration::from_millis(500));
    text
}

#[cfg(windows)]
#[test]
fn starts_a_powershell_script_that_finds_files_next_to_itself() {
    let text = run_script_app(
        "psscript",
        "powershell",
        "start.ps1",
        "$ErrorActionPreference = 'Stop'\r\n\
         $out = Join-Path $PSScriptRoot 'started.txt'\r\n\
         Set-Content -LiteralPath $out -Value \"root=$PSScriptRoot cwd=$((Get-Location).ProviderPath)\"\r\n",
    );
    assert!(
        !text.contains(r"\\?\"),
        "paths without the \\\\?\\ prefix: {text}"
    );
}

#[cfg(windows)]
#[test]
fn starts_a_node_script_that_finds_files_next_to_itself() {
    if n8tray_core::resolve::which("node").is_none() {
        eprintln!("node is not installed; skipped");
        return;
    }
    let text = run_script_app(
        "nodescript",
        "node",
        "start.js",
        "const path = require('path');\n\
         require('fs').writeFileSync(path.join(__dirname, 'started.txt'), `root=${__dirname} cwd=${process.cwd()}`);\n",
    );
    assert!(
        !text.contains(r"\\?\"),
        "paths without the \\\\?\\ prefix: {text}"
    );
}

#[test]
fn an_app_with_a_demand_rule_sleeps_until_its_process_runs() {
    let tray = Tray::start();
    // A program named "demandprobe" plays the browser.
    let probe_dir = tempfile::tempdir().unwrap();
    let probe = probe_dir
        .path()
        .join(format!("demandprobe{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(fake_module(), &probe).unwrap();
    tray.write_manifest(
        "sleepy",
        fake_manifest(
            "sleepy",
            json!({ "type": "file", "intervalSeconds": 1 }),
            &["--status", "file", "--interval-seconds", "1"],
            json!({
                "lifecycle": { "demand": { "processes": ["demandprobe"], "stopAfterMinutes": 1 } },
                "autoStartDefault": true
            }),
        ),
    );
    let app = tray.wait_app("sleepy", "the app to sleep", |a| a.state == "idle");
    assert_eq!(app.level, LevelName::Ok, "sleeping is healthy");
    assert!(!app.running);

    let mut child = Command::new(&probe)
        .args([
            "--id",
            "probe",
            "--status",
            "file",
            "--exit-after-seconds",
            "120",
        ])
        .env("N8TRAY_RUN_DIR", tray.run_dir())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let app = tray.wait_app("sleepy", "the app to wake up", |a| a.state == "ready");
    assert!(app.running);
    let _ = child.kill();
    let _ = child.wait();
}
