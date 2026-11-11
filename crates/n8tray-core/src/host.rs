//! The host: one thread that owns every app and runs a tick about once a
//! second. Commands from the tray and the window come in on a channel; the
//! result goes out as a [`HostSnapshot`] whenever it changed. Slow work (HTTP
//! actions, `exec` actions, update checks) runs on short-lived worker threads
//! and reports back to the tick, so a hung app never stalls the tray.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration as StdDuration, Instant};

use chrono::{DateTime, Duration, Utc};
use n8tray_contract::action::{ActionResult, Outcome};
use n8tray_contract::manifest::{ActionKind, LifecycleFeature, StatusSource, StopKind};
use n8tray_platform::autostart::{self, Autostart, Location};
use n8tray_platform::container::Container;
use n8tray_platform::gpu::{GpuSampler, GpuUse};
use n8tray_platform::process::Processes;
use n8tray_platform::spawn::{SpawnSpec, spawn};
use n8tray_platform::{folders as pfolders, legacy, memory, signal};
use tracing::{info, warn};

use crate::alerts::Budget;
use crate::demand::{self, DemandState};
use crate::discovery::{self, Problem};
use crate::folders::Folders;
use crate::history::{self, History};
use crate::lease::{HostLock, LeaseWriter};
use crate::logging::Limiter;
use crate::model::{
    HostSnapshot, LevelName, MemoryView, ProblemView, SettingsView, TrayUsage, UpdateView,
};
use crate::pressure::Hysteresis;
use crate::resolve;
use crate::settings::{self, Channel};
use crate::status_io::{self, ActionError, Http, Observation};
use crate::supervisor::{AppRuntime, Context, Root, Stopping, level_of, mib, round1};
use crate::update;
use crate::verify::{self, ProcessTable, Reported};

/// How the host runs.
pub struct Options {
    pub folders: Folders,
    /// The command line the sign-in entry starts; `None` leaves sign-in alone.
    pub autostart_command: Option<String>,
    pub autostart_location: Location,
    /// Look for updates (off in tests).
    pub check_updates: bool,
    /// The tick's period.
    pub tick: StdDuration,
    /// How often the host measures every process while the window is open,
    /// or while something is happening (plan 3.2).
    pub sample_open: StdDuration,
    /// How often it measures while the window is closed.
    pub sample_closed: StdDuration,
}

impl Options {
    pub fn new(folders: Folders) -> Options {
        Options {
            folders,
            autostart_command: None,
            autostart_location: Location::User,
            check_updates: false,
            tick: StdDuration::from_secs(1),
            sample_open: StdDuration::from_secs(3),
            sample_closed: StdDuration::from_secs(10),
        }
    }
}

/// What the tray and the window hear from the host.
#[derive(Debug, Clone)]
pub enum Event {
    Snapshot(Arc<HostSnapshot>),
    /// A notification to show (problems only, once per episode).
    Notify {
        title: String,
        body: String,
    },
}

/// What the tray and the window ask of the host.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// `start`, `stop`, `restart` or an action the manifest declares.
    Action {
        app: String,
        action: String,
        component: Option<String>,
    },
    SetEnabled {
        app: String,
        enabled: bool,
    },
    SetAutoStart {
        app: String,
        on: bool,
    },
    SetOwnIcon {
        app: String,
        on: bool,
    },
    SetLaunchAtSignIn(bool),
    SetNotifications(bool),
    SetUpdateChannel(Channel),
    SetCheckUpdates(bool),
    ReloadSettings,
    CheckUpdates,
    /// How installing an update goes (the app installs it).
    UpdateInstall(update::Install),
    /// The window opened or closed: sampling is faster while it is open.
    WindowOpen(bool),
    Shutdown,
}

