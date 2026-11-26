//! The Win32 clipboard as Unicode text (see `crate::clipboard`).

use std::time::Duration;

use windows_sys::Win32::Foundation::GlobalFree;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

/// `CF_UNICODETEXT` (in `Win32_System_Ole`, a feature this crate does not
/// otherwise need).
const CF_UNICODETEXT: u32 = 13;

/// The clipboard, open until dropped. Another program may hold it for a
/// moment: a few tries, 20 ms apart, before giving up.
struct Open;

impl Open {
    fn new() -> Result<Open, String> {
        for _ in 0..10 {
            // SAFETY: a null owner opens the clipboard for this task; it is
            // closed by `Drop`.
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                return Ok(Open);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err(format!(
            "the clipboard is busy: {}",
            std::io::Error::last_os_error()
        ))
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: the clipboard was opened by `Open::new`.
        unsafe {
            CloseClipboard();
        }
    }
}

pub fn set_text(text: &str) -> Result<(), String> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * std::mem::size_of::<u16>();
    let _open = Open::new()?;
    // SAFETY: the clipboard is open (`_open`); emptying it makes this task
    // its owner.
    if unsafe { EmptyClipboard() } == 0 {
        return Err(format!(
            "the clipboard could not be emptied: {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: a movable global block of `bytes` bytes; it is locked, filled
    // with exactly `wide.len()` UTF-16 units and unlocked before it is handed
    // over. Once `SetClipboardData` takes it the system owns it; if it does
    // not, it is freed here.
    unsafe {
        let block = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if block.is_null() {
            return Err("no memory for the clipboard".into());
        }
        let target = GlobalLock(block) as *mut u16;
        if target.is_null() {
            GlobalFree(block);
            return Err("the clipboard's memory could not be locked".into());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
        GlobalUnlock(block);
        if SetClipboardData(CF_UNICODETEXT, block).is_null() {
            let error = std::io::Error::last_os_error();
            GlobalFree(block);
            return Err(format!("the text could not be put on the clipboard: {error}"));
        }
    }
    Ok(())
}

pub fn text() -> Option<String> {
    // SAFETY: only asks whether the format is there.
    if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) } == 0 {
        return None;
    }
    let _open = Open::new().ok()?;
    // SAFETY: the clipboard is open; the system keeps owning the block, which
    // is locked only while it is read, up to its terminating zero.
    unsafe {
        let block = GetClipboardData(CF_UNICODETEXT);
        if block.is_null() {
            return None;
        }
        let source = GlobalLock(block) as *const u16;
        if source.is_null() {
            return None;
        }
        let mut len = 0;
        while *source.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(source, len));
        GlobalUnlock(block);
        Some(text)
    }
}
