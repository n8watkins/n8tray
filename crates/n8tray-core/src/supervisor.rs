//! One app at run time: its root process and container, the user's intent,
//! the last status the host saw, a stop in progress, crash backoff, and the
//! view the hub shows. The host drives it once per tick.
//!
//! Start, stop and restart go only through what the manifest declares
//! (spec 3.4 and 3.5): `run` to start, `run.stop` to stop, a declared
//! `restart` action or stop then start to restart. Force is used only on
//! processes that pass ownership verification, and on Windows only inside
//! the app's Job Object.

use std::collections::HashMap;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::process::Child;

use chrono::{DateTime, Duration, Utc};
use n8tray_contract::action::Outcome;
use n8tray_contract::manifest::{
    ActionKind, Manifest, MenuItem, OpenTarget, Platform, StatusSource, StopKind,
};
use n8tray_contract::status::{Level, State, Status};
use n8tray_platform::container::Container;
use n8tray_platform::gpu::GpuUse;

use crate::alerts::{Alert, AlertState};
use crate::demand::DemandState;
use crate::discovery::Discovered;
use crate::folders::Folders;
use crate::model::{
    ActionKindView, ActionView, AppView, ComponentView, GpuView, HistoryView, LevelName, MenuView,
    OpenView, OutcomeView, ProcessView,
};
use crate::resolve;
use crate::settings::AppSettings;
use crate::status_io::Observation;
use crate::verify::{Attribution, ProcessTable, Verdict};

/// How long an app the host started may run without a status before it
/// shows as a problem.
pub const STATUS_GRACE_SECONDS: i64 = 60;
/// Restart delays after the first, second and third crash in 10 minutes;
/// the fourth gives up.
pub const BACKOFF_SECONDS: [i64; 3] = [5, 30, 120];
pub const CRASH_WINDOW_MINUTES: i64 = 10;
/// How long an action's outcome stays on the row.
pub const OUTCOME_SECONDS: i64 = 10;
/// How long the app's own figures may differ from the host's before it is noted.
pub const MISMATCH_SECONDS: i64 = 60;

/// The app's root process.
pub struct Root {
    pub pid: u32,
    /// Known once the process table has seen it.
    pub started_at: Option<DateTime<Utc>>,
    /// The handle, when the host started it.
    pub child: Option<Child>,
    pub launched_at: DateTime<Utc>,
}

impl Root {
    pub fn ours(&self) -> bool {
        self.child.is_some()
    }
}

/// A stop in progress.
#[derive(Debug, Clone)]
pub struct Stopping {
    pub deadline: DateTime<Utc>,
    /// Start again once stopped (a restart without a declared action).
    pub then_start: bool,
    pub forced: bool,
}

pub struct AppRuntime {
    pub found: Discovered,
    pub root: Option<Root>,
    pub container: Option<Container>,
    /// The user's intent: should the app run (spec section 6).
    pub wanted: bool,
    pub observation: Observation,
    pub stopping: Option<Stopping>,
    pub crashes: Vec<DateTime<Utc>>,
    pub restart_at: Option<DateTime<Utc>>,
    /// Why the host stopped restarting it.
    pub gave_up: Option<String>,
    /// A long action in flight (shown on the row; blocks other actions).
    pub pending: Option<String>,
    pub last_outcome: Option<(OutcomeView, DateTime<Utc>)>,
    pub attribution: Attribution,
    /// Set by the last failed start, shown until the next try.
    pub start_error: Option<String>,
    /// "Start with N8 Tray" was applied (once, when the app was first found).
    pub auto_start_applied: bool,
    pub alerts: AlertState,
    pub alert: Option<Alert>,
    /// The app's demand rule, when it has one.
    pub demand: Option<DemandState>,
    /// The pressure level the app last accepted.
    pub pressure_sent: Option<LevelName>,
    /// Since when the app's own figures differ from the host's by over 25%.
    pub mismatch_since: Option<DateTime<Utc>>,
}

/// What the app needs from the host for one decision.
pub struct Context<'a> {
    pub folders: &'a Folders,
    pub settings: AppSettings,
    pub now: DateTime<Utc>,
    pub table: &'a ProcessTable,
    /// The PowerShell tray runs: watch only.
    pub watch_only: bool,
    /// GPU use per process, where the OS has counters.
    pub gpu: &'a HashMap<u32, GpuUse>,
    pub history: Option<HistoryView>,
}

