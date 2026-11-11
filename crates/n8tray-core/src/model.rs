//! What the host publishes: one [`HostSnapshot`], rebuilt on every tick and
//! sent to the tray and the window only when it changed. The window is a view
//! of it; it never computes state of its own. The TypeScript types in
//! `app/ui/src/bindings/` are generated from these (`cargo test` writes them,
//! CI fails when they are out of date).

use serde::Serialize;
use ts_rs::TS;

/// The host's levels, worst last (spec section 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum LevelName {
    Hidden,
    Off,
    Ok,
    Attention,
    Problem,
}

impl From<n8tray_contract::status::Level> for LevelName {
    fn from(level: n8tray_contract::status::Level) -> Self {
        use n8tray_contract::status::Level;
        match level {
            Level::Hidden => Self::Hidden,
            Level::Off => Self::Off,
            Level::Ok => Self::Ok,
            Level::Attention => Self::Attention,
            Level::Problem => Self::Problem,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HostSnapshot {
    pub version: String,
    /// The worst level of every app and of system memory.
    pub level: LevelName,
    /// "Healthy", "Needs attention" or "Problem", or why nothing runs.
    pub headline: String,
    /// The first reason for the level, such as "VoiceDock: Stopped: crashed".
    pub reason: Option<String>,
    pub running: u32,
    pub total: u32,
    pub memory: MemoryView,
    /// The memory pressure level pushed to apps (10 s in, 60 s out).
    pub pressure: LevelName,
    pub apps: Vec<AppView>,
    pub problems: Vec<ProblemView>,
    pub tray: TrayUsage,
    /// The tray's hover text, at most 127 characters.
    pub tooltip: String,
    /// The PowerShell N8 Tray runs: this host only watches until it is replaced.
    pub legacy_tray: bool,
    pub settings: SettingsView,
    pub update: UpdateView,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MemoryView {
    pub free_gb: Option<f64>,
    pub total_gb: Option<f64>,
    /// The system memory level with hysteresis (shown).
    pub level: LevelName,
    /// N8 Tray plus every hosted app, in MiB.
    pub n8_mb: f64,
    /// The system's name for the memory line: `Windows`, `macOS` or `Linux`.
    pub system: String,
    /// "Windows 4.2/31.1 GB free · N8 0.3 GB".
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrayUsage {
    pub pid: u32,
    /// The tray process alone.
    pub rss_mb: f64,
    /// Its window's processes (the WebView), while there is a window.
    pub window_mb: f64,
    /// The tray process's private memory: what it alone costs the system.
    pub private_mb: Option<f64>,
    /// The window's processes' private memory (the window's budget metric).
    pub window_private_mb: Option<f64>,
    pub cpu_percent: Option<f64>,
    pub history: Option<HistoryView>,
}

/// The last 15 minutes in 10 s steps, oldest first (0 where nothing ran).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryView {
    pub interval_seconds: u32,
    /// The maximum memory of each step, in MiB.
    pub memory_mb: Vec<f64>,
    /// The average CPU of each step, share of the whole machine.
    pub cpu_percent: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppView {
    pub id: String,
    pub name: String,
    pub group: Option<String>,
    pub description: Option<String>,
    /// The icon's absolute path, when the manifest has one.
    pub icon: Option<String>,
    pub version: String,
    pub level: LevelName,
    /// The state in the contract's vocabulary (spec section 6).
    pub state: String,
    /// The state in words users see.
    pub words: String,
    /// One line for the row.
    pub summary: String,
    pub detail: Option<String>,
    pub running: bool,
    pub should_run: bool,
    pub enabled: bool,
    /// Shown in N8 Tray (false: the app uses its own icon).
    pub hosted: bool,
    pub auto_start: bool,
    /// What is happening now, such as "Stopping".
    pub pending: Option<String>,
    /// The last action's outcome, shown for 10 s.
    pub last_outcome: Option<OutcomeView>,
    pub actions: Vec<ActionView>,
    pub components: Vec<ComponentView>,
    pub open: Vec<OpenView>,
    pub menu: Vec<MenuView>,
    /// Memory of every counted process, in MiB.
    pub memory_mb: Option<f64>,
    pub private_mb: Option<f64>,
    pub cpu_percent: Option<f64>,
    pub gpu: Option<GpuView>,
    /// Memory reported for processes the host cannot see (inside WSL).
    pub namespaced_mb: Option<f64>,
    pub processes: Vec<ProcessView>,
    /// A resource alert, such as "Over its 900 MB warning".
    pub alert: Option<String>,
    pub history: Option<HistoryView>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OutcomeView {
    pub action: String,
    /// `ok`, `already`, `refused` or `failed`.
    pub outcome: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ActionKindView {
    Start,
    Stop,
    Restart,
    App,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ActionView {
    pub id: String,
    pub label: String,
    pub kind: ActionKindView,
    /// Possible now; impossible actions are shown dimmed.
    pub enabled: bool,
    /// Applies to one component at a time.
    pub per_component: bool,
    pub confirm: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ComponentView {
    pub id: String,
    pub name: String,
    pub state: String,
    pub words: String,
    pub level: LevelName,
    pub detail: Option<String>,
    pub memory_mb: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OpenView {
    /// `settings`, `logs`, `folder` or `readme`.
    pub key: String,
    pub label: String,
    pub target: String,
    /// False when the file is missing (shown dimmed; never created to open it).
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MenuView {
    pub label: String,
    /// `open-url`, `open-path` or `action`.
    pub kind: String,
    pub target: String,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GpuView {
    pub mem_mb: Option<f64>,
    pub percent: Option<f64>,
    /// `measured` by the host or `reported` by the app.
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProcessView {
    pub pid: u32,
    pub role: Option<String>,
    pub component: Option<String>,
    pub name: String,
    pub rss_mb: Option<f64>,
    pub private_mb: Option<f64>,
    pub cpu_percent: Option<f64>,
    pub started_at: String,
    /// `verified`, `unreported`, `unverified`, `rejected` or `reported` (a
    /// namespace the host cannot see).
    pub verdict: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProblemView {
    pub file: String,
    pub id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SettingsView {
    pub launch_at_sign_in: bool,
    /// The sign-in entry exists but N8 Tray did not make it.
    pub sign_in_foreign: bool,
    pub notifications: bool,
    pub update_channel: String,
    pub check_updates: bool,
    /// The settings file has an error: defaults in use, nothing saved.
    pub error: Option<String>,
    pub file: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateView {
    pub channel: String,
    pub available: Option<String>,
    /// The release page of the available version.
    pub url: Option<String>,
    pub checked_at: Option<String>,
    /// Why the last check failed.
    pub error: Option<String>,
    /// Installing it: `idle`, `downloading`, `installing` or `failed`.
    pub install: String,
    /// The download's percentage, once its size is known.
    pub progress: Option<u8>,
    /// Why the last install failed.
    pub install_error: Option<String>,
}

/// A warning or error from the tray log, for the status details.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WarningView {
    /// Local time, `HH:MM`.
    pub at: String,
    /// `warn` or `error`.
    pub level: String,
    pub message: String,
    /// How many times in a row it was logged.
    pub count: u32,
}
