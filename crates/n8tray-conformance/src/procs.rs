//! Process facts from the OS, through sysinfo: identity, parentage, memory and CPU.

use std::collections::HashMap;

use chrono::{DateTime, TimeZone, Utc};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// One process as the OS reports it.
#[derive(Debug, Clone)]
pub struct Proc {
    pub pid: u32,
    pub parent: Option<u32>,
    /// Creation time. sysinfo gives whole seconds, so comparisons allow for that.
    pub started_at: DateTime<Utc>,
    pub rss_mb: f64,
    /// Share of the whole machine, 0 to 100.
    pub cpu_percent: f64,
    pub exe: Option<String>,
}

/// A process table, refreshed on demand.
pub struct Table {
    system: System,
    cpus: f64,
    sampled: bool,
}

impl Table {
    pub fn new() -> Self {
        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1) as f64;
        Table {
            system: System::new().expect("the process table is readable"),
            cpus,
            sampled: false,
        }
    }

    /// Reads every process again. CPU figures need two refreshes.
    pub fn refresh(&mut self) {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_memory()
                .with_cpu()
                .with_exe(UpdateKind::OnlyIfNotSet),
        );
        self.sampled = true;
    }

    pub fn get(&self, pid: u32) -> Option<Proc> {
        self.system.process(Pid::from_u32(pid)).map(|p| Proc {
            pid,
            parent: p.parent().map(|pp| pp.as_u32()),
            started_at: Utc
                .timestamp_opt(p.start_time() as i64, 0)
                .single()
                .unwrap_or_default(),
            rss_mb: p.memory() as f64 / (1024.0 * 1024.0),
            cpu_percent: (p.cpu_usage() as f64 / self.cpus).clamp(0.0, 100.0),
            exe: p.exe().map(|e| e.display().to_string()),
        })
    }

    /// Every process's parent, for tree walks.
    pub fn parents(&self) -> HashMap<u32, u32> {
        self.system
            .processes()
            .iter()
            // Linux lists threads as tasks; they are part of their process (spec 4.1).
            .filter(|(_, p)| p.thread_kind().is_none())
            .filter_map(|(pid, p)| p.parent().map(|parent| (pid.as_u32(), parent.as_u32())))
            .collect()
    }

    /// Whether `pid` is `root` or descends from it, with each parent older
    /// than its child (spec 7.2, step 2), so a reused parent pid breaks the chain.
    pub fn descends_from(&self, pid: u32, root: u32) -> bool {
        let mut current = pid;
        for _ in 0..64 {
            if current == root {
                return true;
            }
            let (Some(child), Some(parent)) =
                (self.get(current), self.get(current).and_then(|p| p.parent))
            else {
                return false;
            };
            match self.get(parent) {
                Some(p) if p.started_at <= child.started_at + chrono::Duration::seconds(1) => {
                    current = parent
                }
                _ => return false,
            }
        }
        false
    }

    /// Descendants of `root` (not `root` itself).
    pub fn descendants(&self, root: u32) -> Vec<u32> {
        let parents = self.parents();
        let mut found = Vec::new();
        for &pid in parents.keys() {
            if pid != root && self.descends_from(pid, root) {
                found.push(pid);
            }
        }
        found.sort_unstable();
        found
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether a reported creation time matches the OS's, allowing for sysinfo's
/// whole seconds: the OS time is the reported one rounded down, give or take 1 s.
pub fn same_start(reported: DateTime<Utc>, os: DateTime<Utc>) -> bool {
    let difference = (reported - os).num_milliseconds();
    (-1000..2000).contains(&difference)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_in_the_table_and_descends_from_itself() {
        let mut table = Table::new();
        table.refresh();
        let me = std::process::id();
        let proc = table.get(me).expect("this process is listed");
        assert!(proc.rss_mb > 0.0);
        assert!(table.descends_from(me, me));
        assert!(same_start(proc.started_at, proc.started_at));
    }

    #[test]
    fn a_child_descends_from_its_parent() {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "procs::tests::sleeper",
                "--nocapture",
            ])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut table = Table::new();
        table.refresh();
        let result = table.descends_from(child.id(), std::process::id());
        let _ = child.kill();
        let _ = child.wait();
        assert!(result);
    }

    /// Run only as the child of `a_child_descends_from_its_parent`.
    #[test]
    #[ignore = "a helper process, not a test"]
    fn sleeper() {
        std::thread::sleep(std::time::Duration::from_secs(20));
    }

    #[test]
    fn start_time_tolerance() {
        let os = Utc.timestamp_opt(1000, 0).single().unwrap();
        assert!(same_start(os + chrono::Duration::milliseconds(999), os));
        assert!(same_start(os - chrono::Duration::milliseconds(500), os));
        assert!(!same_start(os + chrono::Duration::seconds(3), os));
    }
}