/// What the host measured for one app.
#[derive(Debug, Clone, Default)]
pub struct Measured {
    pub memory_mb: Option<f64>,
    pub private_mb: Option<f64>,
    pub cpu_percent: Option<f64>,
    pub gpu: Option<GpuView>,
    pub processes: Vec<ProcessView>,
    pub counted: usize,
    /// For the processes both the app reported and the host counted: what
    /// the app says and what the host measures, in MiB.
    pub reported_mb: f64,
    pub measured_reported_mb: f64,
}

impl AppRuntime {
    pub fn new(found: Discovered) -> AppRuntime {
        AppRuntime {
            found,
            root: None,
            container: None,
            wanted: false,
            observation: Observation::Absent,
            stopping: None,
            crashes: Vec::new(),
            restart_at: None,
            gave_up: None,
            pending: None,
            last_outcome: None,
            attribution: Attribution::default(),
            start_error: None,
            auto_start_applied: false,
            alerts: AlertState::default(),
            alert: None,
            demand: None,
            pressure_sent: None,
            mismatch_since: None,
        }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.found.manifest
    }

    pub fn id(&self) -> &str {
        &self.found.manifest.id
    }

    pub fn supported(&self) -> bool {
        self.manifest().platforms().contains(&Platform::current())
    }

    pub fn status(&self) -> Option<&Status> {
        match &self.observation {
            Observation::Live(status) => Some(status),
            _ => None,
        }
    }

    /// Whether the user wants it started with N8 Tray.
    pub fn auto_start(&self, settings: &AppSettings) -> bool {
        settings
            .auto_start
            .unwrap_or(self.manifest().auto_start_default.unwrap_or(false))
    }

    /// Whether the root process runs (or a live status says the app does).
    pub fn running(&self, table: &ProcessTable) -> bool {
        self.root_alive(table) || self.status().is_some()
    }

    fn root_alive(&self, table: &ProcessTable) -> bool {
        match &self.root {
            None => false,
            Some(root) => match root.started_at {
                Some(started) => table.is_alive(root.pid, started),
                // Not seen by a snapshot yet: alive while the handle says so.
                None => root.child.is_some(),
            },
        }
    }

    /// The outcome of an action, for the row.
    pub fn record(
        &mut self,
        action: &str,
        outcome: Outcome,
        message: Option<String>,
        now: DateTime<Utc>,
    ) {
        let outcome = match outcome {
            Outcome::Ok => "ok",
            Outcome::Already => "already",
            Outcome::Refused => "refused",
            Outcome::Failed => "failed",
        };
        self.last_outcome = Some((
            OutcomeView {
                action: action.to_string(),
                outcome: outcome.to_string(),
                message,
            },
            now,
        ));
    }

    /// Follows the root process: notes its start time, reaps it when it
    /// exits, and adopts the process a live status names. Returns true when
    /// the root went away without being asked to (a crash).
    pub fn follow_root(&mut self, table: &ProcessTable) -> bool {
        // Adopt an app found running: its status names its main process.
        if self.root.is_none()
            && let Some(status) = self.status()
            && table.is_alive(status.pid, status.pid_started_at)
        {
            let pid = status.pid;
            let started = status.pid_started_at;
            self.root = Some(Root {
                pid,
                started_at: Some(table.get(pid).map(|p| p.started_at).unwrap_or(started)),
                child: None,
                launched_at: table.taken_at,
            });
            if self.container.is_none() {
                self.container = Container::open(self.id());
            }
            return false;
        }
        let Some(root) = self.root.as_mut() else {
            return false;
        };
        let mut exited = false;
        if let Some(child) = root.child.as_mut() {
            if let Ok(Some(_)) = child.try_wait() {
                exited = true;
            } else if root.started_at.is_none() {
                root.started_at = table.get(root.pid).map(|p| p.started_at);
            }
        } else if !root.started_at.is_some_and(|s| table.is_alive(root.pid, s)) {
            exited = true;
        }
        if !exited {
            return false;
        }
        let exited_pid = self.root.take().map(|r| r.pid);
        // A status from another process of the same app (it handed over to
        // an instance that already ran) is adopted next tick, not a crash.
        // A status that still names the process that just exited is not one.
        let handed_over = self
            .status()
            .is_some_and(|s| Some(s.pid) != exited_pid && table.is_alive(s.pid, s.pid_started_at));
        if handed_over {
            return false;
        }
        // Its last status is gone with it.
        self.observation = Observation::Absent;
        self.stopping.is_none()
    }

