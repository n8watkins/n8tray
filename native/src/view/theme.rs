//! Which palette a window draws with: the system's app theme (light or
//! dark), `N8TRAY_THEME` over it, and High Contrast's system colours, read
//! when the window is created. `apply!` sets them on a window's `Theme`
//! global (`ui/theme.slint`).
#![forbid(unsafe_code)]

use n8tray_platform::contrast::{Rgb, SystemColors};

/// The palette for one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    pub dark: bool,
    /// The system colours while High Contrast is on.
    pub contrast: Option<SystemColors>,
}

/// Windows 11's "Night sky" contrast theme: what `N8TRAY_THEME=contrast`
/// shows on a PC without High Contrast, and the gallery's.
pub const NIGHT_SKY: SystemColors = SystemColors {
    canvas: Rgb(0x00, 0x00, 0x00),
    canvas_text: Rgb(0xff, 0xff, 0xff),
    highlight: Rgb(0x8e, 0xe3, 0xf0),
    highlight_text: Rgb(0x26, 0x3b, 0x50),
    button_face: Rgb(0x00, 0x00, 0x00),
    button_text: Rgb(0xff, 0xff, 0xff),
    gray_text: Rgb(0xa6, 0xa6, 0xa6),
    link_text: Rgb(0xff, 0xff, 0x00),
};

/// `N8TRAY_THEME=light|dark` picks the app theme, `contrast` forces High
/// Contrast (Night sky unless it is really on); else the system's app
/// theme as the window reports it (dark when unknown) and its contrast.
pub fn choose(
    env: Option<&str>,
    system_dark: Option<bool>,
    contrast: Option<SystemColors>,
) -> Choice {
    match env {
        Some("light") => Choice {
            dark: false,
            contrast,
        },
        Some("dark") => Choice {
            dark: true,
            contrast,
        },
        Some("contrast") => Choice {
            dark: system_dark.unwrap_or(true),
            contrast: Some(contrast.unwrap_or(NIGHT_SKY)),
        },
        _ => Choice {
            dark: system_dark.unwrap_or(true),
            contrast,
        },
    }
}

/// The choice for a window now: the environment, the window's own theme and
/// High Contrast as the platform reads it.
pub fn for_window(window: &slint::Window) -> Choice {
    use slint::winit_030::WinitWindowAccessor;
    use slint::winit_030::winit::window::Theme;
    let system_dark = window
        .with_winit_window(|w| w.theme())
        .flatten()
        .map(|t| t == Theme::Dark);
    choose(
        std::env::var("N8TRAY_THEME").ok().as_deref(),
        system_dark,
        n8tray_platform::contrast::high_contrast(),
    )
}

/// A colour for Slint.
pub fn color(rgb: Rgb) -> slint::Color {
    slint::Color::from_rgb_u8(rgb.0, rgb.1, rgb.2)
}

/// Sets a choice on a window's `Theme` global: `apply!(ui.global::<Theme>(),
/// SystemColors, choice)`, with the `Theme` and `SystemColors` of the
/// window's own generated module.
macro_rules! apply {
    ($theme:expr, $colors:ty, $choice:expr) => {{
        let theme = $theme;
        let choice: $crate::view::theme::Choice = $choice;
        theme.set_dark(choice.dark);
        theme.set_contrast(choice.contrast.is_some());
        if let Some(c) = choice.contrast {
            use $crate::view::theme::color;
            let mut system = <$colors>::default();
            system.canvas = color(c.canvas);
            system.canvas_text = color(c.canvas_text);
            system.highlight = color(c.highlight);
            system.highlight_text = color(c.highlight_text);
            system.button_face = color(c.button_face);
            system.button_text = color(c.button_text);
            system.gray_text = color(c.gray_text);
            system.link_text = color(c.link_text);
            theme.set_system(system);
        }
    }};
}
pub(crate) use apply;

#[cfg(test)]
mod tests {
    use super::*;

    const HC: SystemColors = SystemColors {
        canvas: Rgb(1, 1, 1),
        ..NIGHT_SKY
    };

    #[test]
    fn the_environment_wins_over_the_system_theme() {
        assert!(!choose(Some("light"), Some(true), None).dark);
        assert!(choose(Some("dark"), Some(false), None).dark);
        assert!(!choose(None, Some(false), None).dark);
        assert!(choose(None, Some(true), None).dark);
        assert!(choose(Some("other"), None, None).dark, "dark when unknown");
    }

    #[test]
    fn high_contrast_follows_the_system_or_is_forced() {
        assert_eq!(choose(None, Some(false), Some(HC)).contrast, Some(HC));
        assert_eq!(choose(Some("light"), None, Some(HC)).contrast, Some(HC));
        assert_eq!(choose(None, None, None).contrast, None);
        assert_eq!(
            choose(Some("contrast"), Some(false), None).contrast,
            Some(NIGHT_SKY)
        );
        assert_eq!(
            choose(Some("contrast"), Some(false), Some(HC)).contrast,
            Some(HC)
        );
    }

    #[test]
    fn colours_reach_slint_unchanged() {
        assert_eq!(
            color(Rgb(0x8e, 0xe3, 0xf0)),
            slint::Color::from_rgb_u8(0x8e, 0xe3, 0xf0)
        );
    }
}
