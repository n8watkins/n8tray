//! Starting an app: fixed arguments, no shell, no window, inside its container.

use std::io;
use std::path::Path;
use std::process::{Child, Command, Stdio};

use crate::container::Container;

/// What to start.
pub struct SpawnSpec<'a> {
    pub program: &'a Path,
    pub args: &'a [String],
    pub cwd: &'a Path,
    /// Windows: no console window.
    pub hidden: bool,
    /// Extra environment variables.
    pub env: &'a [(String, String)],
}

/// Starts a program. With a container, the process is created suspended,
/// put into the container and only then resumed, so nothing it starts can
/// escape the container.
pub fn spawn(spec: &SpawnSpec, container: Option<&Container>) -> io::Result<Child> {
    let mut command = Command::new(spec.program);
    command
        .args(spec.args)
        .current_dir(spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .envs(spec.env.iter().map(|(k, v)| (k, v)));

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{
            CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CREATE_SUSPENDED,
        };
        let mut flags = CREATE_NEW_PROCESS_GROUP;
        if spec.hidden {
            flags |= CREATE_NO_WINDOW;
        }
        if container.is_some() {
            flags |= CREATE_SUSPENDED;
        }
        // Apps outlive the host, so they leave any job the host itself runs
        // in (a terminal's, for example) when that job allows it.
        command.creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => {
                command.creation_flags(flags);
                command.spawn()?
            }
        };
        if let Some(container) = container {
            let placed = container.assign(child.id());
            let resumed = crate::win::system::resume(child.id());
            if let Err(e) = placed.and(resumed) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        }
        Ok(child)
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let _ = (spec.hidden, container);
        // Its own process group, so a stop reaches its children too.
        command.process_group(0);
        command.spawn()
    }
}
