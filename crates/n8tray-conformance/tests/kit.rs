//! The conformance kit end to end: the real binaries, in temporary folders.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use n8tray_conformance::http::Request;
use tempfile::TempDir;

const CONTRACT: &str = env!("CARGO_BIN_EXE_n8tray-contract");
const FAKE: &str = env!("CARGO_BIN_EXE_n8tray-fake-module");
/// CI runners can be slow; nothing here normally takes more than a few seconds.
const DEADLINE: Duration = Duration::from_secs(60);

fn spec() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec")
}

/// A child process that is killed when the test ends, pass or fail.
struct Running {
    child: Child,
    lines: Option<Receiver<String>>,
}

impl Running {
    fn start(program: &str, args: &[&str]) -> Running {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap_or_else(|e| panic!("{program}: {e}"));
        let stdout = child.stdout.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Running {
            child,
            lines: Some(receiver),
        }
    }

    /// Waits for a line of output equal to `wanted`.
    fn expect_line(&mut self, wanted: &str) {
        let lines = self.lines.as_ref().unwrap();
        let end = Instant::now() + DEADLINE;
        let mut seen = Vec::new();
        while Instant::now() < end {
            match lines.recv_timeout(Duration::from_millis(200)) {
                Ok(line) if line == wanted => return,
                Ok(line) => seen.push(line),
                Err(_) => {}
            }
        }
        panic!("no line \"{wanted}\" within {DEADLINE:?}; saw {seen:?}");
    }

