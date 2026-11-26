//! The tray window's native style. On Windows a tray flyout is a tool
//! window, so it has no Alt-Tab or taskbar entry, and it asks for Windows
//! 11's rounded corners (a frameless window gets square ones otherwise).
//! Elsewhere the window toolkit's own settings are enough.
//!
//! The native shell (`n8tray-native`) also shows, hides and places its window
//! here, since its toolkit re-activates a window it shows, and finds the
//! monitor and work area under the tray icon.

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

/// How far the visible frame sits inside the window's rectangle, as
/// (left, top, right, bottom) in physical pixels. A frameless window on
/// Windows keeps invisible resize borders around what users see, so placing
/// the rectangle 2 px from something leaves the visible edge elsewhere.
/// `None` where the OS draws no such borders or cannot say.
pub fn frame_insets(handle: isize) -> Option<(i32, i32, i32, i32)> {
    #[cfg(windows)]
    {
        crate::win::window::frame_insets(handle)
    }
    #[cfg(not(windows))]
    {
        let _ = handle;
        None
    }
}

/// Hides the window (Windows: `SW_HIDE`), for a window shown with
/// [`show_inactive`], which the window toolkit does not know is shown.
pub fn hide(handle: isize) -> bool {
    #[cfg(windows)]
    {
        crate::win::window::hide(handle)
    }
    #[cfg(not(windows))]
    {
        let _ = handle;
        false
    }
}

/// Shows the window and makes it the foreground window, so Escape and a
/// click away reach it. Windows refuses the foreground to a process that did
/// not just get input; the window then borrows the foreground window's input
/// queue for the call. No key presses are synthesised (a lone Alt would put
/// the foreground window into menu mode). Returns whether it is in front.
pub fn show_active(handle: isize) -> bool {
    #[cfg(windows)]
    {
        crate::win::window::show_active(handle)
    }
    #[cfg(not(windows))]
    {
        let _ = handle;
        false
    }
}

/// Whether the window is shown.
pub fn is_visible(handle: isize) -> bool {
    #[cfg(windows)]
    {
        crate::win::window::is_visible(handle)
    }
    #[cfg(not(windows))]
    {
        let _ = handle;
        false
    }
}

/// Moves and sizes the window's rectangle (physical pixels) without
/// activating it or changing its place in the Z order.
pub fn set_rect(handle: isize, x: i32, y: i32, width: i32, height: i32) -> bool {
    #[cfg(windows)]
    {
        crate::win::window::set_rect(handle, x, y, width, height)
    }
    #[cfg(not(windows))]
    {
        let _ = (handle, x, y, width, height);
        false
    }
}

/// A hidden, never shown tool window to own a flyout (Windows). A window
/// toolkit gives an unowned window a taskbar button and an Alt-Tab entry the
/// moment it is visible, even off screen while it is being created; an owned
/// window gets neither from its first moment. `None` where there is no such
/// thing or the OS refuses. The window lives until the process ends.
pub fn create_owner() -> Option<isize> {
    #[cfg(windows)]
    {
        crate::win::window::create_owner()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Lets the process's menus (the tray menu) follow the system's dark mode,
/// exactly as Tauri's window library (tao) does for the app today: Windows
/// 10 build 17763 or later, uxtheme loaded from System32. Returns whether it
/// was done.
pub fn allow_dark_menus() -> bool {
    #[cfg(windows)]
    {
        crate::win::window::allow_dark_menus()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// A monitor, in physical pixels of the virtual screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Monitor {
    /// (x, y, width, height) of the whole monitor.
    pub bounds: (i32, i32, i32, i32),
    /// (x, y, width, height) of its work area: the monitor less the taskbar.
    pub work: (i32, i32, i32, i32),
    /// Its display scale (1.0 at 96 DPI).
    pub scale: f64,
    /// False when Windows could not give the monitor's own DPI and `scale`
    /// is the system's instead.
    pub scale_from_monitor: bool,
}

/// The monitor that holds this point, or the nearest one. `None` where the
/// platform cannot say.
pub fn monitor_at(x: i32, y: i32) -> Option<Monitor> {
    #[cfg(windows)]
    {
        crate::win::window::monitor_at(x, y)
    }
    #[cfg(not(windows))]
    {
        let _ = (x, y);
        None
    }
}

/// The primary monitor.
pub fn primary_monitor() -> Option<Monitor> {
    monitor_at(0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn the_monitor_under_a_point_has_a_work_area_inside_it() {
        // CI runners have one virtual display; the owner's PC has two.
        let m = primary_monitor().expect("a monitor");
        let (mx, my, mw, mh) = m.bounds;
        let (wx, wy, ww, wh) = m.work;
        assert!(mw > 0 && mh > 0 && ww > 0 && wh > 0);
        assert!(wx >= mx && wy >= my && wx + ww <= mx + mw && wy + wh <= my + mh);
        assert!(m.scale >= 1.0);
        assert!(m.scale_from_monitor, "a real monitor reports its DPI");
        // A point far off every monitor still answers with the nearest one.
        assert!(monitor_at(-100_000, -100_000).is_some());
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_nothing_is_known() {
        assert_eq!(monitor_at(0, 0), None);
        assert_eq!(create_owner(), None);
        assert!(!show_active(0) && !is_visible(0) && !set_rect(0, 0, 0, 1, 1));
    }

    #[cfg(windows)]
    #[test]
    fn a_null_window_is_refused() {
        assert!(!show_active(0) && !is_visible(0) && !set_rect(0, 0, 0, 1, 1));
    }
}
