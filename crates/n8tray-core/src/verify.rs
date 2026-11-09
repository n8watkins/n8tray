//! Ownership verification (spec 7.2): which processes belong to an app, from
//! the host's own process table, so an app can neither hide nor inflate what
//! it costs, and the host never acts on a process that is not the app's.
//!
//! 1. Identity: the pid exists and started within 1 s of what was reported.
//! 2. Tree: the app's root process or a descendant of it, each parent older
//!    than its child (so a reused parent pid breaks the chain).
//! 3. Containment: a member of the app's Job Object (Windows).
//! 4. Location: a reported process that fails 1 to 3 but whose executable is
//!    the app's is "unverified": shown, not counted, never acted on.
//! 5. Nothing hidden: tree and container members the app did not report are
//!    counted and flagged "unreported".

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Utc};
use n8tray_platform::process::{ProcInfo, ProcessSnapshot};

use crate::util::same_start;

/// The process table at one moment, indexed for tree walks.
#[derive(Debug, Clone, Default)]
pub struct ProcessTable {
    pub taken_at: DateTime<Utc>,
    by_pid: HashMap<u32, ProcInfo>,
    children: HashMap<u32, Vec<u32>>,
    /// CPU since the previous table, share of the whole machine (0 to 100).
    cpu: HashMap<u32, f64>,
}

impl ProcessTable {
    pub fn new(snapshot: ProcessSnapshot) -> ProcessTable {
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut by_pid = HashMap::with_capacity(snapshot.processes.len());
        for p in snapshot.processes {
            if p.parent_pid != p.pid {
                children.entry(p.parent_pid).or_default().push(p.pid);
            }
            by_pid.insert(p.pid, p);
        }
        ProcessTable {
            taken_at: snapshot.taken_at,
            by_pid,
            children,
            cpu: HashMap::new(),
        }
    }

    /// Computes every process's CPU since `previous` (the same pid with the
    /// same start time): CPU time used over wall time and all `cpus`, as Task
    /// Manager shows it. A process seen for the first time has none yet.
    pub fn with_cpu_from(mut self, previous: &ProcessTable, cpus: u32) -> ProcessTable {
        let wall_ms = (self.taken_at - previous.taken_at).num_milliseconds();
        if wall_ms <= 0 {
            return self;
        }
        let capacity = wall_ms as f64 * cpus.max(1) as f64;
        for p in self.by_pid.values() {
            if let Some(old) = previous.get(p.pid)
                && old.started_at == p.started_at
                && p.cpu_time_ms >= old.cpu_time_ms
            {
                let used = (p.cpu_time_ms - old.cpu_time_ms) as f64;
                self.cpu
                    .insert(p.pid, (used / capacity * 100.0).clamp(0.0, 100.0));
            }
        }
        self
    }

    /// CPU since the previous table; `None` until a process has two samples.
    pub fn cpu_percent(&self, pid: u32) -> Option<f64> {
        self.cpu.get(&pid).copied()
    }

    pub fn from_processes(processes: Vec<ProcInfo>) -> ProcessTable {
        ProcessTable::new(ProcessSnapshot {
            taken_at: Utc::now(),
            processes,
        })
    }

    pub fn get(&self, pid: u32) -> Option<&ProcInfo> {
        self.by_pid.get(&pid)
    }

    pub fn len(&self) -> usize {
        self.by_pid.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_pid.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ProcInfo> {
        self.by_pid.values()
    }

    /// The direct children of a process (by parent pid; started no earlier
    /// than the parent, so a reused parent pid adopts nothing).
    pub fn children(&self, pid: u32) -> Vec<u32> {
        let Some(parent) = self.by_pid.get(&pid) else {
            return Vec::new();
        };
        self.children
            .get(&pid)
            .into_iter()
            .flatten()
            .copied()
            .filter(|c| {
                self.by_pid.get(c).is_some_and(|c| {
                    c.started_at + chrono::Duration::seconds(1) >= parent.started_at
                })
            })
            .collect()
    }

    /// Step 1: the pid runs and started within 1 s of `started_at`.
    pub fn is_alive(&self, pid: u32, started_at: DateTime<Utc>) -> bool {
        self.get(pid)
            .is_some_and(|p| same_start(p.started_at, started_at))
    }

    /// Step 2: `root` and every descendant, each child started no earlier
    /// than its parent (1 s tolerance for whole-second clocks).
    pub fn tree(&self, root: u32) -> Vec<u32> {
        let mut found = Vec::new();
        if self.get(root).is_none() {
            return found;
        }
        let mut stack = vec![root];
        let mut seen = HashSet::new();
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            found.push(pid);
            let parent_start = self.by_pid[&pid].started_at;
            for &child in self.children.get(&pid).into_iter().flatten() {
                if let Some(c) = self.by_pid.get(&child)
                    && c.started_at + chrono::Duration::seconds(1) >= parent_start
                {
                    stack.push(child);
                }
            }
        }
        found
    }