    /// Notes a crash and plans the restart, or gives up after the fourth
    /// crash in 10 minutes.
    pub fn crashed(&mut self, now: DateTime<Utc>) {
        self.crashes
            .retain(|t| now - *t < Duration::minutes(CRASH_WINDOW_MINUTES));
        self.crashes.push(now);
        let n = self.crashes.len();
        if n > BACKOFF_SECONDS.len() {
            self.restart_at = None;
            self.gave_up = Some(format!(
                "it exited {n} times in {CRASH_WINDOW_MINUTES} minutes"
            ));
        } else {
            self.restart_at = Some(now + Duration::seconds(BACKOFF_SECONDS[n - 1]));
        }
    }

    /// The pids the host may force-stop: verified ones only.
    pub fn owned_pids(&self) -> Vec<u32> {
        self.attribution.counted().collect()
    }

    /// The app's view for the hub, the menu and the status file.
    pub fn view(&self, ctx: &Context) -> AppView {
        let m = self.manifest();
        let settings = &ctx.settings;
        let running = self.running(ctx.table);
        let should_run = settings.enabled && self.wanted;
        let busy = self.pending.is_some() || self.stopping.is_some();
        let (state, summary, detail) = self.state(ctx, running, should_run);
        let mut level: LevelName = state.level(should_run).into();
        let alert = self.alert.as_ref().filter(|_| running && settings.enabled);
        if let Some(alert) = alert {
            level = level.max(alert.level);
        }
        if !settings.enabled {
            level = LevelName::Off;
        }
        let can_run = m.run.is_some() && settings.enabled && self.supported() && !ctx.watch_only;

        let mut actions = Vec::new();
        if m.run.is_some() {
            actions.push(ActionView {
                id: "start".into(),
                label: "Start".into(),
                kind: ActionKindView::Start,
                enabled: can_run && !running && !busy,
                per_component: false,
                confirm: false,
            });
            actions.push(ActionView {
                id: "stop".into(),
                label: "Stop".into(),
                kind: ActionKindView::Stop,
                enabled: can_run && running && !busy,
                per_component: false,
                confirm: false,
            });
        }
        let declared_restart = m.actions.iter().any(|a| a.id == "restart");
        if m.run.is_some() || declared_restart {
            actions.push(ActionView {
                id: "restart".into(),
                label: "Restart".into(),
                kind: ActionKindView::Restart,
                enabled: (can_run || declared_restart && !ctx.watch_only) && running && !busy,
                per_component: false,
                confirm: false,
            });
        }
        for action in m.actions.iter().filter(|a| a.id != "restart") {
            let reachable = match action.kind {
                ActionKind::Http => self.status().is_some(),
                ActionKind::Exec => m.run.is_some(),
            };
            actions.push(ActionView {
                id: action.id.clone(),
                label: action.label.clone(),
                kind: ActionKindView::App,
                enabled: settings.enabled && reachable && !busy && !ctx.watch_only,
                per_component: action.per_component(),
                confirm: action.confirm == Some(true),
            });
        }

        let components = self
            .status()
            .and_then(|s| s.components.clone())
            .unwrap_or_default()
            .into_iter()
            .map(|c| {
                let memory: f64 = self.component_memory(ctx.table, &c.id);
                ComponentView {
                    words: c.state.words().to_string(),
                    level: c.state.level(false).into(),
                    state: state_name(c.state),
                    id: c.id,
                    name: c.name,
                    detail: c.detail,
                    memory_mb: (memory > 0.0).then_some(round1(memory)),
                }
            })
            .collect();

        let dir = resolve::install_dir(m, ctx.folders).ok();
        let open = m
            .open
            .iter()
            .map(|(key, target)| {
                let (target, exists) = open_target(dir.as_deref(), target);
                OpenView {
                    key: open_key(*key).into(),
                    label: open_label(*key).into(),
                    target,
                    exists,
                }
            })
            .collect();
        let menu = m
            .menu
            .iter()
            .map(|item| match item {
                MenuItem::OpenUrl { label, url } => MenuView {
                    label: label.clone(),
                    kind: "open-url".into(),
                    target: url.clone(),
                    exists: true,
                },
                MenuItem::OpenPath { label, path } => {
                    let (target, exists) = open_target(dir.as_deref(), path);
                    MenuView {
                        label: label.clone(),
                        kind: "open-path".into(),
                        target,
                        exists,
                    }
                }
                MenuItem::Action { label, action } => MenuView {
                    label: label.clone(),
                    kind: "action".into(),
                    target: action.clone(),
                    exists: true,
                },
            })
            .collect();

        let measured = self.measure(ctx.table, ctx.gpu);
        let namespaced: f64 = self
            .status()
            .and_then(|s| s.resources.namespaces.as_ref())
            .map(|n| n.iter().map(|p| p.rss_mb).sum())
            .unwrap_or(0.0);
        let mut notes = self.found.notes.clone();
        if let Observation::Invalid(why) = &self.observation {
            notes.push(format!("Status ignored: {why}"));
        }
        if matches!(self.observation, Observation::Unauthorized) {
            notes.push("The app refused N8 Tray's token".into());
        }
        if self
            .mismatch_since
            .is_some_and(|since| ctx.now - since >= Duration::seconds(MISMATCH_SECONDS))
        {
            notes.push(format!(
                "The app reports {:.0} MB for its processes; N8 Tray measures {:.0} MB",
                measured.reported_mb, measured.measured_reported_mb
            ));
        }

        AppView {
            id: m.id.clone(),
            name: m.name.clone(),
            group: m.group.clone(),
            description: m.description.clone(),
            icon: m
                .icon
                .as_deref()
                .zip(dir.as_deref())
                .and_then(|(icon, dir)| resolve::inside(dir, icon).ok())
                .map(|p| p.display().to_string()),
            version: m.version.clone(),
            level,
            state: state_name(state),
            words: if settings.enabled {
                state.words().to_string()
            } else {
                "Turned off".into()
            },
            summary,
            detail,
            running,
            should_run,
            enabled: settings.enabled,
            hosted: !settings.own_icon,
            auto_start: self.auto_start(settings),
            pending: self
                .pending
                .clone()
                .or_else(|| self.stopping.as_ref().map(|_| "Stopping".to_string())),
            last_outcome: self
                .last_outcome
                .as_ref()
                .filter(|(_, at)| ctx.now - *at < Duration::seconds(OUTCOME_SECONDS))
                .map(|(o, _)| o.clone()),
            actions,
            components,
            open,
            menu,
            memory_mb: measured.memory_mb,
            private_mb: measured.private_mb,
            cpu_percent: measured.cpu_percent,
            gpu: measured.gpu,
            namespaced_mb: (namespaced > 0.0).then_some(round1(namespaced)),
            processes: measured.processes,
            alert: alert.and_then(|a| a.text.clone()),
            history: ctx.history.clone(),
            notes,
        }
    }

