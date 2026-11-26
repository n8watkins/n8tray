//! High Contrast and the system colours (see `crate::contrast`).

use windows_sys::Win32::Graphics::Gdi::{
    COLOR_BTNFACE, COLOR_BTNTEXT, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT,
    COLOR_HOTLIGHT, COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor, SYS_COLOR_INDEX,
};
use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETHIGHCONTRAST, SystemParametersInfoW};

use crate::contrast::{Rgb, SystemColors};

pub fn high_contrast() -> Option<SystemColors> {
    let mut info = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        dwFlags: 0,
        lpszDefaultScheme: std::ptr::null_mut(),
    };
    // SAFETY: `info` is a HIGHCONTRASTW with its size set, as the call
    // requires; it is only written during the call.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            info.cbSize,
            (&mut info as *mut HIGHCONTRASTW).cast(),
            0,
        )
    };
    if ok == 0 || info.dwFlags & HCF_HIGHCONTRASTON == 0 {
        return None;
    }
    // SAFETY: GetSysColor takes any index and returns a COLORREF.
    let color = |index: SYS_COLOR_INDEX| Rgb::from_colorref(unsafe { GetSysColor(index) });
    Some(SystemColors {
        canvas: color(COLOR_WINDOW),
        canvas_text: color(COLOR_WINDOWTEXT),
        highlight: color(COLOR_HIGHLIGHT),
        highlight_text: color(COLOR_HIGHLIGHTTEXT),
        button_face: color(COLOR_BTNFACE),
        button_text: color(COLOR_BTNTEXT),
        gray_text: color(COLOR_GRAYTEXT),
        link_text: color(COLOR_HOTLIGHT),
    })
}
