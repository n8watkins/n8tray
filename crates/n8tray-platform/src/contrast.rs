//! High Contrast: whether it is on, and the system colours it uses, so a
//! window can draw with them as a browser does under `forced-colors`.
//!
//! On Windows, `SystemParametersInfo(SPI_GETHIGHCONTRAST)` says whether it is
//! on and `GetSysColor` gives each colour; read them when a window is
//! created. Elsewhere there is no such mode: the answer is `None`.

/// A colour as red, green and blue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// From a Windows `COLORREF` (0x00BBGGRR).
    pub fn from_colorref(value: u32) -> Rgb {
        Rgb(
            (value & 0xff) as u8,
            ((value >> 8) & 0xff) as u8,
            ((value >> 16) & 0xff) as u8,
        )
    }
}

/// The system colours, by their CSS `forced-colors` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemColors {
    /// `Canvas`: the window background (COLOR_WINDOW).
    pub canvas: Rgb,
    /// `CanvasText` (COLOR_WINDOWTEXT).
    pub canvas_text: Rgb,
    /// `Highlight` (COLOR_HIGHLIGHT).
    pub highlight: Rgb,
    /// `HighlightText` (COLOR_HIGHLIGHTTEXT).
    pub highlight_text: Rgb,
    /// `ButtonFace` (COLOR_BTNFACE).
    pub button_face: Rgb,
    /// `ButtonText` (COLOR_BTNTEXT).
    pub button_text: Rgb,
    /// `GrayText`: disabled text (COLOR_GRAYTEXT).
    pub gray_text: Rgb,
    /// `LinkText` (COLOR_HOTLIGHT).
    pub link_text: Rgb,
}

/// The system colours while High Contrast is on; `None` when it is off,
/// cannot be read, or the platform has no such mode.
pub fn high_contrast() -> Option<SystemColors> {
    #[cfg(windows)]
    {
        crate::win::contrast::high_contrast()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colorref_is_blue_green_red() {
        assert_eq!(Rgb::from_colorref(0x0000_00ff), Rgb(255, 0, 0));
        assert_eq!(Rgb::from_colorref(0x00ff_0000), Rgb(0, 0, 255));
        assert_eq!(Rgb::from_colorref(0x0012_3456), Rgb(0x56, 0x34, 0x12));
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_there_is_no_high_contrast() {
        assert_eq!(high_contrast(), None);
    }

    /// Whatever the PC's setting is, reading it works and an answer that
    /// says it is on has a background different from its text.
    #[cfg(windows)]
    #[test]
    fn the_real_setting_is_read() {
        if let Some(colors) = high_contrast() {
            assert_ne!(colors.canvas, colors.canvas_text);
        }
    }
}