    /// State, summary and detail (spec section 6).
    fn state(
        &self,
        ctx: &Context,
        running: bool,
        should_run: bool,
    ) -> (State, String, Option<String>) {
        if !self.supported() {
            return (State::Unsupported, State::Unsupported.words().into(), None);
        }
        if !ctx.settings.enabled {
            return (State::Stopped, "Turned off in N8 Tray".into(), None);
        }
        if let Some(stopping) = &self.stopping {
            let words = if stopping.then_start {
                "Restarting"
            } else {
                "Stopping"
            };
            return (State::Busy, words.into(), None);
        }
        if let Some(status) = self.status() {
            return (status.state, status.summary.clone(), status.detail.clone());
        }
        if !running && self.demand.as_ref().is_some_and(|d| d.idle) {
            let names = self
                .manifest()
                .lifecycle
                .as_ref()
                .and_then(|l| l.demand.as_ref())
                .map(|d| d.processes.join(", "))
                .unwrap_or_default();
            return (
                State::Idle,
                "Sleeping, starts when needed".into(),
                Some(format!("Starts again when one of these runs: {names}")),
            );
        }
        if running {
            let since = self
                .root
                .as_ref()
                .map(|r| ctx.now - r.launched_at)
                .unwrap_or_default();
            if self.manifest().status == StatusSource::Builtin
                || since < Duration::seconds(STATUS_GRACE_SECONDS)
            {
                return (State::Starting, "Starting".into(), None);
            }
            return (
                State::Degraded,
                "Running, but it reports no status".into(),
                Some(match &self.observation {
                    Observation::Invalid(why) => why.clone(),
                    Observation::Unauthorized => "it refuses N8 Tray's token".into(),
                    _ => "no status file or status endpoint answered".into(),
                }),
            );
        }
        if let Some(why) = &self.gave_up {
            return (State::Failed, format!("Stopped: {why}"), Some(why.clone()));
        }
        if let Some(error) = &self.start_error {
            return (
                State::Failed,
                format!("Stopped: {error}"),
                Some(error.clone()),
            );
        }
        if let Some(at) = self.restart_at {
            let seconds = (at - ctx.now).num_seconds().max(0);
            return (
                State::Starting,
                format!("Exited; restarting in {seconds} s"),
                None,
            );
        }
        let summary = if should_run {
            "Stopped, but it should run".to_string()
        } else {
            "Installed but not running".to_string()
        };
        (State::Stopped, summary, None)
    }

