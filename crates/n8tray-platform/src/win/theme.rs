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

fn open(path: &str) -> Option<Key> {
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
    (rc == ERROR_SUCCESS).then_some(Key(key))
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
    read(&open(path)?, &wide(value))
}

/// Calls `changed` with the value each time it changes, from a thread of its
/// own that sleeps until the registry signals that something under the key
/// was set: no polling. Returns false when the key cannot be opened.
pub fn watch_dword(path: &str, value: &str, changed: Box<dyn Fn(Option<u32>) + Send>) -> bool {
    let Some(key) = open(path) else {
        return false;
    };
    // Auto-reset, so each wait is for a change after the last arming.
    let Some(event) = Handle::new(unsafe { CreateEventW(null(), 0, 0, null()) }) else {
        return false;
    };
    let name = wide(value);
    // Read before the thread starts, so a change made once this returns is
    // reported even if it lands before the thread first arms its watch.
    let mut last = read(&key, &name);
    std::thread::Builder::new()
        .name("registry-watch".into())
        .spawn(move || {
            loop {
                // Arm first, then read: a change that lands between the two
                // still signals the event, so none is missed. The arming
                // belongs to this thread, which lives as long as the watch.
                let rc = unsafe {
                    RegNotifyChangeKeyValue(key.0, 0, REG_NOTIFY_CHANGE_LAST_SET, event.raw(), 1)
                };
                if rc != ERROR_SUCCESS {
                    return;
                }
                // Windows sets several values under the key on one theme
                // change, so only a real change of ours is reported.
                let now = read(&key, &name);
                if now != last {
                    last = now;
                    changed(now);
                }
                if unsafe { WaitForSingleObject(event.raw(), INFINITE) } != WAIT_OBJECT_0 {
                    return;
                }
            }
        })
        .is_ok()
}
