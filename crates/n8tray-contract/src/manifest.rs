//! The manifest an app installs (spec section 3).
//!
//! These types read a manifest the way a host does at run time: unknown
//! fields are ignored. Use [`crate::validate`] to check one strictly.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// An app manifest, contract 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub contract: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<Platform>>,
    pub status: StatusSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<Run>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<Requirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<Lifecycle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<Resources>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub open: BTreeMap<OpenTarget, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menu: Vec<MenuItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owns: Vec<Owned>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tray_icon: Option<TrayIcon>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start_default: Option<bool>,
}

impl Manifest {
    /// The platforms the app runs on; all three when the manifest says nothing.
    pub fn platforms(&self) -> Vec<Platform> {
        self.platforms
            .clone()
            .unwrap_or_else(|| vec![Platform::Windows, Platform::Macos, Platform::Linux])
    }

    /// Whether the app hides its own icon while hosted.
    pub fn hides_when_hosted(&self) -> bool {
        self.tray_icon == Some(TrayIcon::HideWhenHosted)
    }

    /// Whether the app lists `feature` in `lifecycle.features`.
    pub fn has_feature(&self, feature: LifecycleFeature) -> bool {
        self.lifecycle
            .as_ref()
            .is_some_and(|l| l.features.contains(&feature))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
}

impl Platform {
    /// The platform this program runs on.
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Linux
        }
    }
}

/// How the host reads the app's status (spec 3.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StatusSource {
    #[serde(rename_all = "camelCase")]
    Http {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        interval_seconds: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u32>,
    },
    #[serde(rename_all = "camelCase")]
    File {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        interval_seconds: Option<u32>,
    },
    Builtin,
}

impl StatusSource {
    /// Seconds between status reads; 3 by default.
    pub fn interval_seconds(&self) -> u32 {
        match self {
            Self::Http {
                interval_seconds, ..
            }
            | Self::File { interval_seconds } => interval_seconds.unwrap_or(3),
            Self::Builtin => 3,
        }
    }

    /// The HTTP timeout; 300 ms by default.
    pub fn timeout_ms(&self) -> u32 {
        match self {
            Self::Http { timeout_ms, .. } => timeout_ms.unwrap_or(300),
            _ => 300,
        }
    }

    /// The origin (`http://127.0.0.1:<port>`) of an HTTP status URL.
    pub fn origin(&self) -> Option<&str> {
        match self {
            Self::Http { url, .. } => url.strip_suffix("/n8/v2/status"),
            _ => None,
        }
    }
}

/// How the host starts and stops the app (spec 3.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpreter: Option<Interpreter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<Stop>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Interpreter {
    Powershell,
    Pwsh,
    Node,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stop {
    #[serde(rename = "type")]
    pub kind: StopKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grace_seconds: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StopKind {
    Http,
    Exec,
    Terminate,
}

/// An operation the host may invoke (spec 3.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: ActionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm: Option<bool>,
}

impl Action {
    /// Whether the action applies to one component at a time.
    pub fn per_component(&self) -> bool {
        self.component == Some(true)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionKind {
    Http,
    Exec,
}

/// A dependency, by the id of one of the host's checks (spec 3.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    pub check: Check,
    #[serde(default, rename = "for", skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<String>>,
    pub why: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Check {
    Browser,
    Uv,
    Python,
    EngineEnv,
    Node,
    Wsl,
    Obs,
    Devguard,
    Libfuse2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lifecycle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub demand: Option<Demand>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<LifecycleFeature>,
}

/// The app is only needed while one of these processes runs (spec 3.7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Demand {
    pub processes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_after_minutes: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleFeature {
    Pressure,
    Presence,
}

/// Alert thresholds and the hard cap (spec 3.8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<Budget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hard_cap_mb: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warn_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_warn_percent: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sustain_seconds: Option<u32>,
}

impl Budget {
    /// The defaults for an app without a budget (spec 3.8).
    pub const DEFAULT_WARN_MB: u64 = 500;
    pub const DEFAULT_MAX_MB: u64 = 1500;
    pub const DEFAULT_CPU_WARN_PERCENT: u32 = 50;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenTarget {
    Settings,
    Logs,
    Folder,
    Readme,
}

/// An extra menu line (spec 3.9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum MenuItem {
    OpenUrl { label: String, url: String },
    OpenPath { label: String, path: String },
    Action { label: String, action: String },
}

/// A program glob that belongs to the app (spec 3.10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Owned {
    pub program: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_contain: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrayIcon {
    HideWhenHosted,
    None,
}
