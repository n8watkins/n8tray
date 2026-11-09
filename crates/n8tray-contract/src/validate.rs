//! Validation: the JSON Schema first, then the rules a schema cannot express
//! (spec section 11), each reported with a stable code.

use std::collections::HashSet;
use std::fmt;

use chrono::Duration;
use jsonschema::Validator;
use serde::Serialize;
use serde_json::Value;

use crate::lease::Lease;
use crate::manifest::{ActionKind, LifecycleFeature, Manifest, MenuItem, StatusSource};
use crate::paths;
use crate::schema;
use crate::status::Status;

/// The kinds of document the contract defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A contract 2 manifest.
    Manifest,
    /// A contract 1 `modular.json`, accepted during the migration.
    ModularV1,
    Status,
    Lease,
    ActionResult,
}

impl Kind {
    /// Every kind, in the order the CLI lists them.
    pub const ALL: [Kind; 5] = [
        Kind::Manifest,
        Kind::ModularV1,
        Kind::Status,
        Kind::Lease,
        Kind::ActionResult,
    ];

    /// The kind's name, as the CLI and the fixture folders spell it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Manifest => "manifest",
            Kind::ModularV1 => "modular-v1",
            Kind::Status => "status",
            Kind::Lease => "lease",
            Kind::ActionResult => "action-result",
        }
    }

    /// The kind with this name.
    pub fn from_name(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }

    /// The kind's JSON Schema text.
    pub fn schema(self) -> &'static str {
        match self {
            Kind::Manifest => schema::MANIFEST,
            Kind::ModularV1 => schema::MODULAR_V1,
            Kind::Status => schema::STATUS,
            Kind::Lease => schema::LEASE,
            Kind::ActionResult => schema::ACTION_RESULT,
        }
    }

    fn validator(self) -> &'static Validator {
        match self {
            Kind::Manifest => &schema::MANIFEST_VALIDATOR,
            Kind::ModularV1 => &schema::MODULAR_V1_VALIDATOR,
            Kind::Status => &schema::STATUS_VALIDATOR,
            Kind::Lease => &schema::LEASE_VALIDATOR,
            Kind::ActionResult => &schema::ACTION_RESULT_VALIDATOR,
        }
    }

    /// Guesses a document's kind from its fields and file name.
    pub fn detect(value: &Value, file_name: Option<&str>) -> Kind {
        let has = |key: &str| value.get(key).is_some();
        if has("schemaVersion") || file_name == Some("modular.json") {
            Kind::ModularV1
        } else if has("heartbeat") || has("hosting") || file_name == Some("host.json") {
            Kind::Lease
        } else if has("outcome") || has("jobId") {
            Kind::ActionResult
        } else if has("updatedAt")
            || has("summary")
            || file_name.is_some_and(|n| n.ends_with(".status.json"))
        {
            Kind::Status
        } else {
            Kind::Manifest
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// One problem: a code from spec section 11 (or `schema`), where, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub code: &'static str,
    /// A JSON Pointer into the document; empty for the whole document.
    pub path: String,
    pub message: String,
}

impl Issue {
    fn new(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Issue {
            code,
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = if self.path.is_empty() {
            "/"
        } else {
            &self.path
        };
        write!(f, "[{}] {}: {}", self.code, path, self.message)
    }
}

/// The result of validating one document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub kind: Kind,
    pub issues: Vec<Issue>,
    /// Notes that are not problems, such as "contract 1, accepted during the migration".
    pub notes: Vec<String>,
}

impl Report {
    /// Whether the document is valid.
    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }

    /// The distinct codes reported.
    pub fn codes(&self) -> Vec<&'static str> {
        let mut codes: Vec<_> = self.issues.iter().map(|i| i.code).collect();
        codes.dedup();
        codes
    }
}

/// Validates a document of the kind [`Kind::detect`] finds. `file_name` (the
/// base name, such as `voicedock.json`) enables the `id-file-name` rule.
pub fn validate(value: &Value, file_name: Option<&str>) -> Report {
    validate_as(Kind::detect(value, file_name), value, file_name)
}

