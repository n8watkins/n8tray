//! Memory status, named mutexes, close requests, force stops and resuming a
//! process that was created suspended.

use std::io;

use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    OpenMutexW, OpenProcess, OpenThread, PROCESS_TERMINATE, ResumeThread, THREAD_SUSPEND_RESUME,
    TerminateProcess,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
};

use super::{Handle, wide};

const SYNCHRONIZE: u32 = 0x0010_0000;

/// (total, available) physical memory in bytes.
pub fn memory() -> Option<(u64, u64)> {
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return None;
    }
    Some((status.ullTotalPhys, status.ullAvailPhys))
}

/// Lets the OS take back this process's resident pages it is not using now.
pub fn trim_own_working_set() -> bool {
    use windows_sys::Win32::System::ProcessStatus::K32EmptyWorkingSet;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    unsafe { K32EmptyWorkingSet(GetCurrentProcess()) != 0 }
}

/// Whether a named mutex exists in this session.
pub fn mutex_exists(name: &str) -> bool {
    let name = wide(name);
    Handle::new(unsafe { OpenMutexW(SYNCHRONIZE, 0, name.as_ptr()) }).is_some()
}

struct CloseSearch {
    pid: u32,
    posted: u32,
}

unsafe extern "system" fn close_window(hwnd: HWND, lparam: LPARAM) -> i32 {
    let search = unsafe { &mut *(lparam as *mut CloseSearch) };
    let mut owner = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut owner) };
    if owner == search.pid && unsafe { PostMessageW(hwnd, WM_CLOSE, 0 as WPARAM, 0) } != 0 {
        search.posted += 1;
    }
    1
}

/// Posts WM_CLOSE to every top-level window of a process. Returns whether
/// any window got it.
pub fn close_request(pid: u32) -> bool {
    let mut search = CloseSearch { pid, posted: 0 };
    unsafe {
        EnumWindows(
            Some(close_window),
            &mut search as *mut CloseSearch as LPARAM,
        )
    };
    search.posted > 0
}

/// Ends a process at once.
pub fn terminate(pid: u32) -> io::Result<()> {
    let process = Handle::new(unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) })
        .ok_or_else(io::Error::last_os_error)?;
    if unsafe { TerminateProcess(process.raw(), 1) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Resumes every thread of a process created with CREATE_SUSPENDED.
pub fn resume(pid: u32) -> io::Result<()> {
    let snapshot = Handle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) })
        .filter(|h| h.raw() as isize != -1)
        .ok_or_else(io::Error::last_os_error)?;
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
    let mut resumed = 0;
    let mut more = unsafe { Thread32First(snapshot.raw(), &mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == pid
            && let Some(thread) =
                Handle::new(unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) })
            && unsafe { ResumeThread(thread.raw()) } != u32::MAX
        {
            resumed += 1;
        }
        more = unsafe { Thread32Next(snapshot.raw(), &mut entry) } != 0;
    }
    if resumed == 0 {
        return Err(io::Error::other(format!(
            "no thread of process {pid} could be resumed"
        )));
    }
    Ok(())
}
