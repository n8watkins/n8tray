//! `n8tray-contract probe`: checks a running app against its manifest (the
//! checks are listed in the kit's README).

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Serialize;
use serde_json::Value;

use n8tray_contract::manifest::{ActionKind, LifecycleFeature, Manifest, StatusSource};
use n8tray_contract::status::{Feature, Status};
use n8tray_contract::{Kind, parse_json, validate_as};

use crate::files;
use crate::http::Request;
use crate::procs::{Table, same_start};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Pass,
    Warn,
    Fail,
}

/// One check's result.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub verdict: Verdict,
    pub check: &'static str,
    pub message: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let verdict = match self.verdict {
            Verdict::Pass => "PASS",
            Verdict::Warn => "WARN",
            Verdict::Fail => "FAIL",
        };
        write!(f, "{verdict} {}: {}", self.check, self.message)
    }
}

/// What to probe and how.
#[derive(Debug, Clone)]
pub struct Options {
    pub run_dir: PathBuf,
    /// Overrides the manifest's `status.timeoutMs` (CI runners can be slow).
    pub timeout: Option<Duration>,
    /// Also invoke this action, with this component.
    pub action: Option<(String, Option<String>)>,
}

#[derive(Default)]
struct Findings(Vec<Finding>);

impl Findings {
    fn add(&mut self, verdict: Verdict, check: &'static str, message: impl Into<String>) {
        self.0.push(Finding {
            verdict,
            check,
            message: message.into(),
        });
    }
    fn pass(&mut self, check: &'static str, message: impl Into<String>) {
        self.add(Verdict::Pass, check, message);
    }
    fn warn(&mut self, check: &'static str, message: impl Into<String>) {
        self.add(Verdict::Warn, check, message);
    }
    fn fail(&mut self, check: &'static str, message: impl Into<String>) {
        self.add(Verdict::Fail, check, message);
    }
}

/// Probes the app the manifest at `manifest_path` describes.
pub fn probe(manifest_path: &Path, options: &Options) -> Vec<Finding> {
    let mut out = Findings::default();
    let Some(manifest) = load_manifest(manifest_path, &mut out) else {
        return out.0;
    };
    let status = match &manifest.status {
        StatusSource::Builtin => {
            out.fail(
                "manifest",
                "a built-in has no status to probe; it lives inside the host",
            );
            None
        }
        StatusSource::Http { .. } => probe_http(&manifest, options, &mut out),
        StatusSource::File { .. } => probe_file(&manifest, options, &mut out),
    };
    if let Some(status) = status {
        probe_status(&manifest, &status, &mut out);
    }
    out.0
}

fn load_manifest(path: &Path, out: &mut Findings) -> Option<Manifest> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            out.fail("manifest", format!("cannot read {}: {e}", path.display()));
            return None;
        }
    };
    let value = match parse_json(&text) {
        Ok(value) => value,
        Err(e) => {
            out.fail("manifest", format!("not JSON: {e}"));
            return None;
        }
    };
    let file_name = path.file_name().and_then(|n| n.to_str());
    let report = validate_as(Kind::Manifest, &value, file_name);
    if !report.is_valid() {
        for issue in &report.issues {
            out.fail("manifest", issue.to_string());
        }
        return None;
    }
    out.pass("manifest", "valid");
    serde_json::from_value(value).ok()
}

/// Reads and validates a status document, reporting under `check`.
fn read_status(text: &str, check: &'static str, out: &mut Findings) -> Option<Status> {
    let value: Value = match parse_json(text) {
        Ok(value) => value,
        Err(e) => {
            out.fail(check, format!("not JSON: {e}"));
            return None;
        }
    };
    let report = validate_as(Kind::Status, &value, None);
    if !report.is_valid() {
        for issue in &report.issues {
            out.fail(check, issue.to_string());
        }
        return None;
    }
    serde_json::from_value(value).ok()
}