/// Validates a document as `kind`.
pub fn validate_as(kind: Kind, value: &Value, file_name: Option<&str>) -> Report {
    let mut issues: Vec<Issue> = kind
        .validator()
        .iter_errors(value)
        .map(|e| Issue::new("schema", e.instance_path().to_string(), e.to_string()))
        .collect();
    let mut notes = Vec::new();
    // The rules below assume the shape the schema guarantees.
    if issues.is_empty() {
        match kind {
            Kind::Manifest => match serde_json::from_value::<Manifest>(value.clone()) {
                Ok(manifest) => manifest_rules(&manifest, file_name, &mut issues),
                Err(e) => issues.push(Issue::new(
                    "schema",
                    "",
                    format!("does not read as a manifest: {e}"),
                )),
            },
            Kind::Status => match serde_json::from_value::<Status>(value.clone()) {
                Ok(status) => status_rules(&status, &mut issues),
                Err(e) => issues.push(Issue::new(
                    "schema",
                    "",
                    format!("does not read as a status document: {e}"),
                )),
            },
            Kind::Lease => match serde_json::from_value::<Lease>(value.clone()) {
                Ok(lease) => lease_rules(&lease, &mut issues),
                Err(e) => issues.push(Issue::new(
                    "schema",
                    "",
                    format!("does not read as a lease: {e}"),
                )),
            },
            Kind::ModularV1 => {
                notes.push("contract 1 (modular.json): accepted during the migration only, see spec section 10".into());
            }
            Kind::ActionResult => {}
        }
    }
    Report {
        kind,
        issues,
        notes,
    }
}

