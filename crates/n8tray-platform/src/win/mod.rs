//! The Windows API calls, wrapped once. This is the only place in the crate
//! that uses unsafe code; every function here is safe to call.
#![allow(unsafe_code)]

pub mod gpu;
pub mod job;
pub mod process;
pub mod system;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};

/// A kernel handle that is closed when dropped.
pub struct Handle(HANDLE);

// A kernel handle may be used and closed from any thread.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Handle {
    /// Wraps a handle from an API that returns null on failure.
    pub fn new(raw: HANDLE) -> Option<Handle> {
        (!raw.is_null()).then_some(Handle(raw))
    }

    pub fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// A string as a null-terminated UTF-16 buffer.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
