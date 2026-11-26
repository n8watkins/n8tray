//! Plain text on the clipboard, for the window's "Copy status" (today's
//! window uses the browser's `navigator.clipboard.writeText`).
//!
//! On Windows, the Win32 clipboard as Unicode text. Elsewhere, not available
//! yet: the call says so and copies nothing.

/// Puts `text` on the clipboard, replacing what was there.
pub fn set_text(text: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        crate::win::clipboard::set_text(text)
    }
    #[cfg(not(windows))]
    {
        let _ = text;
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