fn manifest_rules(m: &Manifest, file_name: Option<&str>, issues: &mut Vec<Issue>) {
    if let Some(name) = file_name
        && name != "manifest.json"
        && name != "modular.json"
    {
        let stem = name.strip_suffix(".json").unwrap_or(name);
        if stem != m.id {
            issues.push(Issue::new(
                "id-file-name",
                "/id",
                format!(
                    "the file is named {name} but the id is \"{}\"; name it {}.json",
                    m.id, m.id
                ),
            ));
        }
    }

    let builtin = matches!(m.status, StatusSource::Builtin);

    // installDir: absolute after one known leading placeholder.
    if let Some(dir) = &m.install_dir {
        match paths::split_placeholder(dir) {
            Err(e) => issues.push(Issue::new("placeholder", "/installDir", e.to_string())),
            Ok((placeholder, rest)) => {
                if placeholder.is_none() && !paths::is_absolute_anywhere(dir) {
                    issues.push(Issue::new(
                        "install-dir",
                        "/installDir",
                        format!("\"{dir}\" is not an absolute path and does not start with a placeholder"),
                    ));
                }
                if placeholder.is_some() && rest.split(['/', '\\']).any(|s| s == "..") {
                    issues.push(Issue::new(
                        "path-escapes",
                        "/installDir",
                        "has a \"..\" segment",
                    ));
                }
            }
        }
    }

    // Every relative path.
    let mut relative = |pointer: String, path: &str| {
        if let Err(e) = paths::check_relative(path) {
            issues.push(Issue::new(
                "path-escapes",
                pointer,
                format!("\"{path}\" {e}; it must stay inside installDir"),
            ));
        }
    };
    if let Some(icon) = &m.icon {
        relative("/icon".into(), icon);
    }
    if let Some(run) = &m.run {
        for (key, value) in [
            ("program", &run.program),
            ("script", &run.script),
            ("cwd", &run.cwd),
        ] {
            if let Some(path) = value {
                relative(format!("/run/{key}"), path);
            }
        }
    }
    for (index, item) in m.menu.iter().enumerate() {
        if let MenuItem::OpenPath { path, .. } = item {
            relative(format!("/menu/{index}/path"), path);
        }
    }
    let mut open_urls = Vec::new();
    for (target, value) in &m.open {
        let key = serde_json::to_value(target)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        if value.contains("://") {
            open_urls.push((format!("/open/{key}"), value.clone()));
        } else {
            relative(format!("/open/{key}"), value);
        }
    }
    for (index, owned) in m.owns.iter().enumerate() {
        match paths::split_placeholder(&owned.program) {
            Err(e) => issues.push(Issue::new("placeholder", format!("/owns/{index}/program"), e.to_string())),
            Ok((Some("%N8TRAY_TOOLS%"), rest)) => {
                if rest.split(['/', '\\']).any(|s| s == "..") {
                    issues.push(Issue::new("path-escapes", format!("/owns/{index}/program"), "has a \"..\" segment"));
                }
            }
            Ok((Some(other), _)) => issues.push(Issue::new(
                "placeholder",
                format!("/owns/{index}/program"),
                format!("{other} is not allowed here; owns globs are inside installDir or start with %N8TRAY_TOOLS%"),
            )),
            Ok((None, path)) => {
                if let Err(e) = paths::check_relative(path) {
                    issues.push(Issue::new(
                        "path-escapes",
                        format!("/owns/{index}/program"),
                        format!("\"{path}\" {e}; it must stay inside installDir"),
                    ));
                }
            }
        }
    }

    // Loopback URLs only, in `open`.
    for (pointer, url) in open_urls {
        if !is_loopback_url(&url) {
            issues.push(Issue::new(
                "menu-url",
                pointer,
                format!("\"{url}\" is not a loopback URL (http://127.0.0.1:<port>/...)"),
            ));
        }
    }
    for (index, item) in m.menu.iter().enumerate() {
        if let MenuItem::OpenUrl { url, .. } = item
            && url.starts_with("http://")
            && !is_loopback_url(url)
        {
            issues.push(Issue::new(
                "menu-url",
                format!("/menu/{index}/url"),
                format!("\"{url}\" is neither loopback HTTP nor HTTPS"),
            ));
        }
    }

    // The status URL's port.
    if let StatusSource::Http { url, .. } = &m.status {
        let port = url
            .strip_prefix("http://127.0.0.1:")
            .and_then(|rest| rest.split('/').next())
            .and_then(|port| port.parse::<u32>().ok());
        if !matches!(port, Some(1..=65535)) {
            issues.push(Issue::new(
                "status-url",
                "/status/url",
                "the port must be from 1 to 65535",
            ));
        }
    }

    // Actions.
    let mut seen = HashSet::new();
    for (index, action) in m.actions.iter().enumerate() {
        let pointer = format!("/actions/{index}");
        if action.id == "start" || action.id == "stop" {
            issues.push(Issue::new(
                "action-id",
                format!("{pointer}/id"),
                format!(
                    "\"{}\" is reserved for run; the host starts and stops apps itself",
                    action.id
                ),
            ));
        }
        if !seen.insert(action.id.as_str()) {
            issues.push(Issue::new(
                "action-id",
                format!("{pointer}/id"),
                format!("\"{}\" is declared twice", action.id),
            ));
        }
        match action.kind {
            ActionKind::Exec if builtin => issues.push(Issue::new(
                "builtin-run",
                format!("{pointer}/type"),
                "a built-in has no program to run, so it has no exec actions",
            )),
            ActionKind::Exec if m.run.is_none() => issues.push(Issue::new(
                "action-exec",
                format!("{pointer}/type"),
                "an exec action runs the app's program or script, so the manifest needs run",
            )),
            ActionKind::Http if action.args.is_some() => issues.push(Issue::new(
                "action-exec",
                format!("{pointer}/args"),
                "args belong to exec actions only",
            )),
            _ => {}
        }
        if action.kind == ActionKind::Http && !matches!(m.status, StatusSource::Http { .. }) {
            issues.push(Issue::new(
                "action-exec",
                format!("{pointer}/type"),
                "an http action needs status.type http, where the actions endpoint lives",
            ));
        }
        let per_component = action.per_component();
        if action.label.contains("{component}") && !per_component {
            issues.push(Issue::new(
                "component-placeholder",
                format!("{pointer}/label"),
                "{component} needs \"component\": true",
            ));
        }
        for (arg_index, arg) in action.args.iter().flatten().enumerate() {
            if arg.contains("{component}") && (!per_component || arg != "{component}") {
                issues.push(Issue::new(
                    "component-placeholder",
                    format!("{pointer}/args/{arg_index}"),
                    if per_component {
                        "{component} must be a whole argument, not part of one"
                    } else {
                        "{component} needs \"component\": true"
                    },
                ));
            }
        }
    }
    if let Some(run) = &m.run {
        for (arg_index, arg) in run
            .args
            .iter()
            .flatten()
            .chain(run.stop.iter().flat_map(|s| s.args.iter().flatten()))
            .enumerate()
        {
            if arg.contains("{component}") {
                issues.push(Issue::new(
                    "component-placeholder",
                    "/run",
                    format!(
                        "argument {arg_index} uses {{component}}, which only component actions may"
                    ),
                ));
            }
        }
        if let Some(stop) = &run.stop
            && stop.kind == crate::manifest::StopKind::Http
            && !matches!(m.status, StatusSource::Http { .. })
        {
            issues.push(Issue::new(
                "action-exec",
                "/run/stop/type",
                "stop type http needs status.type http",
            ));
        }
    }

    for (index, item) in m.menu.iter().enumerate() {
        if let MenuItem::Action { action, .. } = item
            && !m.actions.iter().any(|a| &a.id == action)
        {
            issues.push(Issue::new(
                "menu-action",
                format!("/menu/{index}/action"),
                format!("\"{action}\" is not declared in actions"),
            ));
        }
    }

    // Budget order.
    if let Some(resources) = &m.resources {
        let budget = resources.budget.as_ref();
        let warn = budget.and_then(|b| b.warn_mb);
        let max = budget.and_then(|b| b.max_mb);
        if let (Some(warn), Some(max)) = (warn, max)
            && warn >= max
        {
            issues.push(Issue::new(
                "budget",
                "/resources/budget/warnMb",
                format!("warnMb ({warn}) must be below maxMb ({max})"),
            ));
        }
        if let (Some(max), Some(cap)) = (max, resources.hard_cap_mb)
            && max > cap
        {
            issues.push(Issue::new(
                "budget",
                "/resources/budget/maxMb",
                format!("maxMb ({max}) must not be above hardCapMb ({cap})"),
            ));
        }
    }

    if m.has_feature(LifecycleFeature::Pressure) && !matches!(m.status, StatusSource::Http { .. }) {
        issues.push(Issue::new(
            "feature-transport",
            "/lifecycle/features",
            "pressure notices arrive over HTTP, so pressure needs status.type http",
        ));
    }
}