    fn wait_exit(&mut self) -> bool {
        let end = Instant::now() + DEADLINE;
        while Instant::now() < end {
            if self.child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_for_file(path: &Path) {
    let end = Instant::now() + DEADLINE;
    while !path.exists() {
        assert!(
            Instant::now() < end,
            "{} did not appear within {DEADLINE:?}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn contract(args: &[&str]) -> Output {
    Command::new(CONTRACT).args(args).output().unwrap()
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

struct Fixture {
    _dir: TempDir,
    run: PathBuf,
    manifest: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let run = dir.path().join("run");
        let manifest = dir.path().join("modules").join("fake.json");
        Fixture {
            _dir: dir,
            run,
            manifest,
        }
    }

    fn run(&self) -> &str {
        self.run.to_str().unwrap()
    }

    /// Starts the fake module and waits until it has written its manifest,
    /// then one status interval more so CPU figures exist.
    fn start_fake(&self, extra: &[&str]) -> Running {
        let mut args = vec![
            "--run-dir",
            self.run(),
            "--manifest-out",
            self.manifest.to_str().unwrap(),
            "--interval-seconds",
            "1",
        ];
        args.extend_from_slice(extra);
        let fake = Running::start(FAKE, &args);
        wait_for_file(&self.manifest);
        std::thread::sleep(Duration::from_millis(1500));
        fake
    }

    fn probe(&self, extra: &[&str]) -> Output {
        let mut args = vec![
            "probe",
            self.manifest.to_str().unwrap(),
            "--run-dir",
            self.run(),
            "--timeout-ms",
            "5000",
        ];
        args.extend_from_slice(extra);
        contract(&args)
    }
}

fn assert_passes(output: &Output) {
    let out = text(output);
    assert!(
        output.status.success() && !out.contains("FAIL "),
        "the probe should pass:\n{out}"
    );
}

#[test]
fn check_accepts_the_examples_and_rejects_invalid_fixtures() {
    let examples = spec().join("examples");
    let output = contract(&["check", examples.to_str().unwrap()]);
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(text(&output).matches("ok ").count(), 5);

    let invalid = spec().join("fixtures/manifest/invalid/budget.warn-above-max.json");
    let output = contract(&["check", "--kind", "manifest", invalid.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output).contains("[budget]"), "{}", text(&output));

    let output = contract(&["check", "--json", invalid.to_str().unwrap()]);
    let reports: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reports[0]["valid"], false);
    let codes: Vec<_> = reports[0]["report"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["code"].clone())
        .collect();
    assert!(codes.contains(&serde_json::json!("budget")), "{codes:?}");

    let lease = spec().join("fixtures/lease/valid/phase0-powershell.json");
    let output = contract(&["check", lease.to_str().unwrap()]);
    assert!(
        output.status.success() && text(&output).contains("(lease)"),
        "{}",
        text(&output)
    );

    let output = contract(&["check", "no-such-file.json"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn schema_prints_each_schema() {
    for kind in ["manifest", "status", "lease", "action-result", "modular-v1"] {
        let output = contract(&["schema", kind]);
        let schema: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
    }
}

#[test]
fn an_http_app_with_children_passes_the_probe_and_stops_over_http() {
    let fixture = Fixture::new();
    let mut fake = fixture.start_fake(&["--status", "http", "--children", "2"]);
    let output = fixture.probe(&["--action", "engine.install", "--component", "engine"]);
    assert_passes(&output);
    let out = text(&output);
    for check in [
        "auth",
        "origin",
        "host",
        "no-secrets",
        "pressure",
        "action",
        "ownership",
        "unreported",
    ] {
        assert!(
            out.contains(&format!("PASS {check}")),
            "no PASS {check}:\n{out}"
        );
    }
    assert!(out.contains("every reported process (3)"), "{out}");

    // The stop the host sends for stop type http.
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture.manifest).unwrap()).unwrap();
    let url = manifest["status"]["url"].as_str().unwrap();
    let port: u16 = url
        .trim_start_matches("http://127.0.0.1:")
        .split('/')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let token = std::fs::read_to_string(fixture.run.join("fake.token")).unwrap();
    let response = Request::new("POST", port, "/n8/v2/actions/stop", Duration::from_secs(5))
        .header("Authorization", format!("Bearer {}", token.trim()))
        .json("{}")
        .send()
        .unwrap();
    assert_eq!(response.status, 200);
    assert!(fake.wait_exit(), "the fake module did not stop");
}

#[test]
fn a_file_status_app_passes_the_probe() {
    let fixture = Fixture::new();
    let _fake = fixture.start_fake(&["--status", "file", "--children", "1"]);
    let output = fixture.probe(&[]);
    assert_passes(&output);
    assert!(
        text(&output).contains("PASS status-file"),
        "{}",
        text(&output)
    );
}

#[test]
fn the_probe_rejects_a_process_the_app_does_not_own() {
    let fixture = Fixture::new();
    let me = std::process::id().to_string();
    let _fake = fixture.start_fake(&["--status", "file", "--lie-pid", &me]);
    let output = fixture.probe(&[]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(
        text(&output).contains(&format!("FAIL ownership: pid {me} does not descend")),
        "{}",
        text(&output)
    );
}

#[test]
fn the_probe_flags_children_the_app_hides() {
    let fixture = Fixture::new();
    let _fake = fixture.start_fake(&["--status", "file", "--children", "1", "--hide-children"]);
    let output = fixture.probe(&[]);
    assert!(output.status.success(), "{}", text(&output));
    assert!(
        text(&output).contains("WARN unreported"),
        "{}",
        text(&output)
    );
}

#[test]
fn the_probe_fails_when_the_app_is_not_running() {
    let fixture = Fixture::new();
    let mut fake = fixture.start_fake(&["--status", "file", "--exit-after-seconds", "1"]);
    assert!(fake.wait_exit());
    let output = fixture.probe(&[]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(
        text(&output).contains("FAIL main-process"),
        "{}",
        text(&output)
    );
}

#[test]
fn the_test_tray_writes_a_valid_lease() {
    let fixture = Fixture::new();
    let output = contract(&[
        "host",
        "--run-dir",
        fixture.run(),
        "--hosting",
        "fake,voicedock",
        "--once",
    ]);
    assert!(output.status.success(), "{}", text(&output));
    let lease = fixture.run.join("host.json");
    let output = contract(&["check", lease.to_str().unwrap()]);
    assert!(
        output.status.success() && text(&output).contains("(lease)"),
        "{}",
        text(&output)
    );
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&lease).unwrap()).unwrap();
    assert_eq!(value["hosting"], serde_json::json!(["fake", "voicedock"]));
}

#[test]
fn an_app_hides_its_icon_while_the_test_tray_hosts_it_and_shows_it_when_the_tray_quits() {
    let fixture = Fixture::new();
    let host = Running::start(
        CONTRACT,
        &["host", "--run-dir", fixture.run(), "--hosting", "fake"],
    );
    wait_for_file(&fixture.run.join("host.json"));
    let mut fake = Running::start(
        FAKE,
        &[
            "--run-dir",
            fixture.run(),
            "--status",
            "file",
            "--follow-lease",
            "--lease-poll-ms",
            "200",
        ],
    );
    fake.expect_line("tray: hosted");
    drop(host); // the host quits; its pid is gone, so the lease no longer hosts anyone
    fake.expect_line("tray: own");
}

#[test]
fn an_app_the_tray_does_not_list_keeps_its_icon() {
    let fixture = Fixture::new();
    let _host = Running::start(
        CONTRACT,
        &["host", "--run-dir", fixture.run(), "--hosting", "voicedock"],
    );
    wait_for_file(&fixture.run.join("host.json"));
    let mut fake = Running::start(
        FAKE,
        &[
            "--run-dir",
            fixture.run(),
            "--status",
            "file",
            "--follow-lease",
            "--lease-poll-ms",
            "200",
        ],
    );
    fake.expect_line("tray: own");
}
