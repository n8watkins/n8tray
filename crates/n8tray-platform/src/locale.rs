//! The user's clock: 12 or 24 hours, and the AM and PM markers, so times
//! read as the browser's `toLocaleTimeString` writes them for the user's
//! settings ("02:05 PM" on an American clock, "14:05" on most others).
//!
//! On Windows, the user's time format (`LOCALE_STIMEFORMAT`: `h` for a
//! 12-hour clock, `H` for 24) and markers (`LOCALE_S1159`, `LOCALE_S2359`).
//! Elsewhere, a 24-hour clock.

/// How the user's clock writes a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clock {
    pub twelve_hour: bool,
    pub am: String,
    pub pm: String,
    /// The marker comes before the time ("tt hh:mm").
    pub marker_first: bool,
}

impl Clock {
    pub fn twenty_four() -> Clock {
        Clock {
            twelve_hour: false,
            am: String::new(),
            pm: String::new(),
            marker_first: false,
        }
    }

    /// From a Windows time format such as `h:mm:ss tt` or `HH:mm:ss`.
    pub fn from_format(format: &str, am: &str, pm: &str) -> Clock {
        // Quoted text is literal: drop it before looking for `h` and `t`.
        let mut plain = String::new();
        let mut quoted = false;
        for c in format.chars() {
            if c == '\'' {
                quoted = !quoted;
            } else if !quoted {
                plain.push(c);
            }
        }
        let twelve_hour = plain.contains('h');
        let marker_first = match (plain.find('t'), plain.find(['h', 'H'])) {
            (Some(t), Some(h)) => t < h,
            _ => false,
        };
        Clock {
            twelve_hour,
            am: am.into(),
            pm: pm.into(),
            marker_first,
        }
    }
}

/// The user's clock now.
pub fn clock() -> Clock {
    #[cfg(windows)]
    {
        crate::win::locale::clock().unwrap_or_else(Clock::twenty_four)
    }
    #[cfg(not(windows))]
    {
        Clock::twenty_four()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_time_formats_are_read() {
        let us = Clock::from_format("h:mm:ss tt", "AM", "PM");
        assert!(us.twelve_hour && !us.marker_first);
        assert_eq!(us.pm, "PM");
        let gb = Clock::from_format("HH:mm:ss", "", "");
        assert!(!gb.twelve_hour);
        let zh = Clock::from_format("tt h:mm:ss", "上午", "下午");
        assert!(zh.twelve_hour && zh.marker_first);
        let quoted = Clock::from_format("HH'h'mm", "", "");
        assert!(!quoted.twelve_hour, "quoted letters are literal");
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_the_clock_has_24_hours() {
        assert_eq!(clock(), Clock::twenty_four());
    }

    /// Whatever the PC's settings are, a 12-hour clock has its markers.
    #[cfg(windows)]
    #[test]
    fn the_users_clock_is_read() {
        let c = clock();
        if c.twelve_hour {
            assert!(!c.am.is_empty() || !c.pm.is_empty(), "{c:?}");
        }
    }
}
