//! The tray window's native style. On Windows a tray flyout is a tool
//! window, so it has no Alt-Tab or taskbar entry, and it asks for Windows
//! 11's rounded corners (a frameless window gets square ones otherwise).
//! Elsewhere the window toolkit's own settings are enough.

/// Styles the window with this native handle (an `HWND` on Windows) as a
/// tray flyout. Call it before the window is first shown. Returns whether
/// anything was done.
pub fn style_flyout(handle: isize) -> bool {
    #[cfg(windows)]
    {
        crate::win::window::style_flyout(handle)
    }
    #[cfg(not(windows))]
    {
        let _ = handle;
        false
    }
}

/// Shows the window without activating it (Windows: `SW_SHOWNOACTIVATE`),
/// for testing on a PC someone is using. Returns whether it was shown.
pub fn show_inactive(handle: isize) -> bool {
    #[cfg(windows)]
    {
        crate::win::window::show_inactive(handle)
    }
    #[cfg(not(windows))]
    {
        let _ = handle;
        false
    }
}