fn is_loopback_url(url: &str) -> bool {
    url.strip_prefix("http://127.0.0.1:").is_some_and(|rest| {
        let port = rest.split('/').next().unwrap_or("");
        rest.contains('/') && matches!(port.parse::<u32>(), Ok(1..=65535))
    })
}

fn status_rules(s: &Status, issues: &mut Vec<Issue>) {
    let tolerance = Duration::seconds(1);
    let processes = &s.resources.processes;

    match processes.iter().position(|p| p.pid == s.pid) {
        None => issues.push(Issue::new(
            "main-process",
            "/resources/processes",
            format!("the main process (pid {}) is not listed", s.pid),
        )),
        Some(index) => {
            let started = processes[index].started_at;
            if (started - s.pid_started_at).abs() > tolerance {
                issues.push(Issue::new(
                    "start-order",
                    "/pidStartedAt",
                    format!("pidStartedAt differs from the main process's startedAt ({started}) by more than 1 s"),
                ));
            }
        }
    }

    let mut pids = HashSet::new();
    for (index, process) in processes.iter().enumerate() {
        let pointer = format!("/resources/processes/{index}");
        if !pids.insert(process.pid) {
            issues.push(Issue::new(
                "duplicate-pid",
                format!("{pointer}/pid"),
                format!("pid {} is listed twice", process.pid),
            ));
        }
        if process.parent_pid == process.pid {
            issues.push(Issue::new(
                "self-parent",
                format!("{pointer}/parentPid"),
                "a process cannot be its own parent",
            ));
        }
        if process.started_at > s.resources.sampled_at + tolerance {
            issues.push(Issue::new(
                "start-order",
                format!("{pointer}/startedAt"),
                "the process started after resources.sampledAt",
            ));
        }
    }
    for (index, process) in s.resources.namespaces.iter().flatten().enumerate() {
        if process.started_at > s.resources.sampled_at + tolerance {
            issues.push(Issue::new(
                "start-order",
                format!("/resources/namespaces/{index}/startedAt"),
                "the process started after resources.sampledAt",
            ));
        }
    }

    if let Some(components) = &s.components {
        let mut ids = HashSet::new();
        for (index, component) in components.iter().enumerate() {
            if !ids.insert(component.id.as_str()) {
                issues.push(Issue::new(
                    "duplicate-id",
                    format!("/components/{index}/id"),
                    format!("component \"{}\" is listed twice", component.id),
                ));
            }
        }
        let known = |id: &str| components.iter().any(|c| c.id == id);
        for (index, process) in processes.iter().enumerate() {
            if let Some(component) = process.component()
                && !known(component)
            {
                issues.push(Issue::new(
                    "unknown-component",
                    format!("/resources/processes/{index}/component"),
                    format!("\"{component}\" is not in components"),
                ));
            }
        }
        for (index, job) in s.jobs.iter().flatten().enumerate() {
            if let Some(component) = &job.component
                && !known(component)
            {
                issues.push(Issue::new(
                    "unknown-component",
                    format!("/jobs/{index}/component"),
                    format!("\"{component}\" is not in components"),
                ));
            }
        }
    }
    let mut job_ids = HashSet::new();
    for (index, job) in s.jobs.iter().flatten().enumerate() {
        if !job_ids.insert(job.id.as_str()) {
            issues.push(Issue::new(
                "duplicate-id",
                format!("/jobs/{index}/id"),
                format!("job \"{}\" is listed twice", job.id),
            ));
        }
    }
}

