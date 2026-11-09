//! Reading an app's status and talking to its control plane (spec section 5):
//! the status file in the runtime folder, or loopback HTTP with the app's
//! token. Nothing here decides anything; it reports what it saw.

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use n8tray_contract::action::{ActionResult, PressureLevel};
use n8tray_contract::status::Status;

/// What reading an app's status found.
#[derive(Debug, Clone, PartialEq)]
pub enum Observation {
    /// A usable status document.
    Live(Box<Status>),
    /// No status: no file, or nothing listening.
    Absent,
    /// A status that does not count: stale, from a dead process, for another
    /// app, or not a status document. The text says why.
    Invalid(String),
    /// HTTP: the app refused the token (it is running, but not ours to read).
    Unauthorized,
}

/// A status file is valid while its `updatedAt` is under this old (spec 5.2).
pub const FILE_FRESH_SECONDS: i64 = 30;

/// `<run>/<id>.token`, trimmed; `None` when missing or not a token.
pub fn read_token(run_dir: &Path, id: &str) -> Option<String> {
    let text = std::fs::read_to_string(run_dir.join(format!("{id}.token"))).ok()?;
    let token = n8tray_contract::strip_bom(&text).trim().to_string();
    (token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())).then_some(token)
}

/// Checks a parsed status against the app it should belong to.
fn accept(status: Status, id: &str) -> Observation {
    if status.contract != n8tray_contract::CONTRACT {
        return Observation::Invalid(format!("status for contract {}", status.contract));
    }
    if status.id != id {
        return Observation::Invalid(format!("status for {} instead of {id}", status.id));
    }
    if !status.state.is_app_state() {
        return Observation::Invalid(format!("an app cannot report the state {:?}", status.state));
    }
    Observation::Live(Box::new(status))
}

fn parse(text: &str, id: &str) -> Observation {
    match n8tray_contract::parse_json(text)
        .ok()
        .and_then(|v| serde_json::from_value::<Status>(v).ok())
    {
        Some(status) => accept(status, id),
        // Half-written or not a status: no status (spec 5.2).
        None => Observation::Invalid("the status is not a status document".into()),
    }
}

/// Reads `<run>/<id>.status.json` (spec 5.2). `alive` checks that a pid runs
/// with a creation time within 1 s of the one given.
pub fn read_status_file(
    run_dir: &Path,
    id: &str,
    now: DateTime<Utc>,
    alive: impl Fn(u32, DateTime<Utc>) -> bool,
) -> Observation {
    let path = run_dir.join(format!("{id}.status.json"));
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => return Observation::Absent,
    };
    match parse(&text, id) {
        Observation::Live(status) => {
            if (now - status.updated_at).num_seconds() >= FILE_FRESH_SECONDS {
                Observation::Invalid("the status file is stale".into())
            } else if !alive(status.pid, status.pid_started_at) {
                Observation::Invalid("the status file's process is gone".into())
            } else {
                Observation::Live(status)
            }
        }
        other => other,
    }
}

/// A loopback HTTP client for the control plane: no proxy, short timeouts,
/// statuses as values rather than errors.
pub struct Http {
    agent: ureq::Agent,
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

/// An HTTP answer.
struct Answer {
    status: u16,
    body: String,
}

impl Http {
    pub fn new() -> Http {
        let agent = ureq::Agent::config_builder()
            .proxy(None)
            .http_status_as_error(false)
            .max_redirects(0)
            .build()
            .into();
        Http { agent }
    }

    fn get(&self, url: &str, token: &str, timeout: Duration) -> Result<Answer, String> {
        let response = self
            .agent
            .get(url)
            .config()
            .timeout_global(Some(timeout))
            .build()
            .header("Authorization", &format!("Bearer {token}"))
            .call()
            .map_err(|e| e.to_string())?;
        read_answer(response)
    }