#[derive(Debug)]
pub enum StartError {
    /// Another N8 Tray holds the host lock.
    AnotherHost,
    /// No runtime folder (Linux without `XDG_RUNTIME_DIR`).
    NoRunDir,
    Io(io::Error),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AnotherHost => f.write_str("another N8 Tray is running"),
            Self::NoRunDir => {
                f.write_str("there is no runtime folder (XDG_RUNTIME_DIR is not set)")
            }
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

pub struct HostHandle {
    commands: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl HostHandle {
    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    pub fn sender(&self) -> Sender<Command> {
        self.commands.clone()
    }

    /// Stops the host and waits for it. Apps keep running.
    pub fn shutdown(mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for HostHandle {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Starts the host on its own thread.
pub fn start(
    options: Options,
    on_event: impl Fn(Event) + Send + 'static,
) -> Result<HostHandle, StartError> {
    let run = options.folders.run.clone().ok_or(StartError::NoRunDir)?;
    pfolders::ensure_private_dir(&run).map_err(StartError::Io)?;
    let lock = HostLock::acquire(&run)
        .map_err(StartError::Io)?
        .ok_or(StartError::AnotherHost)?;
    std::fs::create_dir_all(&options.folders.modules).map_err(StartError::Io)?;
    let (commands, receiver) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("n8tray-host".into())
        .spawn(move || {
            let mut host = Host::new(options, lock, Box::new(on_event));
            host.run(receiver);
        })
        .map_err(StartError::Io)?;
    Ok(HostHandle {
        commands,
        thread: Some(thread),
    })
}

/// A status reader for one HTTP app, on its own thread.
struct Poller {
    latest: Arc<Mutex<Observation>>,
    stop: Arc<AtomicBool>,
    url: String,
}

impl Poller {
    fn start(
        id: &str,
        url: &str,
        interval: StdDuration,
        timeout: StdDuration,
        run: std::path::PathBuf,
    ) -> Poller {
        let latest = Arc::new(Mutex::new(Observation::Absent));
        let stop = Arc::new(AtomicBool::new(false));
        let (id, url_owned) = (id.to_string(), url.to_string());
        let (latest_t, stop_t) = (Arc::clone(&latest), Arc::clone(&stop));
        let _ = std::thread::Builder::new()
            .name(format!("n8tray-status-{id}"))
            .spawn(move || {
                let http = Http::new();
                let mut token = status_io::read_token(&run, &id);
                while !stop_t.load(Ordering::Relaxed) {
                    if token.is_none() {
                        token = status_io::read_token(&run, &id);
                    }
                    let mut seen = http.status(&url_owned, &id, token.as_deref(), timeout);
                    if seen == Observation::Unauthorized {
                        // The app restarted with a new token (spec section 9).
                        token = status_io::read_token(&run, &id);
                        seen = http.status(&url_owned, &id, token.as_deref(), timeout);
                    }
                    *latest_t.lock().unwrap() = seen;
                    let until = Instant::now() + interval;
                    while Instant::now() < until && !stop_t.load(Ordering::Relaxed) {
                        std::thread::sleep(StdDuration::from_millis(100));
                    }
                }
            });
        Poller {
            latest,
            stop,
            url: url.to_string(),
        }
    }

    fn latest(&self) -> Observation {
        self.latest.lock().unwrap().clone()
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// A worker's report.
enum Done {
    Action {
        app: String,
        action: String,
        result: Result<ActionResult, String>,
    },
    Update(Result<update::Outcome, String>),
    Pressure {
        app: String,
        accepted: bool,
    },
}

#[derive(Default)]
struct UpdateState {
    checked_at: Option<DateTime<Utc>>,
    outcome: Option<update::Outcome>,
    error: Option<String>,
    in_flight: bool,
    install: update::Install,
}

struct Host {
    options: Options,
    _lock: HostLock,
    on_event: Box<dyn Fn(Event) + Send>,
    settings: settings::Loaded,
    apps: BTreeMap<String, AppRuntime>,
    problems: Vec<Problem>,
    pollers: HashMap<String, Poller>,
    processes: Processes,
    table: ProcessTable,
    table_at: Option<Instant>,
    lease: LeaseWriter,
    _watcher: Option<notify::RecommendedWatcher>,
    rescan: Receiver<()>,
    rescan_due: Option<Instant>,
    done_tx: Sender<Done>,
    done_rx: Receiver<Done>,
    pid: u32,
    started_at: DateTime<Utc>,
    legacy: bool,
    legacy_checked: Option<Instant>,
    limiter: Limiter,
    last: Option<Arc<HostSnapshot>>,
    /// When the status file was written, and the states it had.
    status_written: Option<(Instant, String)>,
    update: UpdateState,
    sign_in_foreign: bool,
    window_open: bool,
    cpus: u32,
    gpu_sampler: Option<GpuSampler>,
    gpu: HashMap<u32, GpuUse>,
    gpu_at: Option<Instant>,
    history: History,
    /// System memory: the level shown and the level pushed to apps.
    display_pressure: Hysteresis,
    push_pressure: Hysteresis,
    memory: Option<memory::SystemMemory>,
    memory_notified: bool,
}

impl Host {
    fn new(options: Options, lock: HostLock, on_event: Box<dyn Fn(Event) + Send>) -> Host {
        let pid = std::process::id();
        let mut processes = Processes::new();
        let table = processes
            .snapshot()
            .map(ProcessTable::new)
            .unwrap_or_default();
        let started_at = table
            .get(pid)
            .map(|p| p.started_at)
            .unwrap_or_else(Utc::now);
        let run = options.folders.run.clone().expect("checked in start");
        let (rescan_tx, rescan) = mpsc::channel();
        let watcher = match discovery::watch(&options.folders.modules, rescan_tx) {
            Ok(w) => Some(w),
            Err(e) => {
                warn!("The modules folder is not watched ({e}); it is read again every minute");
                None
            }
        };
        let settings = settings::load(&options.folders.settings_file);
        if let Some(error) = &settings.error {
            warn!(
                "Settings file {} has an error, defaults in use and nothing saved: {error}",
                settings.path.display()
            );
        }
        let (done_tx, done_rx) = mpsc::channel();
        let history_dir = options.folders.history.clone();
        let keep_days = settings.settings.history.keep_days;
        let attention = settings.settings.memory.attention_below_gib;
        let problem = settings.settings.memory.problem_below_gib;
        info!(
            "N8 Tray {} host started, pid {pid}, modules {}, runtime {}",
            crate::VERSION,
            options.folders.modules.display(),
            run.display()
        );
        Host {
            lease: LeaseWriter::new(&run, pid, started_at),
            options,
            _lock: lock,
            on_event,
            settings,
            apps: BTreeMap::new(),
            problems: Vec::new(),
            pollers: HashMap::new(),
            processes,
            table,
            table_at: Some(Instant::now()),
            _watcher: watcher,
            rescan,
            rescan_due: Some(Instant::now()),
            done_tx,
            done_rx,
            pid,
            started_at,
            legacy: false,
            legacy_checked: None,
            limiter: Limiter::default(),
            last: None,
            status_written: None,
            update: UpdateState::default(),
            sign_in_foreign: false,
            window_open: false,
            cpus: n8tray_platform::process::logical_cpus(),
            gpu_sampler: GpuSampler::new(),
            gpu: HashMap::new(),
            gpu_at: None,
            history: History::new(&history_dir, keep_days),
            display_pressure: Hysteresis::display(attention, problem),
            push_pressure: Hysteresis::push(attention, problem),
            memory: None,
            memory_notified: false,
        }
    }

    fn run(&mut self, commands: Receiver<Command>) {
        self.apply_sign_in();
        loop {
            match commands.recv_timeout(self.options.tick) {
                Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(command) => {
                    self.handle(command);
                    while let Ok(next) = commands.try_recv() {
                        if next == Command::Shutdown {
                            info!("N8 Tray host stopped; apps keep running");
                            return;
                        }
                        self.handle(next);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.tick();
        }
        info!("N8 Tray host stopped; apps keep running");
    }

    fn run_dir(&self) -> std::path::PathBuf {
        self.options.folders.run.clone().expect("checked in start")
    }

    fn watch_only(&self) -> bool {
        self.legacy
    }

    // ---- the tick -------------------------------------------------------

    fn tick(&mut self) {
        let now = Utc::now();
        while let Ok(done) = self.done_rx.try_recv() {
            self.finish(done, now);
        }
        while self.rescan.try_recv().is_ok() {
            self.rescan_due = Some(Instant::now() + StdDuration::from_millis(300));
        }
        if self.rescan_due.is_some_and(|due| Instant::now() >= due) {
            self.rescan_due = None;
            self.reconcile();
        }
        let interval = if self.window_open || self.busy() {
            self.options.sample_open
        } else {
            self.options.sample_closed
        };
        let mut sampled = false;
        if self.table_at.is_none_or(|t| t.elapsed() >= interval) {
            match self.processes.snapshot() {
                Ok(snapshot) => {
                    let table = ProcessTable::new(snapshot).with_cpu_from(&self.table, self.cpus);
                    self.table = table;
                    self.table_at = Some(Instant::now());
                    sampled = true;
                }
                Err(e) => {
                    self.table_at = Some(Instant::now());
                    if self.limiter.due("snapshot", &e.to_string(), now) {
                        warn!("The process table could not be read: {e}");
                    }
                }
            }
        }
        self.check_legacy();
        self.observe(now);
        if sampled {
            self.sample(now);
        }
        self.auto_start(now);
        self.supervise(now);
        if !self.watch_only() {
            let hosting = self.hosting();
            if let Err(e) = self.lease.tick(now, &hosting) {
                if self.limiter.due("lease", &e.to_string(), now) {
                    warn!(
                        "Could not write the host lease {}: {e}",
                        self.lease.path().display()
                    );
                }
            } else {
                self.limiter.clear("lease");
            }
        }
        self.maybe_check_updates(now);
        self.publish(now);
    }

    /// Reads the modules folder again and brings the apps in line with it.
    fn reconcile(&mut self) {
        let found = discovery::discover(&self.options.folders.modules);
        for problem in &found.problems {
            let key = format!("problem:{}", problem.file.display());
            if self.limiter.due(&key, &problem.message, Utc::now()) {
                warn!("{}: {}", problem.file.display(), problem.message);
            }
        }
        self.problems = found.problems;
        self.apps.retain(|id, _| found.apps.contains_key(id));
        self.pollers.retain(|id, _| found.apps.contains_key(id));
        for (id, discovered) in found.apps {
            match self.apps.get_mut(&id) {
                Some(app) if app.found.manifest == discovered.manifest => {
                    app.found = discovered;
                }
                Some(app) => {
                    info!(
                        "{id}: manifest changed (version {})",
                        discovered.manifest.version
                    );
                    app.found = discovered;
                }
                None => {
                    info!(
                        "{id}: found {} {} ({})",
                        discovered.manifest.name,
                        discovered.manifest.version,
                        discovered.file.display()
                    );
                    self.apps.insert(id.clone(), AppRuntime::new(discovered));
                }
            }
            let app = &self.apps[&id];
            match &app.manifest().status {
                StatusSource::Http { url, .. } => {
                    let interval =
                        StdDuration::from_secs(app.manifest().status.interval_seconds() as u64);
                    let timeout =
                        StdDuration::from_millis(app.manifest().status.timeout_ms() as u64);
                    if self.pollers.get(&id).is_none_or(|p| &p.url != url) {
                        let poller = Poller::start(&id, url, interval, timeout, self.run_dir());
                        self.pollers.insert(id.clone(), poller);
                    }
                }
                _ => {
                    self.pollers.remove(&id);
                }
            }
        }
    }

    fn check_legacy(&mut self) {
        if self.options.folders.isolated {
            return;
        }
        if self
            .legacy_checked
            .is_some_and(|t| t.elapsed() < StdDuration::from_secs(5))
        {
            return;
        }
        self.legacy_checked = Some(Instant::now());
        let running = legacy::legacy_tray_running();
        if running != self.legacy {
            self.legacy = running;
            if running {
                warn!("The PowerShell N8 Tray runs: this host only watches until it is replaced");
            } else {
                info!("The PowerShell N8 Tray is gone: this host takes over");
            }
        }
    }

    /// Reads every app's status and follows its root process.
    fn observe(&mut self, now: DateTime<Utc>) {
        let run = self.run_dir();
        let table = &self.table;
        for (id, app) in self.apps.iter_mut() {
            let observation = match &app.manifest().status {
                StatusSource::File { .. } => {
                    status_io::read_status_file(&run, id, now, |pid, started| {
                        table.is_alive(pid, started)
                    })
                }
                StatusSource::Http { .. } => self
                    .pollers
                    .get(id)
                    .map(Poller::latest)
                    .unwrap_or(Observation::Absent),
                StatusSource::Builtin => Observation::Absent,
            };
            if let Observation::Live(status) = &observation
                && !matches!(app.observation, Observation::Live(_))
            {
                info!(
                    "{id}: status {} ({})",
                    crate::supervisor::state_name(status.state),
                    status.summary
                );
            }
            app.observation = observation;

            let crashed = app.follow_root(table);
            if crashed && app.wanted && !self.legacy {
                app.crashed(now);
                match (&app.gave_up, app.restart_at) {
                    (Some(why), _) => warn!("{id}: exited; not restarted again: {why}"),
                    (None, Some(at)) => warn!(
                        "{id}: exited unexpectedly; restarting in {} s",
                        (at - now).num_seconds().max(0)
                    ),
                    _ => {}
                }
            } else if crashed {
                info!("{id}: exited");
            }

            // Ownership: the root's tree, the container, and what the app reports.
            let root = app
                .root
                .as_ref()
                .and_then(|r| r.started_at.map(|s| (r.pid, s)));
            let contained = app
                .container
                .as_ref()
                .map(Container::pids)
                .unwrap_or_default();
            let reported: Vec<Reported> = app
                .status()
                .map(|s| {
                    s.resources
                        .processes
                        .iter()
                        .map(|p| Reported {
                            pid: p.pid,
                            started_at: p.started_at,
                        })
                        .collect()
                })
                .unwrap_or_default();
            let install = resolve::install_dir(app.manifest(), &self.options.folders).ok();
            let owns = owns_matcher(app, &self.options.folders);
            let tools = self.options.folders.tools.clone();
            app.attribution = verify::attribute(table, root, &contained, &reported, |pid| {
                location_ok(pid, install.as_deref(), &tools, &owns)
            });
        }
    }

    /// Apps set to start with N8 Tray are started once, when first found:
    /// at host start, or when installed while the host runs.
    fn auto_start(&mut self, now: DateTime<Utc>) {
        if self.watch_only() {
            return;
        }
        let ids: Vec<String> = self
            .apps
            .iter()
            .filter(|(_, a)| !a.auto_start_applied)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let settings = self.settings.settings.app(&id);
            let app = self.apps.get_mut(&id).unwrap();
            app.auto_start_applied = true;
            if !settings.enabled || !app.supported() || !app.auto_start(&settings) {
                continue;
            }
            app.wanted = true;
            // With a demand rule, it starts only while a demand process runs.
            if let Some(demand) = app
                .manifest()
                .lifecycle
                .as_ref()
                .and_then(|l| l.demand.clone())
                && !demand.processes.iter().any(|p| self.table.has_program(p))
            {
                app.demand = Some(DemandState::sleeping());
                info!(
                    "{id}: sleeping until one of {} runs",
                    demand.processes.join(", ")
                );
                continue;
            }
            if !app.running(&self.table) && app.manifest().run.is_some() {
                info!("{id}: starting with N8 Tray");
                if let Err(e) = self.start_app(&id, now) {
                    warn!("{id}: could not start: {e}");
                }
            }
        }
    }

    /// The tray's own processes: itself and its descendants (its window's
    /// WebView), without the apps it started, which are its children too.
    fn own_processes(&self) -> Vec<u32> {
        let apps: std::collections::HashSet<u32> = self
            .apps
            .values()
            .flat_map(|a| a.attribution.verdicts.keys().copied())
            .collect();
        let mut own = vec![self.pid];
        let mut stack = vec![self.pid];
        while let Some(pid) = stack.pop() {
            for child in self.table.children(pid) {
                if !apps.contains(&child) && !own.contains(&child) {
                    own.push(child);
                    stack.push(child);
                }
            }
        }
        own
    }

    /// Whether something is happening that needs the faster sampling:
    /// a start, a stop, an action, a planned restart or an app waiting for
    /// its demand process.
    fn busy(&self) -> bool {
        self.apps.values().any(|a| {
            a.pending.is_some()
                || a.stopping.is_some()
                || a.restart_at.is_some()
                || a.demand.as_ref().is_some_and(|d| d.idle)
                || a.root.as_ref().is_some_and(|r| r.started_at.is_none())
                || (a.root.is_some() && a.status().is_none())
        })
    }

    /// One measurement pass (plan 3.2 to 3.5, 5.3 and 5.5): GPU counters,
    /// system memory pressure, history, alerts, demand rules.
    fn sample(&mut self, now: DateTime<Utc>) {
        // GPU counters are the most expensive read: at most every 10 s.
        if let Some(sampler) = self.gpu_sampler.as_mut()
            && self
                .gpu_at
                .is_none_or(|t| t.elapsed() >= StdDuration::from_secs(10))
        {
            self.gpu = sampler.sample();
            self.gpu_at = Some(Instant::now());
        }

        // System memory: the shown level and the level pushed to apps.
        self.memory = memory::system_memory();
        let settings = self.settings.settings.clone();
        for h in [&mut self.display_pressure, &mut self.push_pressure] {
            h.attention_below = settings.memory.attention_below_gib;
            h.problem_below = settings.memory.problem_below_gib;
        }
        match self.memory {
            Some(m) => {
                let free = m.available_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                let shown = self.display_pressure.step(free, now);
                self.push_pressure.step(free, now);
                if shown == LevelName::Problem && !self.memory_notified {
                    self.memory_notified = true;
                    self.notify("Low memory", &format!("{free:.1} GB free"));
                } else if shown == LevelName::Ok {
                    self.memory_notified = false;
                }
            }
            None => {
                self.display_pressure.reset();
                self.push_pressure.reset();
            }
        }
        let push = self.push_pressure.level().unwrap_or(LevelName::Ok);

        self.history.set_keep_days(settings.history.keep_days);
        let ids: Vec<String> = self.apps.keys().cloned().collect();
        let mut notices = Vec::new();
        let mut decisions = Vec::new();
        let mut pushes = Vec::new();
        for id in &ids {
            let app = self.apps.get_mut(id).unwrap();
            let user = settings.app(id);
            let measured = app.measure(&self.table, &self.gpu);
            app.track_mismatch(&measured, now);
            if measured.counted > 0 {
                let sample = history::Sample {
                    memory_mb: measured.memory_mb.unwrap_or(0.0),
                    private_mb: measured.private_mb,
                    cpu_percent: measured.cpu_percent,
                    gpu_mem_mb: measured.gpu.as_ref().and_then(|g| g.mem_mb),
                    processes: measured.counted,
                };
                if let Err(e) = self.history.record(id, now, &sample)
                    && self.limiter.due("history", &e.to_string(), now)
                {
                    warn!("Could not write the resource history: {e}");
                }
            }
            let budget = Budget::resolve(
                app.manifest()
                    .resources
                    .as_ref()
                    .and_then(|r| r.budget.as_ref()),
                user.budget.as_ref(),
            );
            let alert = app
                .alerts
                .step(measured.memory_mb, measured.cpu_percent, &budget, now);
            if let Some(text) = &alert.notify {
                notices.push((app.manifest().name.clone(), text.clone()));
            }
            if alert.text != app.alert.as_ref().and_then(|a| a.text.clone()) {
                match &alert.text {
                    Some(text) => warn!("{id}: {text}"),
                    None if app.alert.as_ref().is_some_and(|a| a.text.is_some()) => {
                        info!("{id}: back within its budget")
                    }
                    None => {}
                }
            }
            app.alert = Some(alert);

            // Memory pressure, to apps that take it, on every change and when they start.
            if app.manifest().has_feature(LifecycleFeature::Pressure) && app.status().is_some() {
                if app.pressure_sent != Some(push) && !self.legacy {
                    app.pressure_sent = Some(push);
                    pushes.push((id.clone(), push));
                }
            } else {
                app.pressure_sent = None;
            }

            // Demand rules, for apps that should run.
            if let Some(rule) = app
                .manifest()
                .lifecycle
                .as_ref()
                .and_then(|l| l.demand.clone())
                && app.wanted
                && user.enabled
                && !self.legacy
            {
                let present = rule.processes.iter().any(|p| self.table.has_program(p));
                let after = Duration::minutes(rule.stop_after_minutes.unwrap_or(10) as i64);
                let decision = app.demand.get_or_insert_default().step(present, now, after);
                decisions.push((id.clone(), decision, rule.processes.join(", ")));
            }
        }
        // The tray itself, in the history too.
        let tree = self.own_processes();
        let tray_mb: f64 = tree
            .iter()
            .filter_map(|&p| self.table.get(p))
            .map(|p| mib(p.rss_bytes))
            .sum();
        let tray_cpu: Option<f64> = tree.iter().map(|&p| self.table.cpu_percent(p)).sum();
        let _ = self.history.record(
            "n8tray",
            now,
            &history::Sample {
                memory_mb: tray_mb,
                private_mb: self
                    .table
                    .get(self.pid)
                    .and_then(|p| p.private_bytes)
                    .map(mib),
                cpu_percent: tray_cpu,
                gpu_mem_mb: None,
                processes: tree.len(),
            },
        );

        for (title, body) in notices {
            self.notify(&title, &body);
        }
        for (id, decision, names) in decisions {
            match decision {
                demand::Decision::Stop => {
                    info!("{id}: none of {names} has run for a while; stopping it to save memory");
                    self.stop_app(&id, false, now);
                }
                demand::Decision::Start => {
                    info!("{id}: one of {names} runs again; starting it");
                    if !self.apps[&id].running(&self.table)
                        && let Err(e) = self.start_app(&id, now)
                    {
                        warn!("{id}: could not start: {e}");
                    }
                }
                demand::Decision::Nothing => {}
            }
        }
        for (id, level) in pushes {
            self.push_pressure_to(&id, level);
        }
    }

    fn notify(&self, title: &str, body: &str) {
        if self.settings.settings.notifications {
            (self.on_event)(Event::Notify {
                title: title.to_string(),
                body: body.to_string(),
            });
        }
    }

    /// `POST /n8/v2/pressure` (spec 5.4), on a worker.
    fn push_pressure_to(&self, id: &str, level: LevelName) {
        let Some(origin) = self.apps[id].manifest().status.origin().map(str::to_string) else {
            return;
        };
        let pressure = match level {
            LevelName::Problem => n8tray_contract::action::PressureLevel::Problem,
            LevelName::Attention => n8tray_contract::action::PressureLevel::Attention,
            _ => n8tray_contract::action::PressureLevel::Ok,
        };
        let (run, tx, app) = (self.run_dir(), self.done_tx.clone(), id.to_string());
        let _ = std::thread::Builder::new()
            .name(format!("n8tray-pressure-{id}"))
            .spawn(move || {
                let accepted = status_io::read_token(&run, &app).is_some_and(|token| {
                    Http::new()
                        .pressure(&origin, &token, pressure, StdDuration::from_secs(2))
                        .is_ok()
                });
                info!(
                    "{app}: memory pressure {pressure:?} {}",
                    if accepted {
                        "sent"
                    } else {
                        "not accepted; sent again later"
                    }
                );
                let _ = tx.send(Done::Pressure { app, accepted });
            });
    }

    /// Stops in progress, restarts after crashes.
    fn supervise(&mut self, now: DateTime<Utc>) {
        let ids: Vec<String> = self.apps.keys().cloned().collect();
        for id in ids {
            let app = self.apps.get_mut(&id).unwrap();
            if let Some(stopping) = app.stopping.clone() {
                if app.root.is_none() {
                    app.stopping = None;
                    let outcome_message = stopping
                        .forced
                        .then(|| "Stopped by force after the grace period".to_string());
                    app.record("stop", Outcome::Ok, outcome_message, now);
                    info!("{id}: stopped");
                    if stopping.then_start
                        && let Err(e) = self.start_app(&id, now)
                    {
                        let app = self.apps.get_mut(&id).unwrap();
                        app.record("restart", Outcome::Failed, Some(e.clone()), now);
                        warn!("{id}: could not start again: {e}");
                    }
                } else if now >= stopping.deadline {
                    if stopping.forced {
                        app.stopping = None;
                        app.record(
                            "stop",
                            Outcome::Failed,
                            Some("It did not stop, even by force".into()),
                            now,
                        );
                        warn!("{id}: did not stop, even by force");
                    } else {
                        let forced = force_stop(app);
                        match forced {
                            Ok(count) => {
                                warn!(
                                    "{id}: did not stop within its grace period; ended {count} verified processes"
                                );
                                app.stopping = Some(Stopping {
                                    deadline: now + Duration::seconds(5),
                                    forced: true,
                                    ..stopping
                                });
                            }
                            Err(why) => {
                                app.stopping = None;
                                app.record("stop", Outcome::Failed, Some(why.clone()), now);
                                warn!("{id}: did not stop: {why}");
                            }
                        }
                    }
                }
                continue;
            }
            if let Some(at) = app.restart_at
                && now >= at
            {
                app.restart_at = None;
                let settings = self.settings.settings.app(&id);
                if app.wanted && settings.enabled && !app.running(&self.table) && !self.legacy {
                    info!("{id}: restarting after it exited");
                    if let Err(e) = self.start_app(&id, now) {
                        warn!("{id}: could not restart: {e}");
                    }
                }
            }
        }
    }

    // ---- start, stop, actions -------------------------------------------

    fn start_app(&mut self, id: &str, now: DateTime<Utc>) -> Result<(), String> {
        let folders = self.options.folders.clone();
        let app = self.apps.get_mut(id).ok_or("no such app")?;
        let result = (|| {
            let launch = resolve::launch(app.manifest(), &folders, None)?;
            let hard_cap = app
                .manifest()
                .resources
                .as_ref()
                .and_then(|r| r.hard_cap_mb);
            if Container::SUPPORTED {
                match Container::create(id, hard_cap) {
                    Ok(container) => app.container = Some(container),
                    Err(e) => warn!("{id}: no Job Object ({e}); it runs without one"),
                }
            }
            let mut env = Vec::new();
            if folders.isolated {
                if let Some(run) = &folders.run {
                    env.push(("N8TRAY_RUN_DIR".to_string(), run.display().to_string()));
                }
                env.push((
                    "N8TRAY_MODULES_DIR".to_string(),
                    folders.modules.display().to_string(),
                ));
            }
            let child = spawn(
                &SpawnSpec {
                    program: &launch.program,
                    args: &launch.args,
                    cwd: &launch.cwd,
                    hidden: launch.hidden,
                    env: &env,
                },
                app.container.as_ref(),
            )
            .map_err(|e| format!("{}: {e}", launch.program.display()))?;
            info!(
                "{id}: started {} (pid {})",
                launch.program.display(),
                child.id()
            );
            app.root = Some(Root {
                pid: child.id(),
                started_at: None,
                child: Some(child),
                launched_at: now,
            });
            Ok(())
        })();
        app.start_error = result.as_ref().err().cloned();
        result
    }

    /// Asks the app to stop the way its manifest says (spec 3.4).
    fn stop_app(&mut self, id: &str, then_start: bool, now: DateTime<Utc>) -> Outcome {
        let folders = self.options.folders.clone();
        let run = self.run_dir();
        let tx = self.done_tx.clone();
        let Some(app) = self.apps.get_mut(id) else {
            return Outcome::Failed;
        };
        if app.root.is_none() {
            return Outcome::Already;
        }
        match app.stop_kind() {
            StopKind::Http => {
                let origin = app.manifest().status.origin().map(str::to_string);
                let token = status_io::read_token(&run, id);
                match (origin, token) {
                    (Some(origin), Some(token)) => {
                        spawn_worker(tx, id, "stop.request", move || {
                            Http::new()
                                .action(&origin, &token, "stop", None, StdDuration::from_secs(10))
                                .map_err(|e| e.to_string())
                        });
                    }
                    _ => warn!(
                        "{id}: no status endpoint or token to ask it to stop; waiting for the grace period"
                    ),
                }
            }
            StopKind::Exec => {
                let args = app
                    .manifest()
                    .run
                    .as_ref()
                    .and_then(|r| r.stop.as_ref())
                    .and_then(|s| s.args.clone())
                    .unwrap_or_default();
                match resolve::launch(app.manifest(), &folders, Some(&args)) {
                    Ok(launch) => spawn_worker(tx, id, "stop.request", move || run_exec(&launch)),
                    Err(e) => warn!("{id}: cannot ask it to stop: {e}"),
                }
            }
            StopKind::Terminate => {
                let pid = app.root.as_ref().map(|r| r.pid).unwrap_or(0);
                if !signal::close_request(pid) {
                    info!("{id}: it has no window to close; the grace period decides");
                }
            }
        }
        app.stopping = Some(Stopping {
            deadline: now + app.grace(),
            then_start,
            forced: false,
        });
        Outcome::Ok
    }

    fn action(&mut self, id: &str, action: &str, component: Option<String>, now: DateTime<Utc>) {
        let Some(app) = self.apps.get(id) else {
            warn!("{id}: no such app for {action}");
            return;
        };
        let settings = self.settings.settings.app(id);
        let refuse = |host: &mut Host, why: &str| {
            info!("{id}: {action} refused: {why}");
            if let Some(app) = host.apps.get_mut(id) {
                app.record(action, Outcome::Refused, Some(why.to_string()), now);
            }
        };
        if self.legacy {
            return refuse(
                self,
                "the PowerShell N8 Tray runs; it owns the apps until it is replaced",
            );
        }
        if !settings.enabled && action != "stop" {
            return refuse(self, "it is turned off in N8 Tray");
        }
        if app.pending.is_some() || app.stopping.is_some() {
            return refuse(self, "another action is still running");
        }
        let declared_restart = app.manifest().actions.iter().any(|a| a.id == "restart");
        match action {
            "start" => {
                if app.manifest().run.is_none() {
                    return refuse(self, "N8 Tray does not start this app");
                }
                if !app.supported() {
                    return refuse(self, "it does not run on this computer");
                }
                let running = app.running(&self.table);
                let app = self.apps.get_mut(id).unwrap();
                app.wanted = true;
                app.gave_up = None;
                app.crashes.clear();
                app.restart_at = None;
                // The user's start overrides a demand rule's sleep until the
                // rule decides again.
                app.demand = None;
                if running {
                    app.record("start", Outcome::Already, None, now);
                    return;
                }
                match self.start_app(id, now) {
                    Ok(()) => {
                        self.apps
                            .get_mut(id)
                            .unwrap()
                            .record("start", Outcome::Ok, None, now)
                    }
                    Err(e) => {
                        warn!("{id}: could not start: {e}");
                        self.apps.get_mut(id).unwrap().record(
                            "start",
                            Outcome::Failed,
                            Some(e),
                            now,
                        );
                    }
                }
            }
            "stop" => {
                if app.manifest().run.is_none() {
                    return refuse(self, "N8 Tray does not stop this app");
                }
                let app = self.apps.get_mut(id).unwrap();
                app.wanted = false;
                app.restart_at = None;
                app.demand = None;
                let outcome = self.stop_app(id, false, now);
                if outcome != Outcome::Ok {
                    self.apps
                        .get_mut(id)
                        .unwrap()
                        .record("stop", outcome, None, now);
                } else {
                    info!("{id}: stopping");
                }
            }
            "restart" if !declared_restart => {
                if app.manifest().run.is_none() {
                    return refuse(self, "N8 Tray does not restart this app");
                }
                let app = self.apps.get_mut(id).unwrap();
                app.wanted = true;
                app.gave_up = None;
                app.crashes.clear();
                if app.root.is_none() {
                    self.action(id, "start", None, now);
                } else {
                    info!("{id}: restarting");
                    self.stop_app(id, true, now);
                }
            }
            declared => self.declared_action(id, declared, component, now),
        }
    }

    /// An action the manifest declares (spec 3.5), and only those.
    fn declared_action(
        &mut self,
        id: &str,
        action_id: &str,
        component: Option<String>,
        now: DateTime<Utc>,
    ) {
        let run = self.run_dir();
        let folders = self.options.folders.clone();
        let tx = self.done_tx.clone();
        let app = self.apps.get_mut(id).unwrap();
        let Some(action) = app
            .manifest()
            .actions
            .iter()
            .find(|a| a.id == action_id)
            .cloned()
        else {
            app.record(
                action_id,
                Outcome::Refused,
                Some("the app does not declare this action".into()),
                now,
            );
            warn!("{id}: refused undeclared action {action_id}");
            return;
        };
        let listed: Vec<String> = app
            .status()
            .and_then(|s| s.components.as_ref())
            .map(|c| c.iter().map(|c| c.id.clone()).collect())
            .unwrap_or_default();
        let component = if action.per_component() {
            match component {
                Some(c) if listed.contains(&c) && resolve::valid_id(&c) => Some(c),
                Some(c) => {
                    app.record(
                        action_id,
                        Outcome::Refused,
                        Some(format!("{c} is not a component the app lists")),
                        now,
                    );
                    return;
                }
                None => {
                    app.record(
                        action_id,
                        Outcome::Refused,
                        Some("this action needs a component".into()),
                        now,
                    );
                    return;
                }
            }
        } else {
            None
        };
        let label = match &component {
            Some(c) => action.label.replace("{component}", c),
            None => action.label.clone(),
        };
        match action.kind {
            ActionKind::Http => {
                let Some(origin) = app.manifest().status.origin().map(str::to_string) else {
                    app.record(
                        action_id,
                        Outcome::Failed,
                        Some("no status endpoint".into()),
                        now,
                    );
                    return;
                };
                app.pending = Some(label);
                let id_owned = id.to_string();
                let action_owned = action_id.to_string();
                spawn_worker(tx, id, action_id, move || {
                    let http = Http::new();
                    let timeout = StdDuration::from_secs(10);
                    let token = status_io::read_token(&run, &id_owned)
                        .ok_or("the app has no token file")?;
                    match http.action(
                        &origin,
                        &token,
                        &action_owned,
                        component.as_deref(),
                        timeout,
                    ) {
                        Err(ActionError::Unauthorized) => {
                            let token = status_io::read_token(&run, &id_owned)
                                .ok_or("the app has no token file")?;
                            http.action(
                                &origin,
                                &token,
                                &action_owned,
                                component.as_deref(),
                                timeout,
                            )
                            .map_err(|e| e.to_string())
                        }
                        other => other.map_err(|e| e.to_string()),
                    }
                });
            }
            ActionKind::Exec => {
                let args = action.args.clone().unwrap_or_default();
                let args = match resolve::substitute(&args, component.as_deref(), &listed) {
                    Ok(args) => args,
                    Err(e) => {
                        app.record(action_id, Outcome::Refused, Some(e), now);
                        return;
                    }
                };
                match resolve::launch(app.manifest(), &folders, Some(&args)) {
                    Ok(launch) => {
                        app.pending = Some(label);
                        spawn_worker(tx, id, action_id, move || run_exec(&launch));
                    }
                    Err(e) => app.record(action_id, Outcome::Failed, Some(e), now),
                }
            }
        }
        info!("{id}: {action_id} requested");
    }

    fn finish(&mut self, done: Done, now: DateTime<Utc>) {
        match done {
            Done::Action {
                app,
                action,
                result,
            } => {
                let Some(runtime) = self.apps.get_mut(&app) else {
                    return;
                };
                if action == "stop.request" {
                    if let Err(e) = result {
                        warn!("{app}: the stop request failed ({e}); the grace period decides");
                    }
                    return;
                }
                runtime.pending = None;
                match result {
                    Ok(ActionResult::Outcome { outcome, message }) => {
                        info!(
                            "{app}: {action}: {outcome:?}{}",
                            message
                                .as_deref()
                                .map(|m| format!(" ({m})"))
                                .unwrap_or_default()
                        );
                        runtime.record(&action, outcome, message, now);
                    }
                    Ok(ActionResult::Job { job_id }) => {
                        info!("{app}: {action}: job {job_id} started");
                        runtime.record(
                            &action,
                            Outcome::Ok,
                            Some("Started; progress shows in the panel".into()),
                            now,
                        );
                    }
                    Err(e) => {
                        warn!("{app}: {action} failed: {e}");
                        runtime.record(&action, Outcome::Failed, Some(e), now);
                    }
                }
            }
            Done::Pressure { app, accepted } => {
                if !accepted && let Some(runtime) = self.apps.get_mut(&app) {
                    runtime.pressure_sent = None;
                }
            }
            Done::Update(result) => {
                self.update.in_flight = false;
                self.update.checked_at = Some(now);
                match result {
                    Ok(outcome) => {
                        if let update::Outcome::Available { version, .. } = &outcome {
                            info!("Update available: {version}");
                        }
                        self.update.outcome = Some(outcome);
                        self.update.error = None;
                    }
                    Err(e) => {
                        if self.limiter.due("update", &e, now) {
                            warn!("Update check failed: {e}");
                        }
                        self.update.error = Some(e);
                    }
                }
            }
        }
    }

    // ---- commands ---------------------------------------------------------

    fn handle(&mut self, command: Command) {
        let now = Utc::now();
        match command {
            Command::Action {
                app,
                action,
                component,
            } => self.action(&app, &action, component, now),
            Command::SetEnabled { app, enabled } => {
                self.change(|s| s.app_mut(&app).enabled = enabled);
                if !enabled && self.apps.get(&app).is_some_and(|a| a.root.is_some()) {
                    if let Some(runtime) = self.apps.get_mut(&app) {
                        runtime.wanted = false;
                    }
                    self.stop_app(&app, false, now);
                }
            }
            Command::SetAutoStart { app, on } => {
                self.change(|s| s.app_mut(&app).auto_start = Some(on))
            }
            Command::SetOwnIcon { app, on } => self.change(|s| s.app_mut(&app).own_icon = on),
            Command::SetLaunchAtSignIn(on) => {
                self.change(|s| s.startup.launch_at_sign_in = on);
                self.apply_sign_in();
            }
            Command::SetNotifications(on) => self.change(|s| s.notifications = on),
            Command::SetUpdateChannel(channel) => {
                self.change(|s| s.updates.channel = channel);
                self.update = UpdateState::default();
                self.update.checked_at = None;
            }
            Command::SetCheckUpdates(on) => self.change(|s| s.updates.check = on),
            Command::ReloadSettings => {
                self.settings = settings::load(&self.options.folders.settings_file);
                match &self.settings.error {
                    Some(e) => warn!("Settings reloaded with an error, defaults in use: {e}"),
                    None => info!("Settings reloaded"),
                }
                self.apply_sign_in();
            }
            Command::CheckUpdates => self.start_update_check(),
            Command::UpdateInstall(install) => {
                match &install {
                    update::Install::Failed(e) => warn!("Update install failed: {e}"),
                    update::Install::Installing => {
                        info!("Update downloaded and verified; installing")
                    }
                    _ => {}
                }
                self.update.install = install;
            }
            Command::WindowOpen(open) => self.window_open = open,
            Command::Shutdown => {}
        }
    }

    /// Changes and saves the settings, unless the file has an error.
    fn change(&mut self, edit: impl FnOnce(&mut settings::Settings)) {
        if let Some(error) = &self.settings.error {
            warn!("Setting not changed: the settings file has an error ({error})");
            return;
        }
        let before = self.settings.settings.clone();
        edit(&mut self.settings.settings);
        if let Err(e) = self.settings.save() {
            warn!("Settings not saved: {e}");
            self.settings.settings = before;
        }
    }

    /// Makes the sign-in entry follow the setting (repaired at start).
    fn apply_sign_in(&mut self) {
        let Some(command) = self.options.autostart_command.clone() else {
            return;
        };
        let location = self.options.autostart_location.clone();
        let wanted = self.settings.settings.startup.launch_at_sign_in;
        let result = autostart::state(&location, &command).and_then(|state| {
            self.sign_in_foreign = matches!(state, Autostart::Foreign(_));
            match (wanted, state) {
                (true, Autostart::Off | Autostart::Stale(_)) => {
                    info!("Start at sign-in: entry written");
                    autostart::enable(&location, &command)
                }
                (true, Autostart::Foreign(existing)) => {
                    warn!("Start at sign-in: an entry N8 Tray did not make exists ({existing}); left as it is");
                    Ok(())
                }
                (false, Autostart::Ours | Autostart::Stale(_)) => {
                    info!("Start at sign-in: entry removed");
                    autostart::disable(&location, &command)
                }
                _ => Ok(()),
            }
        });
        if let Err(e) = result {
            warn!("Start at sign-in could not be updated: {e}");
        }
    }

    fn maybe_check_updates(&mut self, now: DateTime<Utc>) {
        if !self.options.check_updates
            || !self.settings.settings.updates.check
            || self.update.in_flight
        {
            return;
        }
        if now >= update::next_check(self.started_at, self.update.checked_at) {
            self.start_update_check();
        }
    }

    fn start_update_check(&mut self) {
        if self.update.in_flight {
            return;
        }
        self.update.in_flight = true;
        let channel = self.settings.settings.updates.channel;
        let tx = self.done_tx.clone();
        let _ = std::thread::Builder::new()
            .name("n8tray-update".into())
            .spawn(move || {
                let result = Http::new()
                    .fetch(&update::feed_url(channel), StdDuration::from_secs(15))
                    .and_then(|(status, body)| update::compare(status, &body, crate::VERSION));
                let _ = tx.send(Done::Update(result));
            });
    }

    // ---- the snapshot -----------------------------------------------------

    /// The apps the lease lists: shown in N8 Tray, turned on, for this OS.
    fn hosting(&self) -> Vec<String> {
        self.apps
            .values()
            .filter(|a| {
                let s = self.settings.settings.app(a.id());
                s.enabled && !s.own_icon && a.supported()
            })
            .map(|a| a.id().to_string())
            .collect()
    }

    fn publish(&mut self, now: DateTime<Utc>) {
        let snapshot = Arc::new(self.snapshot(now));
        let changed = self.last.as_deref() != Some(&*snapshot);
        let level_changed = self.last.as_ref().is_none_or(|l| l.level != snapshot.level);
        // The status file follows every change of level or of an app's state
        // at once, and the figures every 10 s.
        let mut states = format!("{:?}", snapshot.level);
        for app in &snapshot.apps {
            states.push_str(&format!(
                "|{}:{}:{}:{:?}",
                app.id, app.state, app.running, app.pending
            ));
        }
        if self.status_written.as_ref().is_none_or(|(t, written)| {
            *written != states || t.elapsed() >= StdDuration::from_secs(10)
        }) {
            match serde_json::to_vec_pretty(&*snapshot) {
                Ok(bytes) => {
                    if let Err(e) =
                        crate::util::write_atomic(&self.options.folders.status_file, &bytes)
                        && self.limiter.due("status-file", &e.to_string(), now)
                    {
                        warn!(
                            "Could not write {}: {e}",
                            self.options.folders.status_file.display()
                        );
                    }
                }
                Err(e) => warn!("Could not encode the status: {e}"),
            }
            self.status_written = Some((Instant::now(), states));
        }
        if level_changed && self.last.is_some() {
            info!(
                "Status: {}{}",
                snapshot.headline,
                snapshot
                    .reason
                    .as_deref()
                    .map(|r| format!(" ({r})"))
                    .unwrap_or_default()
            );
        }
        if changed {
            self.last = Some(Arc::clone(&snapshot));
            (self.on_event)(Event::Snapshot(snapshot));
        }
    }

    fn snapshot(&self, now: DateTime<Utc>) -> HostSnapshot {
        let mut apps = Vec::new();
        for app in self.apps.values() {
            let ctx = Context {
                folders: &self.options.folders,
                settings: self.settings.settings.app(app.id()),
                now,
                table: &self.table,
                watch_only: self.watch_only(),
                gpu: &self.gpu,
                history: self.history.view(app.id(), now),
            };
            apps.push(app.view(&ctx));
        }
        apps.sort_by(|a, b| {
            (a.group.as_deref().unwrap_or("~"), a.name.to_lowercase())
                .cmp(&(b.group.as_deref().unwrap_or("~"), b.name.to_lowercase()))
        });

        // The tray itself: its process, and its window's processes (the
        // WebView runs as its children).
        let own = self.table.get(self.pid);
        let tree = self.own_processes();
        let window_mb: f64 = tree
            .iter()
            .filter(|&&pid| pid != self.pid)
            .filter_map(|&pid| self.table.get(pid))
            .map(|p| mib(p.rss_bytes))
            .sum();
        let window_private_mb: Option<f64> = tree
            .iter()
            .filter(|&&pid| pid != self.pid)
            .filter_map(|&pid| self.table.get(pid))
            .map(|p| p.private_bytes.map(mib))
            .sum::<Option<f64>>()
            .filter(|_| tree.len() > 1)
            .map(round1);
        let tray = TrayUsage {
            pid: self.pid,
            rss_mb: round1(own.map(|p| mib(p.rss_bytes)).unwrap_or(0.0)),
            window_mb: round1(window_mb),
            window_private_mb,
            private_mb: own.and_then(|p| p.private_bytes).map(|b| round1(mib(b))),
            cpu_percent: tree
                .iter()
                .map(|&p| self.table.cpu_percent(p))
                .sum::<Option<f64>>()
                .map(round1),
            history: self.history.view("n8tray", now),
        };

        let hosted: Vec<_> = apps
            .iter()
            .filter(|a| a.hosted && a.level != LevelName::Hidden)
            .collect();
        let n8_mb =
            tray.rss_mb + tray.window_mb + hosted.iter().filter_map(|a| a.memory_mb).sum::<f64>();
        let memory = self.memory_view(n8_mb);

        let mut level = hosted
            .iter()
            .map(|a| level_of(a))
            .max()
            .unwrap_or(n8tray_contract::status::Level::Off);
        let memory_level = match memory.level {
            LevelName::Problem => n8tray_contract::status::Level::Problem,
            LevelName::Attention => n8tray_contract::status::Level::Attention,
            _ => n8tray_contract::status::Level::Ok,
        };
        level = level.max(memory_level);
        let mut level: LevelName = level.into();
        let mut reason = hosted
            .iter()
            .find(|a| {
                a.level == level && matches!(level, LevelName::Attention | LevelName::Problem)
            })
            .map(|a| format!("{}: {}", a.name, a.summary))
            .or_else(|| {
                (memory.level == level && level >= LevelName::Attention)
                    .then(|| format!("Low memory: {}", memory.text))
            });
        if self.legacy {
            level = level.max(LevelName::Attention);
            reason = Some("The PowerShell N8 Tray runs; this one only watches".into());
        }
        if !self.problems.is_empty() && level < LevelName::Attention {
            level = LevelName::Attention;
            reason = Some(format!(
                "{}: {}",
                self.problems[0].file.display(),
                self.problems[0].message
            ));
        }
        let headline = match level {
            LevelName::Problem => "Problem",
            LevelName::Attention => "Needs attention",
            LevelName::Ok => "Healthy",
            LevelName::Off | LevelName::Hidden => "Nothing running",
        }
        .to_string();
        let running_names: Vec<String> = hosted
            .iter()
            .filter(|a| a.running)
            .map(|a| a.name.clone())
            .collect();
        let tooltip = crate::tooltip::format(
            &format!("N8 Tray: {headline}"),
            &running_names,
            std::slice::from_ref(&memory.text),
        );
        let settings = &self.settings.settings;
        let update = UpdateView {
            channel: channel_name(settings.updates.channel).into(),
            available: match &self.update.outcome {
                Some(update::Outcome::Available { version, .. }) => Some(version.clone()),
                _ => None,
            },
            url: match &self.update.outcome {
                Some(update::Outcome::Available { url, .. }) => Some(url.clone()),
                _ => None,
            },
            checked_at: self.update.checked_at.map(crate::util::timestamp),
            error: self.update.error.clone(),
            install: self.update.install.name().into(),
            progress: match &self.update.install {
                update::Install::Downloading(p) => *p,
                _ => None,
            },
            install_error: match &self.update.install {
                update::Install::Failed(e) => Some(e.clone()),
                _ => None,
            },
        };
        HostSnapshot {
            version: crate::VERSION.into(),
            level,
            headline,
            reason,
            running: running_names.len() as u32,
            total: hosted.len() as u32,
            pressure: self.push_pressure.level().unwrap_or(LevelName::Ok),
            memory,
            problems: self
                .problems
                .iter()
                .map(|p| ProblemView {
                    file: p.file.display().to_string(),
                    id: p.id.clone(),
                    message: p.message.clone(),
                })
                .collect(),
            apps,
            tray,
            tooltip,
            legacy_tray: self.legacy,
            settings: SettingsView {
                launch_at_sign_in: settings.startup.launch_at_sign_in,
                sign_in_foreign: self.sign_in_foreign,
                notifications: settings.notifications,
                update_channel: channel_name(settings.updates.channel).into(),
                check_updates: settings.updates.check,
                error: self.settings.error.clone(),
                file: self.settings.path.display().to_string(),
            },
            update,
        }
    }

    fn memory_view(&self, n8_mb: f64) -> MemoryView {
        let os = if cfg!(windows) {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "Linux"
        };
        let gib = |b: u64| b as f64 / (1024.0 * 1024.0 * 1024.0);
        let free = self.memory.map(|m| gib(m.available_bytes));
        let total = self.memory.map(|m| gib(m.total_bytes));
        // The level with hysteresis and dwell, not the raw reading.
        let level = self.display_pressure.level().unwrap_or(LevelName::Off);
        let mut text = match (free, total) {
            (Some(f), Some(t)) => format!("{os} {f:.1}/{t:.1} GB free"),
            _ => format!("{os} memory unknown"),
        };
        text.push_str(&format!(" · N8 {:.1} GB", n8_mb / 1024.0));
        MemoryView {
            free_gb: free.map(|f| (f * 10.0).round() / 10.0),
            total_gb: total.map(|t| (t * 10.0).round() / 10.0),
            level,
            n8_mb: round1(n8_mb),
            system: os.to_string(),
            text,
        }
    }
}

fn channel_name(channel: Channel) -> &'static str {
    match channel {
        Channel::Stable => "stable",
        Channel::Beta => "beta",
    }
}

/// Forces a stop: the whole Job Object on Windows, the verified tree on Unix.
/// Returns how many processes it ended.
fn force_stop(app: &mut AppRuntime) -> Result<usize, String> {
    let owned = app.owned_pids();
    if Container::SUPPORTED {
        let Some(container) = app.container.as_ref() else {
            return Err(
                "it did not stop in time, and N8 Tray force-stops only apps it started".into(),
            );
        };
        let members = container.pids();
        if members.is_empty() {
            return Err(
                "it did not stop in time, and its processes are not in its Job Object".into(),
            );
        }
        container.terminate().map_err(|e| e.to_string())?;
        return Ok(members.len());
    }
    let mut ended = 0;
    for pid in owned {
        if signal::force_stop(pid).is_ok() {
            ended += 1;
        }
    }
    Ok(ended)
}

/// Runs an `exec` action or stop and waits for it (up to 60 s).
fn run_exec(launch: &resolve::Launch) -> Result<ActionResult, String> {
    let mut child = spawn(
        &SpawnSpec {
            program: &launch.program,
            args: &launch.args,
            cwd: &launch.cwd,
            hidden: launch.hidden,
            env: &[],
        },
        None,
    )
    .map_err(|e| format!("{}: {e}", launch.program.display()))?;
    let deadline = Instant::now() + StdDuration::from_secs(60);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(ActionResult::Outcome {
                    outcome: if status.success() {
                        Outcome::Ok
                    } else {
                        Outcome::Failed
                    },
                    message: (!status.success()).then(|| format!("it exited with {status}")),
                });
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(StdDuration::from_millis(100))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("it did not finish within 60 s".into());
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn spawn_worker(
    tx: Sender<Done>,
    app: &str,
    action: &str,
    work: impl FnOnce() -> Result<ActionResult, String> + Send + 'static,
) {
    let (app, action) = (app.to_string(), action.to_string());
    let _ = std::thread::Builder::new()
        .name(format!("n8tray-action-{app}"))
        .spawn(move || {
            let result = work();
            let _ = tx.send(Done::Action {
                app,
                action,
                result,
            });
        });
}

/// The `owns` globs, expanded (spec 3.10).
struct Owns {
    globs: Vec<(globset::GlobMatcher, Option<String>)>,
}

fn owns_matcher(app: &AppRuntime, folders: &Folders) -> Owns {
    let install = resolve::install_dir(app.manifest(), folders).ok();
    let globs = app
        .manifest()
        .owns
        .iter()
        .filter_map(|owned| {
            let pattern = if owned.program.starts_with('%') {
                resolve::expand(&owned.program, folders).ok()?
            } else {
                install.as_ref()?.join(&owned.program)
            };
            let pattern = pattern.display().to_string().replace('\\', "/");
            let glob = globset::GlobBuilder::new(&pattern)
                .case_insensitive(cfg!(windows))
                .literal_separator(true)
                .build()
                .ok()?;
            Some((glob.compile_matcher(), owned.args_contain.clone()))
        })
        .collect();
    Owns { globs }
}

/// Step 4 of ownership: the executable is the app's (spec 7.2).
fn location_ok(
    pid: u32,
    install: Option<&std::path::Path>,
    tools: &std::path::Path,
    owns: &Owns,
) -> bool {
    let Some(exe) = n8tray_platform::process::exe_path(pid) else {
        return false;
    };
    if install.is_some_and(|dir| exe.starts_with(dir)) || exe.starts_with(tools) {
        return true;
    }
    let text = exe.display().to_string().replace('\\', "/");
    owns.globs.iter().any(|(glob, args)| {
        glob.is_match(&text)
            && args.as_ref().is_none_or(|needle| {
                n8tray_platform::process::command_line(pid)
                    .is_some_and(|c| c.contains(needle.as_str()))
            })
    })
}