    /// The host's own figures for the app's counted processes, and the
    /// process table for the panel.
    pub fn measure(&self, table: &ProcessTable, gpu: &HashMap<u32, GpuUse>) -> Measured {
        let reported = self
            .status()
            .map(|s| s.resources.processes.clone())
            .unwrap_or_default();
        let mut m = Measured::default();
        let (mut rss, mut private, mut cpu, mut gpu_mem, mut gpu_pct) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let (mut any_private, mut any_cpu, mut any_gpu) = (false, false, false);
        for (&pid, verdict) in &self.attribution.verdicts {
            let os = table.get(pid);
            let r = reported.iter().find(|p| p.pid == pid);
            let counted = matches!(verdict, Verdict::Verified | Verdict::Unreported);
            let process_cpu = table.cpu_percent(pid);
            if counted && let Some(p) = os {
                m.counted += 1;
                rss += mib(p.rss_bytes);
                if let Some(b) = p.private_bytes {
                    private += mib(b);
                    any_private = true;
                }
                if let Some(c) = process_cpu {
                    cpu += c;
                    any_cpu = true;
                }
                if let Some(g) = gpu.get(&pid) {
                    gpu_mem += g.mem_mb;
                    gpu_pct += g.percent;
                    any_gpu = true;
                }
                if let Some(r) = r {
                    m.reported_mb += r.rss_mb;
                    m.measured_reported_mb += mib(p.rss_bytes);
                }
            }
            m.processes.push(ProcessView {
                pid,
                role: r.map(|p| format!("{:?}", p.role).to_lowercase()),
                component: r.and_then(|p| p.component().map(str::to_string)),
                name: os.map(|p| p.name.clone()).unwrap_or_default(),
                rss_mb: os.map(|p| round1(mib(p.rss_bytes))),
                private_mb: os.and_then(|p| p.private_bytes).map(|b| round1(mib(b))),
                cpu_percent: process_cpu.map(round1),
                started_at: os
                    .map(|p| crate::util::timestamp(p.started_at))
                    .or_else(|| r.map(|p| crate::util::timestamp(p.started_at)))
                    .unwrap_or_default(),
                verdict: verdict_name(*verdict).into(),
                note: self.attribution.reasons.get(&pid).cloned(),
            });
        }
        if let Some(status) = self.status() {
            for p in status.resources.namespaces.iter().flatten() {
                m.processes.push(ProcessView {
                    pid: p.pid,
                    role: p.role.map(|r| format!("{r:?}").to_lowercase()),
                    component: p.component.clone().flatten(),
                    name: p.exe.clone().unwrap_or_default(),
                    rss_mb: Some(round1(p.rss_mb)),
                    private_mb: None,
                    cpu_percent: p.cpu_percent,
                    started_at: crate::util::timestamp(p.started_at),
                    verdict: "reported".into(),
                    note: Some(format!("reported, inside {}", p.namespace)),
                });
            }
        }
        if m.counted > 0 {
            m.memory_mb = Some(round1(rss));
            m.private_mb = any_private.then_some(round1(private));
            m.cpu_percent = any_cpu.then_some(round1(cpu));
        }
        // The host's GPU counters where it has them, else what the app reports.
        let reported_gpu: Vec<_> = reported
            .iter()
            .filter(|p| self.attribution.is_owned(p.pid))
            .filter_map(|p| p.gpu.as_ref())
            .collect();
        m.gpu = if any_gpu {
            Some(GpuView {
                mem_mb: Some(round1(gpu_mem)),
                percent: Some(round1(gpu_pct.min(100.0))),
                source: "measured".into(),
            })
        } else if !reported_gpu.is_empty() {
            Some(GpuView {
                mem_mb: Some(round1(reported_gpu.iter().filter_map(|g| g.mem_mb).sum())),
                percent: Some(round1(
                    reported_gpu
                        .iter()
                        .filter_map(|g| g.percent)
                        .sum::<f64>()
                        .min(100.0),
                )),
                source: "reported".into(),
            })
        } else {
            None
        };
        m
    }