    /// Whether any process has this executable name (without extension,
    /// compared case-insensitively), for demand rules.
    pub fn has_program(&self, name: &str) -> bool {
        self.by_pid
            .values()
            .any(|p| program_name(&p.name).eq_ignore_ascii_case(name))
    }
}

/// An executable's name without its extension (`chrome.exe` is `chrome`).
pub fn program_name(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, ext)) if ext.eq_ignore_ascii_case("exe") => stem,
        _ => name,
    }
}

/// What the app reported about one process.
#[derive(Debug, Clone, PartialEq)]
pub struct Reported {
    pub pid: u32,
    pub started_at: DateTime<Utc>,
}

/// How a process came to count, or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    /// Reported and owned.
    Verified,
    /// Owned but not reported: counted and flagged.
    Unreported,
    /// Reported, not owned, but the executable is the app's: shown, not counted.
    Unverified,
    /// Reported, and either gone, a reused pid or not the app's: ignored.
    Rejected,
}

/// The processes attributed to one app.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Attribution {
    /// Every pid with a verdict.
    pub verdicts: BTreeMap<u32, Verdict>,
    /// Why each rejected pid was rejected.
    pub reasons: BTreeMap<u32, String>,
}

impl Attribution {
    /// The processes that count: verified and unreported ones.
    pub fn counted(&self) -> impl Iterator<Item = u32> + '_ {
        self.verdicts
            .iter()
            .filter(|(_, v)| matches!(v, Verdict::Verified | Verdict::Unreported))
            .map(|(pid, _)| *pid)
    }

    pub fn is_owned(&self, pid: u32) -> bool {
        matches!(
            self.verdicts.get(&pid),
            Some(Verdict::Verified | Verdict::Unreported)
        )
    }
}

