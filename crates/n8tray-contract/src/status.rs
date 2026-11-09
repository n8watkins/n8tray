//! The status document an app serves or writes (spec section 4) and the
//! shared status vocabulary (spec section 6).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// An app's status, contract 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub contract: u32,
    pub id: String,
    pub version: String,
    pub updated_at: DateTime<Utc>,
    pub pid: u32,
    pub pid_started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<Feature>,
    pub state: State,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<Component>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence: Option<Presence>,
    pub resources: Resources,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jobs: Option<Vec<Job>>,
}

/// Optional protocol features (spec 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Feature {
    Events,
    Progress,
    Pressure,
    Presence,
}

/// One state for apps and components (spec section 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    Ready,
    Idle,
    Starting,
    Degraded,
    Busy,
    Stopped,
    Failed,
    NotInstalled,
    NeedsSetup,
    Unsupported,
}

/// The host's levels; the tray shows the worst one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// Not shown at all (`unsupported`).
    Hidden,
    Off,
    Ok,
    Attention,
    Problem,
}

impl State {
    /// Whether an app may report this state for itself; the others are what
    /// the host concludes when there is no live status (spec section 6).
    pub fn is_app_state(self) -> bool {
        matches!(
            self,
            Self::Ready | Self::Idle | Self::Starting | Self::Degraded | Self::Busy | Self::Failed
        )
    }

    /// The host level for this state; `should_run` turns `stopped` into a problem.
    pub fn level(self, should_run: bool) -> Level {
        match self {
            Self::Ready | Self::Idle => Level::Ok,
            Self::Starting | Self::Degraded | Self::Busy => Level::Attention,
            Self::Stopped if should_run => Level::Problem,
            Self::Stopped | Self::NotInstalled | Self::NeedsSetup => Level::Off,
            Self::Failed => Level::Problem,
            Self::Unsupported => Level::Hidden,
        }
    }

    /// The words users see (spec section 6).
    pub fn words(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Idle => "Sleeping, starts when needed",
            Self::Starting => "Starting",
            Self::Degraded => "Ready, with a problem",
            Self::Busy => "Busy",
            Self::Stopped => "Installed but not running",
            Self::Failed => "Stopped",
            Self::NotInstalled => "Not set up",
            Self::NeedsSetup => "Needs a dependency, see Checks",
            Self::Unsupported => "Not available on this computer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    pub id: String,
    pub name: String,
    pub state: State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Client presence, for apps with the `presence` feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    pub last_seen_at: Option<DateTime<Utc>>,
    pub clients: u32,
    /// App-defined fields, such as VoiceDock's `activeEngine`.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Option<String>>,
}

/// The resources of every owned process (spec 4.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resources {
    pub sampled_at: DateTime<Utc>,
    pub interval_seconds: u32,
    pub processes: Vec<Process>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespaces: Option<Vec<NamespacedProcess>>,
}

impl Resources {
    /// Memory of every process the host can see, in MiB.
    pub fn total_rss_mb(&self) -> f64 {
        self.processes.iter().map(|p| p.rss_mb).sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Main,
    Hub,
    Engine,
    Worker,
    Helper,
    Ui,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Process {
    pub pid: u32,
    pub started_at: DateTime<Utc>,
    pub parent_pid: u32,
    pub role: Role,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub component: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
    pub rss_mb: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_mb: Option<f64>,
    /// `None` until the app has a second sample.
    pub cpu_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu: Option<Gpu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<Model>,
}

impl Process {
    /// The component the process belongs to, if any.
    pub fn component(&self) -> Option<&str> {
        self.component.as_ref().and_then(|c| c.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mem_mb: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub loaded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mb: Option<f64>,
}

/// A process the host cannot see, such as a daemon inside WSL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespacedProcess {
    pub namespace: String,
    pub pid: u32,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub component: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
    pub rss_mb: f64,
    pub cpu_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    pub state: JobState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Running,
    Done,
    Failed,
}

/// Tells "absent" from `null` for optional nullable fields, so a document
/// reads and writes back the same.
mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer, T: Serialize>(
        value: &Option<Option<T>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(inner) => inner.serialize(s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        d: D,
    ) -> Result<Option<Option<T>>, D::Error> {
        Option::<T>::deserialize(d).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_the_vocabulary() {
        assert_eq!(State::Ready.level(true), Level::Ok);
        assert_eq!(State::Idle.level(true), Level::Ok);
        assert_eq!(State::Starting.level(true), Level::Attention);
        assert_eq!(State::Stopped.level(false), Level::Off);
        assert_eq!(State::Stopped.level(true), Level::Problem);
        assert_eq!(State::Failed.level(false), Level::Problem);
        assert_eq!(State::Unsupported.level(true), Level::Hidden);
        assert!(
            Level::Problem > Level::Attention
                && Level::Attention > Level::Ok
                && Level::Ok > Level::Off
        );
    }

    #[test]
    fn only_six_states_are_app_states() {
        let all = [
            State::Ready,
            State::Idle,
            State::Starting,
            State::Degraded,
            State::Busy,
            State::Stopped,
            State::Failed,
            State::NotInstalled,
            State::NeedsSetup,
            State::Unsupported,
        ];
        let app: Vec<_> = all.iter().filter(|s| s.is_app_state()).collect();
        assert_eq!(app.len(), 6);
        assert!(!State::Stopped.is_app_state());
    }
}
