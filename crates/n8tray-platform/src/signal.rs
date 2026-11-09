//! Stopping processes: a polite request first, force only for processes the
//! caller verified (spec 3.4 and 7.2).

use std::io;

/// Asks a process to close: WM_CLOSE to its windows on Windows, SIGTERM
/// elsewhere. Returns whether the request was delivered.
pub fn close_request(pid: u32) -> bool {
    #[cfg(windows)]
    {
        crate::win::system::close_request(pid)
    }
    #[cfg(unix)]
    {
        #[allow(unsafe_code)]
        let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        result == 0
    }
}

/// Ends a process at once. The caller must have verified that the process
/// belongs to the app.
pub fn force_stop(pid: u32) -> io::Result<()> {
    #[cfg(windows)]
    {
        crate::win::system::terminate(pid)
    }
    #[cfg(unix)]
    {
        #[allow(unsafe_code)]
        let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