fn lease_rules(lease: &Lease, issues: &mut Vec<Issue>) {
    if lease.heartbeat + Duration::seconds(1) < lease.pid_started_at {
        issues.push(Issue::new(
            "lease-start-order",
            "/heartbeat",
            "the heartbeat is earlier than the host process's start",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_kinds() {
        assert_eq!(
            Kind::detect(&json!({"schemaVersion": 1}), None),
            Kind::ModularV1
        );
        assert_eq!(Kind::detect(&json!({"heartbeat": "x"}), None), Kind::Lease);
        assert_eq!(Kind::detect(&json!({}), Some("host.json")), Kind::Lease);
        assert_eq!(
            Kind::detect(&json!({"outcome": "ok"}), None),
            Kind::ActionResult
        );
        assert_eq!(Kind::detect(&json!({"updatedAt": "x"}), None), Kind::Status);
        assert_eq!(
            Kind::detect(&json!({"resources": {}, "installDir": "/x"}), None),
            Kind::Manifest
        );
        assert_eq!(
            Kind::detect(&json!({}), Some("x.status.json")),
            Kind::Status
        );
        assert_eq!(
            Kind::detect(&json!({"contract": 2}), Some("x.json")),
            Kind::Manifest
        );
    }

    #[test]
    fn id_must_match_the_file_name() {
        let manifest = json!({"contract": 2, "id": "fake", "name": "Fake", "version": "1.0.0",
            "installDir": "/opt/fake", "status": {"type": "file"}});
        assert!(validate(&manifest, Some("fake.json")).is_valid());
        assert!(validate(&manifest, Some("manifest.json")).is_valid());
        assert_eq!(
            validate(&manifest, Some("other.json")).codes(),
            vec!["id-file-name"]
        );
    }

    #[test]
    fn loopback_urls() {
        assert!(is_loopback_url("http://127.0.0.1:7477/settings"));
        assert!(!is_loopback_url("http://127.0.0.1:7477"));
        assert!(!is_loopback_url("http://127.0.0.1:0/x"));
        assert!(!is_loopback_url("http://localhost:7477/x"));
        assert!(!is_loopback_url("https://127.0.0.1:7477/x"));
    }
}
