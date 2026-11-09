//! Job Objects: every app the host starts runs in a named job, so its whole
//! tree is known even after a parent exits (spec 7.2, step 3), the host can
//! enforce `hardCapMb`, and a force stop ends exactly that tree.

use std::io;

use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_JOB_MEMORY,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicProcessIdList,
    JobObjectExtendedLimitInformation, OpenJobObjectW, QueryInformationJobObject,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

use super::{Handle, wide};

const JOB_OBJECT_ALL_ACCESS: u32 = 0x1F001F;

pub struct Job {
    handle: Handle,
}

/// The job's name for an app: one per app id in the user's session.
pub fn name(app_id: &str) -> String {
    format!("Local\\N8Tray.App.{app_id}")
}

impl Job {
    /// Creates the app's job, or opens it if it exists already.
    pub fn create(app_id: &str) -> io::Result<Job> {
        let name = wide(&name(app_id));
        // An existing job of the same name (from before the host restarted)
        // is opened instead: the processes in it are this app's.
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), name.as_ptr()) };
        let handle = Handle::new(raw).ok_or_else(io::Error::last_os_error)?;
        Ok(Job { handle })
    }

    /// Opens the app's job if it exists (after the host restarted).
    pub fn open(app_id: &str) -> Option<Job> {
        let name = wide(&name(app_id));
        let raw = unsafe { OpenJobObjectW(JOB_OBJECT_ALL_ACCESS, 0, name.as_ptr()) };
        Handle::new(raw).map(|handle| Job { handle })
    }

    /// Limits the memory of every process in the job together.
    pub fn set_memory_limit(&self, bytes: u64) -> io::Result<()> {
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_JOB_MEMORY;
        info.JobMemoryLimit = bytes as usize;
        let ok = unsafe {
            SetInformationJobObject(
                self.handle.raw(),
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Puts a process into the job.
    pub fn assign(&self, pid: u32) -> io::Result<()> {
        let process =
            Handle::new(unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) })
                .ok_or_else(io::Error::last_os_error)?;
        self.assign_handle(process.raw())
    }

    pub fn assign_handle(&self, process: windows_sys::Win32::Foundation::HANDLE) -> io::Result<()> {
        if unsafe { AssignProcessToJobObject(self.handle.raw(), process) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Whether a process is in the job.
    pub fn contains(&self, pid: u32) -> bool {
        let Some(process) = super::process::open_query(pid) else {
            return false;
        };
        let mut result = 0;
        let ok = unsafe { IsProcessInJob(process.raw(), self.handle.raw(), &mut result) };
        ok != 0 && result != 0
    }

    /// Every process in the job now.
    pub fn pids(&self) -> Vec<u32> {
        // JOBOBJECT_BASIC_PROCESS_ID_LIST: two u32 counts, then pointer-sized ids.
        let mut capacity = 64usize;
        loop {
            let mut buffer: Vec<usize> = vec![0; 1 + capacity];
            let ok = unsafe {
                QueryInformationJobObject(
                    self.handle.raw(),
                    JobObjectBasicProcessIdList,
                    buffer.as_mut_ptr().cast(),
                    (buffer.len() * std::mem::size_of::<usize>()) as u32,
                    std::ptr::null_mut(),
                )
            };
            let assigned = (buffer[0] & 0xFFFF_FFFF) as usize;
            let listed = (buffer[0] >> 32) as usize;
            if ok == 0 && assigned <= capacity {
                return Vec::new();
            }
            if assigned > listed && capacity < 65_536 {
                capacity = assigned + 16;
                continue;
            }
            return buffer[1..1 + listed.min(capacity)]
                .iter()
                .map(|&pid| pid as u32)
                .collect();
        }
    }

    /// Ends every process in the job.
    pub fn terminate(&self, exit_code: u32) -> io::Result<()> {
        if unsafe { TerminateJobObject(self.handle.raw(), exit_code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
