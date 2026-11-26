//! A tool window with rounded corners, showing, hiding and placing it, and
//! the monitors (see `crate::window`).

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmGetWindowAttribute, DwmSetWindowAttribute,
};
use windows_sys::Win32::Graphics::Gdi::{
    ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForSystem, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, GWL_EXSTYLE, GetClientRect, GetForegroundWindow,
    GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsHungAppWindow, IsWindowVisible,
    SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_POPUP,
};

pub fn style_flyout(handle: isize) -> bool {
    let hwnd = handle as HWND;
    if hwnd.is_null() {
        return false;
    }
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let wanted = (style | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize);
        if wanted != style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        // Windows 10 has no such attribute and returns an error: square corners.
        let round: i32 = DWMWCP_ROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&round as *const i32).cast(),
            size_of::<i32>() as u32,
        );
    }
    true
}

pub fn show_inactive(handle: isize) -> bool {
    let hwnd = handle as HWND;
    if hwnd.is_null() {
        return false;
    }
    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    true
}

pub fn frame_insets(handle: isize) -> Option<(i32, i32, i32, i32)> {
    let hwnd = handle as HWND;
    if hwnd.is_null() {
        return None;
    }
    let empty = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let (mut outer, mut visible, mut client) = (empty, empty, empty);
    unsafe {
        if GetWindowRect(hwnd, &mut outer) == 0 {
            return None;
        }
        let ok = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&mut visible as *mut RECT).cast(),
            size_of::<RECT>() as u32,
        ) == 0;
        let unknown = visible.right <= visible.left
            || (visible.left == outer.left && visible.right == outer.right);
        if !ok || unknown {
            // No frame bounds yet: what users see is the client area.
            let mut origin = POINT { x: 0, y: 0 };
            if GetClientRect(hwnd, &mut client) == 0 || ClientToScreen(hwnd, &mut origin) == 0 {
                return None;
            }
            visible = RECT {
                left: origin.x,
                top: origin.y,
                right: origin.x + client.right,
                bottom: origin.y + client.bottom,
            };
        }
    }
    Some((
        visible.left - outer.left,
        visible.top - outer.top,
        outer.right - visible.right,
        outer.bottom - visible.bottom,
    ))
}

pub fn hide(handle: isize) -> bool {
    let hwnd = handle as HWND;
    if hwnd.is_null() {
        return false;
    }
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
    }
    true
}

pub fn show_active(handle: isize) -> bool {
    let hwnd = handle as HWND;
    if hwnd.is_null() {
        return false;
    }
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        if GetForegroundWindow() == hwnd {
            return true;
        }
        let front = GetForegroundWindow();
        let front_thread = if front.is_null() {
            0
        } else {
            GetWindowThreadProcessId(front, std::ptr::null_mut())
        };
        let mine = GetCurrentThreadId();
        // Attaching to a hung window's input queue would hang this thread
        // (the tray's UI thread) with it: then the window just stays behind.
        let hung = !front.is_null() && IsHungAppWindow(front) != 0;
        let attached = !hung
            && front_thread != 0
            && front_thread != mine
            && AttachThreadInput(mine, front_thread, 1) != 0;
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        if attached {
            AttachThreadInput(mine, front_thread, 0);
        }
        GetForegroundWindow() == hwnd
    }
}

pub fn is_visible(handle: isize) -> bool {
    let hwnd = handle as HWND;
    !hwnd.is_null() && unsafe { IsWindowVisible(hwnd) } != 0
}

pub fn set_rect(handle: isize, x: i32, y: i32, width: i32, height: i32) -> bool {
    let hwnd = handle as HWND;
    if hwnd.is_null() {
        return false;
    }
    unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOZORDER,
        ) != 0
    }
}

pub fn create_owner() -> Option<isize> {
    // "STATIC" is a window class every process has; the title is empty.
    let class = super::wide("STATIC");
    let title = super::wide("");
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    (!hwnd.is_null()).then_some(hwnd as isize)
}

/// Windows' version, as tao reads it (`RtlGetVersion`, which does not lie
/// to an exe without a compatibility manifest): (major, minor, build).
fn windows_version() -> (u32, u32, u32) {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
    let mut info: OSVERSIONINFOW = unsafe { std::mem::zeroed() };
    info.dwOSVersionInfoSize = size_of::<OSVERSIONINFOW>() as u32;
    unsafe { RtlGetVersion(&mut info) };
    (info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber)
}

/// What tao (Tauri's window library) does at start, `allow_dark_mode_for_app(true)`:
/// on Windows 10 build 17763 or later, uxtheme's export 135 with "allow dark"
/// (`AllowDarkModeForApp(true)` before build 18362, `SetPreferredAppMode(AllowDark)`
/// from it), then export 104, `RefreshImmersiveColorPolicyState`.
pub fn allow_dark_menus() -> bool {
    use windows_sys::Win32::System::LibraryLoader::{
        GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
    };
    let (major, minor, build) = windows_version();
    if !(major == 10 && minor == 0 && build >= 17763) {
        return false;
    }
    type AllowDarkModeForApp = unsafe extern "system" fn(bool) -> bool;
    type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
    type Refresh = unsafe extern "system" fn();
    const ALLOW_DARK: i32 = 1;
    let name = super::wide("uxtheme.dll");
    unsafe {
        // From System32 only, never from the exe's folder or the path.
        let module = LoadLibraryExW(
            name.as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        );
        if module.is_null() {
            return false;
        }
        let Some(address) = GetProcAddress(module, 135usize as *const u8) else {
            return false;
        };
        if build < 18362 {
            let allow: AllowDarkModeForApp = std::mem::transmute(address);
            allow(true);
        } else {
            let set: SetPreferredAppMode = std::mem::transmute(address);
            set(ALLOW_DARK);
        }
        if let Some(address) = GetProcAddress(module, 104usize as *const u8) {
            let refresh: Refresh = std::mem::transmute(address);
            refresh();
        }
    }
    true
}

pub fn monitor_at(x: i32, y: i32) -> Option<crate::window::Monitor> {
    let rect = |r: RECT| (r.left, r.top, r.right - r.left, r.bottom - r.top);
    unsafe {
        let monitor = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return None;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let (mut dx, mut dy) = (0u32, 0u32);
        // Without the monitor's DPI, the system's (the primary monitor's at
        // sign-in), and the caller is told so.
        let from_monitor = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) == 0;
        if !from_monitor || dx == 0 {
            dx = GetDpiForSystem();
        }
        Some(crate::window::Monitor {
            bounds: rect(info.rcMonitor),
            work: rect(info.rcWork),
            scale: f64::from(dx.max(96)) / 96.0,
            scale_from_monitor: from_monitor && dy != 0,
        })
    }
}
