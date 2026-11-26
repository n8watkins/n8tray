//! The user's time format and AM/PM markers (see `crate::locale`).

use windows_sys::Win32::Globalization::{
    GetLocaleInfoEx, LOCALE_S1159, LOCALE_S2359, LOCALE_STIMEFORMAT,
};

use crate::locale::Clock;

/// One locale string of the user's default locale.
fn info(kind: u32) -> Option<String> {
    let mut buffer = [0u16; 128];
    // SAFETY: a null locale name is the user's default locale; the buffer
    // and its length in characters are given, and only written during the
    // call.
    let len = unsafe {
        GetLocaleInfoEx(
            std::ptr::null(),
            kind,
            buffer.as_mut_ptr(),
            buffer.len() as i32,
        )
    };
    if len <= 0 {
        return None;
    }
    // The length counts the terminating zero.
    Some(String::from_utf16_lossy(
        &buffer[..(len as usize).saturating_sub(1)],
    ))
}

pub fn clock() -> Option<Clock> {
    let format = info(LOCALE_STIMEFORMAT)?;
    let am = info(LOCALE_S1159).unwrap_or_default();
    let pm = info(LOCALE_S2359).unwrap_or_default();
    Some(Clock::from_format(&format, &am, &pm))
}