    /// Notes when the app's own memory figures stay over 25% off the host's.
    pub fn track_mismatch(&mut self, measured: &Measured, now: DateTime<Utc>) {
        let off = measured.measured_reported_mb > 1.0
            && ((measured.reported_mb - measured.measured_reported_mb).abs()
                / measured.measured_reported_mb)
                > 0.25;
        if !off {
            self.mismatch_since = None;
        } else if self.mismatch_since.is_none() {
            self.mismatch_since = Some(now);
        }
    }

    fn component_memory(&self, table: &ProcessTable, component: &str) -> f64 {
        self.status()
            .map(|s| {
                s.resources
                    .processes
                    .iter()
                    .filter(|p| {
                        p.component() == Some(component) && self.attribution.is_owned(p.pid)
                    })
                    .filter_map(|p| table.get(p.pid))
                    .map(|p| mib(p.rss_bytes))
                    .sum()
            })
            .unwrap_or(0.0)
    }

    /// Whether a stop is due to be forced, and the stop's kind.
    pub fn stop_kind(&self) -> StopKind {
        self.manifest()
            .run
            .as_ref()
            .and_then(|r| r.stop.as_ref())
            .map(|s| s.kind)
            .unwrap_or(StopKind::Terminate)
    }

    pub fn grace(&self) -> Duration {
        Duration::seconds(
            self.manifest()
                .run
                .as_ref()
                .and_then(|r| r.stop.as_ref())
                .and_then(|s| s.grace_seconds)
                .unwrap_or(10) as i64,
        )
    }
}

pub fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

pub fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

pub fn state_name(state: State) -> String {
    serde_json::to_value(state)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn verdict_name(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Verified => "verified",
        Verdict::Unreported => "unreported",
        Verdict::Unverified => "unverified",
        Verdict::Rejected => "rejected",
    }
}

fn open_key(key: OpenTarget) -> &'static str {
    match key {
        OpenTarget::Settings => "settings",
        OpenTarget::Logs => "logs",
        OpenTarget::Folder => "folder",
        OpenTarget::Readme => "readme",
    }
}

fn open_label(key: OpenTarget) -> &'static str {
    match key {
        OpenTarget::Settings => "Open settings",
        OpenTarget::Logs => "Open log",
        OpenTarget::Folder => "Open folder",
        OpenTarget::Readme => "Open README",
    }
}

/// A loopback URL as it is, or a path inside the install folder and whether
/// it exists (a missing file is shown dimmed and never created to open it).
fn open_target(dir: Option<&Path>, target: &str) -> (String, bool) {
    if target.starts_with("http://") || target.starts_with("https://") {
        return (target.to_string(), true);
    }
    match dir.map(|d| resolve::inside_lenient(d, target)) {
        Some(Ok(path)) => {
            let exists = path.exists();
            (path.display().to_string(), exists)
        }
        _ => (target.to_string(), false),
    }
}

/// The level of a view, for the overall level.
pub fn level_of(view: &AppView) -> Level {
    match view.level {
        LevelName::Hidden => Level::Hidden,
        LevelName::Off => Level::Off,
        LevelName::Ok => Level::Ok,
        LevelName::Attention => Level::Attention,
        LevelName::Problem => Level::Problem,
    }
}

