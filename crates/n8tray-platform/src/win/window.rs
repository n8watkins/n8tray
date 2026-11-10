//! A tool window with rounded corners (see `crate::window`).

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmGetWindowAttribute, DwmSetWindowAttribute,
};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetClientRect, GetWindowLongPtrW, GetWindowRect, SW_SHOWNOACTIVATE,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
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