/// Attributes processes to an app.
///
/// - `root`: the app's root process (the one the host started, or the
///   status pid of an app found running) with its creation time.
/// - `contained`: the pids in the app's container, where the OS has one.
/// - `reported`: what the app's status lists under `processes`.
/// - `location_ok`: step 4, asked only for reported pids that fail 1 to 3.
pub fn attribute(
    table: &ProcessTable,
    root: Option<(u32, DateTime<Utc>)>,
    contained: &[u32],
    reported: &[Reported],
    location_ok: impl Fn(u32) -> bool,
) -> Attribution {
    let mut owned: HashSet<u32> = HashSet::new();
    if let Some((pid, started_at)) = root
        && table.is_alive(pid, started_at)
    {
        owned.extend(table.tree(pid));
    }
    for &pid in contained {
        if table.get(pid).is_some() {
            // A container member's own children are the app's too.
            owned.extend(table.tree(pid));
        }
    }

    let mut attribution = Attribution::default();
    let mut reported_pids = HashSet::new();
    for r in reported {
        let verdict = if !table.is_alive(r.pid, r.started_at) {
            attribution.reasons.insert(
                r.pid,
                if table.get(r.pid).is_some() {
                    "pid reused: the process started at another time".into()
                } else {
                    "no such process".into()
                },
            );
            Verdict::Rejected
        } else if owned.contains(&r.pid) {
            Verdict::Verified
        } else if location_ok(r.pid) {
            Verdict::Unverified
        } else {
            attribution
                .reasons
                .insert(r.pid, "not the app's: outside its process tree".into());
            Verdict::Rejected
        };
        if verdict == Verdict::Rejected && owned.contains(&r.pid) {
            // A wrong start time for a process that is the app's: the real
            // process still counts, as unreported, below.
            attribution.reasons.remove(&r.pid);
            continue;
        }
        reported_pids.insert(r.pid);
        attribution.verdicts.insert(r.pid, verdict);
    }
    for pid in owned {
        if !reported_pids.contains(&pid) {
            attribution.verdicts.insert(pid, Verdict::Unreported);
        }
    }
    attribution
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
    }

    fn proc(pid: u32, parent: u32, started: i64) -> ProcInfo {
        ProcInfo {
            pid,
            parent_pid: parent,
            started_at: at(started),
            name: format!("p{pid}.exe"),
            cpu_time_ms: 0,
            rss_bytes: 10 << 20,
            private_bytes: None,
        }
    }

    fn table() -> ProcessTable {
        ProcessTable::from_processes(vec![
            proc(1, 0, 0),
            proc(100, 1, 10),   // the app's root
            proc(101, 100, 11), // a reported worker
            proc(102, 100, 12), // an unreported worker
            proc(103, 102, 13), // its grandchild
            proc(200, 1, 20),   // someone else
            proc(300, 999, 5),  // orphan whose parent pid could be reused
            proc(999, 100, 30), // started after 300: not 300's parent
        ])
    }

    fn reported(pid: u32, started: i64) -> Reported {
        Reported {
            pid,
            started_at: at(started),
        }
    }

    #[test]
    fn verified_unreported_and_rejected() {
        let t = table();
        let a = attribute(
            &t,
            Some((100, at(10))),
            &[],
            &[reported(100, 10), reported(101, 11), reported(200, 20)],
            |_| false,
        );
        assert_eq!(a.verdicts[&100], Verdict::Verified);
        assert_eq!(a.verdicts[&101], Verdict::Verified);
        assert_eq!(a.verdicts[&102], Verdict::Unreported);
        assert_eq!(a.verdicts[&103], Verdict::Unreported, "grandchildren count");
        assert_eq!(a.verdicts[&200], Verdict::Rejected, "a lie is rejected");
        assert_eq!(a.verdicts[&999], Verdict::Unreported);
        assert!(
            !a.verdicts.contains_key(&300),
            "a reused parent pid breaks the chain"
        );
        let mut counted: Vec<_> = a.counted().collect();
        counted.sort();
        assert_eq!(counted, [100, 101, 102, 103, 999]);
    }

    #[test]
    fn a_reused_pid_is_rejected() {
        let t = table();
        let a = attribute(&t, Some((100, at(10))), &[], &[reported(101, 500)], |_| {
            true
        });
        assert_eq!(
            a.verdicts[&101],
            Verdict::Unreported,
            "the real 101 still counts"
        );
        let a = attribute(&t, None, &[], &[reported(101, 500)], |_| true);
        assert_eq!(a.verdicts[&101], Verdict::Rejected);
        assert!(a.reasons[&101].contains("reused"));
    }

    #[test]
    fn a_root_that_is_gone_owns_nothing() {
        let t = table();
        let a = attribute(&t, Some((100, at(99))), &[], &[], |_| false);
        assert!(a.verdicts.is_empty());
    }

    #[test]
    fn containers_keep_orphans() {
        let t = table();
        // 300's parent exited; the container still knows it.
        let a = attribute(&t, Some((100, at(10))), &[300], &[reported(300, 5)], |_| {
            false
        });
        assert_eq!(a.verdicts[&300], Verdict::Verified);
    }

    #[test]
    fn location_makes_unverified_not_owned() {
        let t = table();
        let a = attribute(&t, Some((100, at(10))), &[], &[reported(200, 20)], |pid| {
            pid == 200
        });
        assert_eq!(a.verdicts[&200], Verdict::Unverified);
        assert!(!a.is_owned(200));
    }

    #[test]
    fn cpu_is_a_share_of_the_whole_machine() {
        let mut first = proc(100, 1, 10);
        first.cpu_time_ms = 1_000;
        let mut later = first.clone();
        later.cpu_time_ms = 1_500;
        let reused = proc(200, 1, 99);
        let old = ProcessTable::new(ProcessSnapshot {
            taken_at: at(100),
            processes: vec![first, proc(200, 1, 20)],
        });
        let new = ProcessTable::new(ProcessSnapshot {
            taken_at: at(110),
            processes: vec![later, reused, proc(300, 1, 105)],
        })
        .with_cpu_from(&old, 4);
        // 500 ms of CPU in 10 s on 4 processors: 1.25% of the machine.
        assert_eq!(new.cpu_percent(100), Some(1.25));
        assert_eq!(new.cpu_percent(200), None, "a reused pid starts over");
        assert_eq!(new.cpu_percent(300), None, "new: measuring");
    }

    #[test]
    fn program_names() {
        assert_eq!(program_name("chrome.exe"), "chrome");
        assert_eq!(program_name("Chrome.EXE"), "Chrome");
        assert_eq!(program_name("node"), "node");
        assert_eq!(program_name("python3.12"), "python3.12");
        assert!(table().has_program("P100"));
    }
}
