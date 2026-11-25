//! Reading and watching one DWORD registry value (see `crate::theme`).

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, WAIT_OBJECT_0};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, KEY_QUERY_VALUE, REG_NOTIFY_CHANGE_LAST_SET,
    RRF_RT_REG_DWORD, RegCloseKey, RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW,
};
use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};

use super::{Handle, wide};

/// An open key under HKEY_CURRENT_USER, closed when dropped.
struct Key(HKEY);

// A registry key handle may be used and closed from any thread.
unsafe impl Send for Key {}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

/// Opens a key, or says why it could not (a Windows error code).
fn open(path: &str) -> Result<Key, u32> {
    let path = wide(path);
    let mut key: HKEY = null_mut();
    let rc = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            0,
            KEY_QUERY_VALUE | KEY_NOTIFY,
            &mut key,
        )
    };
    if rc == ERROR_SUCCESS {
        Ok(Key(key))
    } else {
        Err(rc)
    }
}

fn read(key: &Key, name: &[u16]) -> Option<u32> {
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            key.0,
            null(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    (rc == ERROR_SUCCESS).then_some(data)
}

/// The DWORD `value` under HKEY_CURRENT_USER\`path`; `None` when the key or
/// the value is missing or is not a DWORD.
pub fn dword(path: &str, value: &str) -> Option<u32> {
    read(&open(path).ok()?, &wide(value))
}

/// What a watch reports: the value (first as it is when the watch is armed,
/// then each time it changes), or why the watch stopped.
pub type Report = Result<Option<u32>, String>;

/// Watches a DWORD from a thread of its own that sleeps until the registry
/// signals that something under the key was set: no polling. `report` gets
/// the value once the watch is armed, then each change, and a last `Err` if
/// the watch ever stops. Returns why it could not start.
pub fn watch_dword(
    path: &str,
    value: &str,
    report: Box<dyn Fn(Report) + Send>,
) -> Result<(), String> {
    let key = open(path).map_err(|rc| format!("cannot open the key (error {rc})"))?;
    // Auto-reset, so each wait is for a change after the last arming.
    let event = Handle::new(unsafe { CreateEventW(null(), 0, 0, null()) }).ok_or_else(|| {
        format!(
            "cannot create an event: {}",
            std::io::Error::last_os_error()
        )
    })?;
    let name = wide(value);
    std::thread::Builder::new()
        .name("registry-watch".into())
        .spawn(move || {
            // Nothing was reported yet: the first read always is, so a change
            // made before the watch was armed is not lost.
            let mut last = None;
            loop {
                // Arm first, then read: a change that lands between the two
                // still signals the event, so none is missed. The arming
                // belongs to this thread, which lives as long as the watch.
                let rc = unsafe {
                    RegNotifyChangeKeyValue(key.0, 0, REG_NOTIFY_CHANGE_LAST_SET, event.raw(), 1)
                };
                if rc != ERROR_SUCCESS {
                    report(Err(format!("cannot watch the key (error {rc})")));
                    return;
                }
                // Windows sets several values under the key on one theme
                // change, so only a real change of ours is reported.
                let now = read(&key, &name);
                if last != Some(now) {
                    last = Some(now);
                    report(Ok(now));
                }
                let waited = unsafe { WaitForSingleObject(event.raw(), INFINITE) };
                if waited != WAIT_OBJECT_0 {
                    report(Err(format!(
                        "the wait failed ({waited}): {}",
                        std::io::Error::last_os_error()
                    )));
                    return;
                }
            }
        })
        .map(|_| ())
        .map_err(|e| format!("cannot start the watch thread: {e}"))
}
