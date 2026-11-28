//! Plain text on the clipboard, for the window's "Copy status" (today's
//! window uses the browser's `navigator.clipboard.writeText`).
//!
//! On Windows, the Win32 clipboard as Unicode text. Elsewhere, not available
//! yet: the call says so and copies nothing.

/// Puts `text` on the clipboard, replacing what was there. `owner` is the
/// window that copies (its native handle, an `HWND` on Windows), when there
/// is one: the clipboard then names it as its owner, as Windows expects.
pub fn set_text(owner: Option<isize>, text: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        crate::win::clipboard::set_text(owner, text)
    }
    #[cfg(not(windows))]
    {
        let _ = (owner, text);
        Err("copying is only available on Windows".into())
    }
}

/// The clipboard's text, if it holds any (for tests and checks).
pub fn text() -> Option<String> {
    #[cfg(windows)]
    {
        crate::win::clipboard::text()
    }
    #[cfg(not(windows))]
    {
        None
    }
}