fn probe_http(manifest: &Manifest, options: &Options, out: &mut Findings) -> Option<Status> {
    let StatusSource::Http { url, .. } = &manifest.status else {
        unreachable!()
    };
    let port: u16 = url
        .strip_prefix("http://127.0.0.1:")?
        .split('/')
        .next()?
        .parse()
        .ok()?;
    let timeout = options
        .timeout
        .unwrap_or(Duration::from_millis(manifest.status.timeout_ms() as u64));
    // Generous for the security checks, which only need an answer.
    let slow = timeout.max(Duration::from_secs(5));

    let token_path = files::token_path(&options.run_dir, &manifest.id);
    let token = match fs::read_to_string(&token_path) {
        Ok(text) if files::is_token(&text) => {
            match files::readable_by_others(&token_path) {
                Ok(true) => out.fail(
                    "token-file",
                    format!("{} is readable by other users", token_path.display()),
                ),
                _ => out.pass("token-file", "present, 64 hex characters, user-only"),
            }
            text.trim_end_matches(['\r', '\n']).to_string()
        }
        Ok(_) => {
            out.fail(
                "token-file",
                format!(
                    "{} is not 64 lowercase hex characters",
                    token_path.display()
                ),
            );
            return None;
        }
        Err(e) => {
            out.fail(
                "token-file",
                format!("cannot read {}: {e}", token_path.display()),
            );
            return None;
        }
    };
    let bearer = format!("Bearer {token}");
    let status_path = "/n8/v2/status";

    // The control plane refuses what it must (spec 5.1 and 9).
    let expect = |out: &mut Findings,
                  check: &'static str,
                  request: Request,
                  want: u16,
                  what: &str| match request.send() {
        Ok(response) if response.status == want => out.pass(check, format!("{what}: {want}")),
        Ok(response) => out.fail(
            check,
            format!("{what}: expected {want}, got {}", response.status),
        ),
        Err(e) => out.fail(check, format!("{what}: {e}")),
    };
    expect(
        out,
        "auth",
        Request::new("GET", port, status_path, slow),
        401,
        "no token",
    );
    expect(
        out,
        "auth",
        Request::new("GET", port, status_path, slow)
            .header("Authorization", format!("Bearer {}", "0".repeat(64))),
        401,
        "wrong token",
    );
    expect(
        out,
        "origin",
        Request::new("GET", port, status_path, slow)
            .header("Authorization", bearer.clone())
            .header("Origin", "https://example.com"),
        403,
        "a request with an Origin header",
    );
    expect(
        out,
        "host",
        Request::new("GET", port, status_path, slow)
            .header("Authorization", bearer.clone())
            .header("Host", format!("evil.example:{port}")),
        403,
        "a foreign Host header",
    );
    expect(
        out,
        "unknown-action",
        Request::new("POST", port, "/n8/v2/actions/no-such-action", slow)
            .header("Authorization", bearer.clone())
            .json("{}"),
        404,
        "an unknown action",
    );

    let started = Instant::now();
    let response = match Request::new("GET", port, status_path, slow)
        .header("Authorization", bearer.clone())
        .send()
    {
        Ok(response) => response,
        Err(e) => {
            out.fail("status", format!("GET {status_path}: {e}"));
            return None;
        }
    };
    let elapsed = started.elapsed();
    if response.status != 200 {
        out.fail(
            "status",
            format!("GET {status_path}: expected 200, got {}", response.status),
        );
        return None;
    }
    if elapsed > timeout {
        out.warn(
            "status",
            format!(
                "answered in {} ms, over the {} ms timeout",
                elapsed.as_millis(),
                timeout.as_millis()
            ),
        );
    }
    if response.body.contains(&token) {
        out.fail("no-secrets", "the status document contains the token");
    } else {
        out.pass("no-secrets", "the token is not in the status document");
    }
    let status = read_status(&response.body, "status", out)?;
    out.pass(
        "status",
        format!("200 in {} ms, a valid status document", elapsed.as_millis()),
    );

    if manifest.has_feature(LifecycleFeature::Pressure) {
        if !status.features.contains(&Feature::Pressure) {
            out.fail(
                "pressure",
                "the manifest declares pressure but the status's features do not",
            );
        }
        match Request::new("POST", port, "/n8/v2/pressure", slow)
            .header("Authorization", bearer.clone())
            .json(r#"{"level":"ok"}"#)
            .send()
        {
            Ok(r) if r.status == 200 => {
                check_action_result(&r.body, "pressure", "POST /n8/v2/pressure", out)
            }
            Ok(r) => out.fail(
                "pressure",
                format!("POST /n8/v2/pressure: expected 200, got {}", r.status),
            ),
            Err(e) => out.fail("pressure", format!("POST /n8/v2/pressure: {e}")),
        }
    }

    if let Some((id, component)) = &options.action {
        match manifest.actions.iter().find(|a| &a.id == id) {
            None => out.fail(
                "action",
                format!("\"{id}\" is not declared in the manifest"),
            ),
            Some(action) if action.kind == ActionKind::Exec => out.warn(
                "action",
                format!("\"{id}\" is an exec action; the host runs those, the probe does not"),
            ),
            Some(_) => {
                let body = match component {
                    Some(c) => serde_json::json!({ "component": c }).to_string(),
                    None => "{}".to_string(),
                };
                let path = format!("/n8/v2/actions/{id}");
                match Request::new("POST", port, &path, slow)
                    .header("Authorization", bearer)
                    .json(&body)
                    .send()
                {
                    Ok(r) if r.status == 200 || r.status == 202 => {
                        check_action_result(&r.body, "action", &format!("POST {path}"), out)
                    }
                    Ok(r) => out.fail(
                        "action",
                        format!("POST {path}: expected 200 or 202, got {}", r.status),
                    ),
                    Err(e) => out.fail("action", format!("POST {path}: {e}")),
                }
            }
        }
    }
    Some(status)
}

fn check_action_result(body: &str, check: &'static str, what: &str, out: &mut Findings) {
    match parse_json(body) {
        Ok(value) => {
            let report = validate_as(Kind::ActionResult, &value, None);
            if report.is_valid() {
                out.pass(check, format!("{what}: {value}"));
            } else {
                for issue in report.issues {
                    out.fail(check, format!("{what}: {issue}"));
                }
            }
        }
        Err(e) => out.fail(check, format!("{what}: not JSON: {e}")),
    }
}

fn probe_file(manifest: &Manifest, options: &Options, out: &mut Findings) -> Option<Status> {
    let path = files::status_path(&options.run_dir, &manifest.id);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            out.fail(
                "status-file",
                format!("cannot read {}: {e}", path.display()),
            );
            return None;
        }
    };
    let status = read_status(&text, "status-file", out)?;
    let age = Utc::now() - status.updated_at;
    if age > chrono::Duration::seconds(30) {
        out.fail(
            "status-file",
            format!(
                "updatedAt is {} s old; a status file is valid for 30 s",
                age.num_seconds()
            ),
        );
        return None;
    }
    out.pass(
        "status-file",
        format!(
            "a valid status document, {} s old",
            age.num_seconds().max(0)
        ),
    );
    Some(status)
}