    fn post(
        &self,
        url: &str,
        token: &str,
        body: &str,
        timeout: Duration,
    ) -> Result<Answer, String> {
        let response = self
            .agent
            .post(url)
            .config()
            .timeout_global(Some(timeout))
            .build()
            .header("Authorization", &format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .send(body)
            .map_err(|e| e.to_string())?;
        read_answer(response)
    }

    /// `GET /n8/v2/status` (spec 5.1).
    pub fn status(
        &self,
        url: &str,
        id: &str,
        token: Option<&str>,
        timeout: Duration,
    ) -> Observation {
        let Some(token) = token else {
            return Observation::Absent;
        };
        match self.get(url, token, timeout) {
            Err(_) => Observation::Absent,
            Ok(answer) if answer.status == 401 => Observation::Unauthorized,
            Ok(answer) if answer.status != 200 => {
                Observation::Invalid(format!("the status answered HTTP {}", answer.status))
            }
            Ok(answer) => parse(&answer.body, id),
        }
    }

    /// `POST /n8/v2/actions/<id>` (spec 5.1). `401` comes back as an error
    /// the caller retries once with a fresh token.
    pub fn action(
        &self,
        origin: &str,
        token: &str,
        action: &str,
        component: Option<&str>,
        timeout: Duration,
    ) -> Result<ActionResult, ActionError> {
        let body = match component {
            Some(c) => serde_json::json!({ "component": c }).to_string(),
            None => "{}".to_string(),
        };
        let url = format!("{origin}/n8/v2/actions/{action}");
        let answer = self
            .post(&url, token, &body, timeout)
            .map_err(ActionError::Unreachable)?;
        match answer.status {
            200 | 202 => serde_json::from_str(&answer.body)
                .map_err(|_| ActionError::Protocol("the answer is not an action result".into())),
            401 => Err(ActionError::Unauthorized),
            code => Err(ActionError::Protocol(format!("HTTP {code}"))),
        }
    }

    /// `POST /n8/v2/pressure` (spec 5.4).
    pub fn pressure(
        &self,
        origin: &str,
        token: &str,
        level: PressureLevel,
        timeout: Duration,
    ) -> Result<ActionResult, ActionError> {
        let body = serde_json::json!({ "level": level }).to_string();
        let answer = self
            .post(&format!("{origin}/n8/v2/pressure"), token, &body, timeout)
            .map_err(ActionError::Unreachable)?;
        match answer.status {
            200 => serde_json::from_str(&answer.body)
                .map_err(|_| ActionError::Protocol("the answer is not an action result".into())),
            401 => Err(ActionError::Unauthorized),
            code => Err(ActionError::Protocol(format!("HTTP {code}"))),
        }
    }

    /// A plain HTTPS GET for the update check (no token).
    pub fn fetch(&self, url: &str, timeout: Duration) -> Result<(u16, String), String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(timeout))
            .build()
            .into();
        let response = agent
            .get(url)
            .header("User-Agent", concat!("n8tray/", env!("CARGO_PKG_VERSION")))
            .call()
            .map_err(|e| e.to_string())?;
        let answer = read_answer(response)?;
        Ok((answer.status, answer.body))
    }
}

fn read_answer(mut response: ureq::http::Response<ureq::Body>) -> Result<Answer, String> {
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    Ok(Answer { status, body })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionError {
    Unreachable(String),
    Unauthorized,
    Protocol(String),
}

impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(e) => write!(f, "the app did not answer ({e})"),
            Self::Unauthorized => f.write_str("the app refused N8 Tray's token"),
            Self::Protocol(e) => write!(f, "the app answered wrongly ({e})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status_text(id: &str, updated: DateTime<Utc>) -> String {
        let t = crate::util::timestamp(updated);
        format!(
            r#"{{ "contract": 2, "id": "{id}", "version": "1.0.0", "updatedAt": "{t}",
                 "pid": 42, "pidStartedAt": "{t}", "state": "ready", "summary": "Ready",
                 "resources": {{ "sampledAt": "{t}", "intervalSeconds": 5, "processes": [] }} }}"#
        )
    }

    #[test]
    fn status_files() {
        let dir = tempfile::tempdir().unwrap();
        let now = Utc::now();
        let alive = |_, _| true;
        assert_eq!(
            read_status_file(dir.path(), "a", now, alive),
            Observation::Absent
        );

        let path = dir.path().join("a.status.json");
        std::fs::write(&path, format!("\u{feff}{}", status_text("a", now))).unwrap();
        assert!(matches!(
            read_status_file(dir.path(), "a", now, alive),
            Observation::Live(_)
        ));
        assert!(matches!(
            read_status_file(dir.path(), "a", now, |_, _| false),
            Observation::Invalid(_)
        ));
        let later = now + chrono::Duration::seconds(31);
        assert!(matches!(
            read_status_file(dir.path(), "a", later, alive),
            Observation::Invalid(_)
        ));

        std::fs::write(&path, status_text("b", now)).unwrap();
        assert!(matches!(
            read_status_file(dir.path(), "a", now, alive),
            Observation::Invalid(_)
        ));
        std::fs::write(&path, &status_text("a", now)[..40]).unwrap();
        assert!(matches!(
            read_status_file(dir.path(), "a", now, alive),
            Observation::Invalid(_)
        ));
        std::fs::write(
            &path,
            status_text("a", now).replace("\"ready\"", "\"stopped\""),
        )
        .unwrap();
        assert!(matches!(
            read_status_file(dir.path(), "a", now, alive),
            Observation::Invalid(_)
        ));
    }

    #[test]
    fn tokens() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_token(dir.path(), "a"), None);
        let token = "ab".repeat(32);
        std::fs::write(dir.path().join("a.token"), format!("{token}\n")).unwrap();
        assert_eq!(read_token(dir.path(), "a"), Some(token));
        std::fs::write(dir.path().join("a.token"), "short").unwrap();
        assert_eq!(read_token(dir.path(), "a"), None);
    }

    #[test]
    fn nothing_listening_is_absent() {
        let http = Http::new();
        let observation = http.status(
            "http://127.0.0.1:9/n8/v2/status",
            "a",
            Some(&"ab".repeat(32)),
            Duration::from_millis(300),
        );
        assert_eq!(observation, Observation::Absent);
    }
}
