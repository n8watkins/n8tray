//! Process snapshots: one pass over every process, cheap enough to run every
//! few seconds (spec 7.2, plan 3.2).

use std::io;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

/// One process as the OS reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcInfo {
    pub pid: u32,
    /// 0 when there is none.
    pub parent_pid: u32,
    /// Creation time: to the 100 ns on Windows, whole seconds elsewhere.
    pub started_at: DateTime<Utc>,
    /// The executable's file name, such as `chrome.exe` or `node`.
    pub name: String,
    /// User plus kernel CPU time since the process started.
    pub cpu_time_ms: u64,
    /// Working set on Windows, resident set size elsewhere.
    pub rss_bytes: u64,
    /// Private bytes on Windows; not read elsewhere.
    pub private_bytes: Option<u64>,
}

/// Every process at one moment.
#[derive(Debug, Clone)]
pub struct ProcessSnapshot {
    pub taken_at: DateTime<Utc>,
    pub processes: Vec<ProcInfo>,
}

/// A source of snapshots. It keeps state between passes where the OS API
/// needs it (sysinfo on macOS and Linux).
pub struct Processes {
    #[cfg(not(windows))]
    system: Option<sysinfo::System>,
}

impl Processes {
    pub fn new() -> Self {
        Processes {
            #[cfg(not(windows))]
            system: sysinfo::System::new().ok(),
        }
    }

    /// Every process now.
    pub fn snapshot(&mut self) -> io::Result<ProcessSnapshot> {
        #[cfg(windows)]
        {
            let processes = crate::win::process::snapshot()?;
            Ok(ProcessSnapshot {
                taken_at: Utc::now(),
                processes,
            })
        }
        #[cfg(not(windows))]
        {
            use chrono::TimeZone;
            use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};
            let system = self
                .system
                .as_mut()
                .ok_or_else(|| io::Error::other("the process table cannot be read"))?;
            system.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::nothing().with_memory().with_cpu(),
            );
            let taken_at = Utc::now();
            let processes = system
                .processes()
                .iter()
                // Linux lists threads as tasks; they are part of their process.
                .filter(|(_, p)| p.thread_kind().is_none())
                .map(|(pid, p)| ProcInfo {
                    pid: pid.as_u32(),
                    parent_pid: p.parent().map(|pp| pp.as_u32()).unwrap_or(0),
                    started_at: Utc
                        .timestamp_opt(p.start_time() as i64, 0)
                        .single()
                        .unwrap_or_default(),
                    name: p.name().to_string_lossy().into_owned(),
                    cpu_time_ms: p.accumulated_cpu_time(),
                    rss_bytes: p.memory(),
                    private_bytes: None,
                })
                .collect();
            Ok(ProcessSnapshot {
                taken_at,
                processes,
            })
        }
    }
}

impl Default for Processes {
    fn default() -> Self {
        Self::new()
    }
}

/// The number of logical processors: CPU percentages are shares of all of them.
pub fn logical_cpus() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
}

/// The full path of a process's executable, if the OS tells.
pub fn exe_path(pid: u32) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        crate::win::process::exe_path(pid)
    }
    #[cfg(not(windows))]
    {
        with_process(
            pid,
            sysinfo::ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::Always),
            |p| p.exe().map(|e| e.to_path_buf()),
        )
        .flatten()
    }
}

/// A process's command line, its arguments joined by spaces.
pub fn command_line(pid: u32) -> Option<String> {
    with_process(
        pid,
        sysinfo::ProcessRefreshKind::nothing().with_cmd(sysinfo::UpdateKind::Always),
        |p| {
            p.cmd()
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" ")
        },
    )
}

fn with_process<T>(
    pid: u32,
    kind: sysinfo::ProcessRefreshKind,
    read: impl FnOnce(&sysinfo::Process) -> T,
) -> Option<T> {
    let pid = sysinfo::Pid::from_u32(pid);
    let mut system = sysinfo::System::new().ok()?;
    system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::Some(&[pid]), false, kind);
    system.process(pid).map(read)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_snapshot_lists_this_process_with_its_parent() {
        let snapshot = Processes::new().snapshot().unwrap();
        let me = std::process::id();
        let mine = snapshot
            .processes
            .iter()
            .find(|p| p.pid == me)
            .expect("this process is listed");
        assert!(mine.rss_bytes > 0);
        assert!(mine.parent_pid != 0);
        assert!(mine.started_at <= snapshot.taken_at);
        assert!(
            snapshot.processes.iter().any(|p| p.pid == mine.parent_pid),
            "the parent is listed too"
        );
    }

    #[test]
    fn exe_path_and_command_line_of_this_process() {
        let me = std::process::id();
        let exe = exe_path(me).expect("the executable is known");
        assert_eq!(
            exe.file_name(),
            std::env::current_exe().unwrap().file_name()
        );
        assert!(command_line(me).is_some());
        assert!(logical_cpus() >= 1);
    }

    #[test]
    fn cpu_time_grows_when_this_process_works() {
        let mut source = Processes::new();
        let me = std::process::id();
        let before = source.snapshot().unwrap();
        let start = std::time::Instant::now();
        let mut x = 0u64;
        while start.elapsed() < std::time::Duration::from_millis(300) {
            x = x.wrapping_mul(31).wrapping_add(7);
        }
        std::hint::black_box(x);
        let after = source.snapshot().unwrap();
        let cpu = |s: &ProcessSnapshot| {
            s.processes
                .iter()
                .find(|p| p.pid == me)
                .unwrap()
                .cpu_time_ms
        };
        assert!(cpu(&after) > cpu(&before));
    }
}
