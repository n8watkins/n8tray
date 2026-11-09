//! The container an app runs in: a Job Object on Windows (spec 7.2, step 3).
//! Linux systemd scopes come with the Linux phase; until then, and on macOS,
//! there is no container and ownership rests on the process tree.

use std::io;

/// An app's container.
pub struct Container {
    #[cfg(windows)]
    job: crate::win::job::Job,
}

impl Container {
    /// Whether this OS has containers yet.
    pub const SUPPORTED: bool = cfg!(windows);

    /// Creates the app's container (or opens the one that exists), with an
    /// optional memory ceiling for everything in it.
    pub fn create(app_id: &str, hard_cap_mb: Option<u64>) -> io::Result<Container> {
        #[cfg(windows)]
        {
            let job = crate::win::job::Job::create(app_id)?;
            if let Some(mb) = hard_cap_mb {
                job.set_memory_limit(mb * 1024 * 1024)?;
            }
            Ok(Container { job })
        }
        #[cfg(not(windows))]
        {
            let _ = (app_id, hard_cap_mb);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "containers are not available on this OS yet",
            ))
        }
    }

    /// Opens the app's container if it exists, such as after the host restarted.
    pub fn open(app_id: &str) -> Option<Container> {
        #[cfg(windows)]
        {
            crate::win::job::Job::open(app_id).map(|job| Container { job })
        }
        #[cfg(not(windows))]
        {
            let _ = app_id;
            None
        }
    }

    /// Puts a running process into the container.
    pub fn assign(&self, pid: u32) -> io::Result<()> {
        #[cfg(windows)]
        {
            self.job.assign(pid)
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }
    }

    /// Whether a process is in the container.
    pub fn contains(&self, pid: u32) -> bool {
        #[cfg(windows)]
        {
            self.job.contains(pid)
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            false
        }
    }

    /// Every process in the container.
    pub fn pids(&self) -> Vec<u32> {
        #[cfg(windows)]
        {
            self.job.pids()
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }

    /// Ends every process in the container.
    pub fn terminate(&self) -> io::Result<()> {
        #[cfg(windows)]
        {
            self.job.terminate(1)
        }
        #[cfg(not(windows))]
        {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn a_process_started_in_a_container_is_in_it() {
        let id = format!("platform-test-{}", std::process::id());
        let container = Container::create(&id, Some(512)).unwrap();
        let spec = crate::spawn::SpawnSpec {
            program: std::path::Path::new("C:\\Windows\\System32\\ping.exe"),
            args: &["-n".into(), "30".into(), "127.0.0.1".into()],
            cwd: std::path::Path::new("C:\\Windows"),
            hidden: true,
            env: &[],
        };
        let mut child = crate::spawn::spawn(&spec, Some(&container)).unwrap();
        let pid = child.id();
        assert!(container.contains(pid));
        assert!(container.pids().contains(&pid));
        assert!(
            Container::open(&id).unwrap().contains(pid),
            "found again by name"
        );
        assert!(!container.contains(std::process::id()));
        container.terminate().unwrap();
        child.wait().unwrap();
        assert!(container.pids().is_empty());
    }
}