/// Where an app sits in the list: what runs or is starting first, then what
/// wants to run but does not (it needs a look), then what is stopped, then
/// what is not installed or cannot run here. Name order within each.
pub fn list_rank(running: bool, state: &str, pending: bool) -> u8 {
    match state {
        _ if running || pending => 0,
        "starting" => 0,
        "failed" => 1,
        "not-installed" | "needs-setup" | "unsupported" => 3,
        _ => 2,
    }
}

/// The order of the apps in the snapshot (the hub and the tray menu).
pub fn list_order(a: &AppView, b: &AppView) -> std::cmp::Ordering {
    let rank = |v: &AppView| list_rank(v.running, &v.state, v.pending.is_some());
    (rank(a), a.name.to_lowercase()).cmp(&(rank(b), b.name.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_apps_come_first() {
        let mut rows = [
            ("Zed", list_rank(false, "not-installed", false)),
            ("Alpha", list_rank(false, "stopped", false)),
            ("Hotkeys", list_rank(true, "ready", false)),
            ("Busy", list_rank(true, "degraded", false)),
            ("Crashed", list_rank(false, "failed", false)),
            ("Coming", list_rank(false, "stopped", true)),
            ("Setup", list_rank(false, "needs-setup", false)),
        ];
        rows.sort_by(|a, b| (a.1, a.0).cmp(&(b.1, b.0)));
        let names: Vec<_> = rows.iter().map(|r| r.0).collect();
        assert_eq!(
            names,
            [
                "Busy", "Coming", "Hotkeys", "Crashed", "Alpha", "Setup", "Zed"
            ]
        );
    }

    fn runtime() -> AppRuntime {
        let manifest: Manifest = serde_json::from_str(
            r#"{ "contract": 2, "id": "a", "name": "A", "version": "1.0.0",
                 "installDir": "/opt/a", "status": { "type": "file" },
                 "run": { "program": "a" } }"#,
        )
        .unwrap();
        AppRuntime::new(Discovered {
            manifest,
            file: PathBuf::from("a.json"),
            notes: vec![],
        })
    }

    #[test]
    fn crash_backoff_then_give_up() {
        let mut app = runtime();
        let now = Utc::now();
        app.crashed(now);
        assert_eq!(app.restart_at, Some(now + Duration::seconds(5)));
        app.crashed(now + Duration::seconds(10));
        assert_eq!(app.restart_at, Some(now + Duration::seconds(40)));
        app.crashed(now + Duration::seconds(60));
        assert_eq!(app.restart_at, Some(now + Duration::seconds(180)));
        app.crashed(now + Duration::seconds(300));
        assert_eq!(app.restart_at, None);
        assert!(app.gave_up.as_deref().unwrap().contains("4 times"));
    }

    #[test]
    fn old_crashes_are_forgotten() {
        let mut app = runtime();
        let now = Utc::now();
        for minutes in [0, 11, 22, 33] {
            app.crashed(now + Duration::minutes(minutes));
        }
        assert!(app.gave_up.is_none());
        assert_eq!(
            app.restart_at,
            Some(now + Duration::minutes(33) + Duration::seconds(5))
        );
    }

    #[test]
    fn views_follow_the_vocabulary() {
        let app = runtime();
        let folders = Folders::under(Path::new("/t"));
        let table = ProcessTable::default();
        let mut settings = AppSettings::default();
        let gpu = HashMap::new();
        let ctx = |settings: AppSettings| Context {
            folders: &folders,
            settings,
            now: Utc::now(),
            table: &table,
            watch_only: false,
            gpu: &gpu,
            history: None,
        };
        let view = app.view(&ctx(settings.clone()));
        assert_eq!(view.state, "stopped");
        assert_eq!(view.level, LevelName::Off);
        assert!(view.actions.iter().any(|a| a.id == "start" && a.enabled));
        assert!(view.actions.iter().any(|a| a.id == "stop" && !a.enabled));

        let mut wanted = runtime();
        wanted.wanted = true;
        assert_eq!(
            wanted.view(&ctx(settings.clone())).level,
            LevelName::Problem
        );

        settings.enabled = false;
        let view = wanted.view(&ctx(settings));
        assert_eq!(view.level, LevelName::Off);
        assert_eq!(view.words, "Turned off");
        assert!(!view.actions.iter().any(|a| a.enabled));
    }
}