fn probe_status(manifest: &Manifest, status: &Status, out: &mut Findings) {
    if status.id == manifest.id {
        out.pass("identity", format!("id \"{}\"", status.id));
    } else {
        out.fail(
            "identity",
            format!(
                "the status id \"{}\" is not the manifest's \"{}\"",
                status.id, manifest.id
            ),
        );
    }
    if status.version != manifest.version {
        out.warn(
            "identity",
            format!(
                "the status says version {} and the manifest {}",
                status.version, manifest.version
            ),
        );
    }

    let resources_age = Utc::now() - status.resources.sampled_at;
    let limit = 3 * status.resources.interval_seconds as i64 + 5;
    if resources_age > chrono::Duration::seconds(limit) {
        out.fail(
            "resources",
            format!(
                "sampled {} s ago, more than {limit} s",
                resources_age.num_seconds()
            ),
        );
    } else {
        out.pass(
            "resources",
            format!(
                "{} process(es), sampled {} s ago",
                status.resources.processes.len(),
                resources_age.num_seconds().max(0)
            ),
        );
    }

    let mut table = Table::new();
    table.refresh();
    match table.get(status.pid) {
        Some(main) if same_start(status.pid_started_at, main.started_at) => out.pass(
            "main-process",
            format!(
                "pid {} is alive and started when the status says",
                status.pid
            ),
        ),
        Some(main) => out.fail(
            "main-process",
            format!(
                "pid {} started at {}, not {}: a reused pid",
                status.pid, main.started_at, status.pid_started_at
            ),
        ),
        None => out.fail("main-process", format!("pid {} is not running", status.pid)),
    }

    // Spec 7.2, steps 1 and 2: identity and tree for every reported process.
    let mut owned = 0;
    for process in &status.resources.processes {
        match table.get(process.pid) {
            None => out.fail("ownership", format!("pid {} is not running", process.pid)),
            Some(os) if !same_start(process.started_at, os.started_at) => out.fail(
                "ownership",
                format!(
                    "pid {} started at {}, not {}: a reused pid",
                    process.pid, os.started_at, process.started_at
                ),
            ),
            Some(_) if !table.descends_from(process.pid, status.pid) => out.fail(
                "ownership",
                format!(
                    "pid {} does not descend from the main process {}: the app does not own it",
                    process.pid, status.pid
                ),
            ),
            Some(_) => owned += 1,
        }
    }
    if owned == status.resources.processes.len() {
        out.pass(
            "ownership",
            format!(
                "every reported process ({owned}) is alive and descends from pid {}",
                status.pid
            ),
        );
    }

    // Step 5: nothing hidden.
    let reported: Vec<u32> = status.resources.processes.iter().map(|p| p.pid).collect();
    let unreported: Vec<u32> = table
        .descendants(status.pid)
        .into_iter()
        .filter(|pid| !reported.contains(pid))
        .collect();
    if unreported.is_empty() {
        out.pass("unreported", "no unreported child processes");
    } else {
        out.warn(
            "unreported",
            format!(
                "child process(es) not in resources: {unreported:?}; the host counts and flags them"
            ),
        );
    }

    if manifest.has_feature(LifecycleFeature::Presence) && status.presence.is_none() {
        out.fail(
            "presence",
            "the manifest declares presence but the status has no presence",
        );
    }
    if manifest.actions.iter().any(|a| a.per_component()) && status.components.is_none() {
        out.warn(
            "components",
            "the manifest has component actions but the status lists no components",
        );
    }
}

/// Whether the findings include a failure.
pub fn failed(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.verdict == Verdict::Fail)
}
