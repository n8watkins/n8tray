//! Which palette a window draws with: the system's app theme (light or
//! dark), `N8TRAY_THEME` over it, and High Contrast's system colours. The
//! window reads them again every time it is revealed, so a change of theme
//! or of High Contrast shows the next time it opens.
#![forbid(unsafe_code)]

use n8tray_platform::contrast::{Rgb, SystemColors};

use crate::ui::{SystemColors as UiColors, Theme};

/// The palette for one window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub dark: bool,
    /// The system colours while High Contrast is on.
    pub contrast: Option<SystemColors>,
    /// "Segoe UI Variable Text" where Windows has it (Windows 11), else
    /// "Segoe UI".
    pub font: &'static str,
}

/// Windows 11's "Night sky" contrast theme, exactly as
/// `C:\Windows\Resources\Ease of Access Themes\hc2.theme` has it: what
/// `N8TRAY_THEME=contrast` shows on a PC without High Contrast, and the
/// gallery's.
pub const NIGHT_SKY: SystemColors = SystemColors {
    canvas: Rgb(0, 0, 0),
    canvas_text: Rgb(255, 255, 255),
    highlight: Rgb(214, 180, 253),
    highlight_text: Rgb(43, 43, 43),
    button_face: Rgb(0, 0, 0),
    button_text: Rgb(255, 238, 50),
    gray_text: Rgb(166, 166, 166),
    link_text: Rgb(128, 128, 255),
};

/// The font stack of style.css: "Segoe UI Variable Text", then "Segoe UI".
pub const FONT: &str = "Segoe UI Variable Text";
pub const FONT_FALLBACK: &str = "Segoe UI";

/// `N8TRAY_THEME=light|dark` picks the app theme, `contrast` forces High
/// Contrast (Night sky unless it is really on); else the system's app
/// theme as the window reports it (dark when unknown) and its contrast.
pub fn choose(
    env: Option<&str>,
    system_dark: Option<bool>,
    contrast: Option<SystemColors>,
) -> Choice {
    let font = font();
    match env {
        Some("light") => Choice {
            dark: false,
            contrast,
            font,
        },
        Some("dark") => Choice {
            dark: true,
            contrast,
            font,
        },
        Some("contrast") => Choice {
            dark: system_dark.unwrap_or(true),
            contrast: Some(contrast.unwrap_or(NIGHT_SKY)),
            font,
        },
        _ => Choice {
            dark: system_dark.unwrap_or(true),
            contrast,
            font,
        },
    }
}

/// The first of the font stack this PC has. Slint takes one family, so the
/// stack is resolved here: Segoe UI Variable ships with Windows 11 only.
pub fn font() -> &'static str {
    let windows = std::env::var_os("WINDIR").map(std::path::PathBuf::from);
    match windows {
        Some(dir) if !dir.join("Fonts").join("SegUIVar.ttf").exists() => FONT_FALLBACK,
        _ => FONT,
    }
}

/// The choice for a window now: the environment, the window's own theme and
/// High Contrast as the platform reads it.
pub fn for_window(window: &slint::Window) -> Choice {
    use slint::winit_030::WinitWindowAccessor;
    use slint::winit_030::winit::window::Theme as WinitTheme;
    let system_dark = window
        .with_winit_window(|w| w.theme())
        .flatten()
        .map(|t| t == WinitTheme::Dark);
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

/// Sets a choice on a window's `Theme` global (the shell's window or the
/// gallery: `apply(&ui.global::<Theme>(), &choice)`).
pub fn apply(theme: &Theme<'_>, choice: &Choice) {
    theme.set_dark(choice.dark);
    theme.set_contrast(choice.contrast.is_some());
    theme.set_font_family(choice.font.into());
    if let Some(c) = choice.contrast {
        theme.set_system(UiColors {
            canvas: color(c.canvas),
            canvas_text: color(c.canvas_text),
            highlight: color(c.highlight),
            highlight_text: color(c.highlight_text),
            button_face: color(c.button_face),
            button_text: color(c.button_text),
            gray_text: color(c.gray_text),
            link_text: color(c.link_text),
        });
    }
}

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
    fn the_font_is_one_of_the_stack() {
        assert!([FONT, FONT_FALLBACK].contains(&font()));
    }

    #[test]
    fn colours_reach_slint_unchanged() {
        assert_eq!(
            color(Rgb(0xd6, 0xb4, 0xfd)),
            slint::Color::from_rgb_u8(0xd6, 0xb4, 0xfd)
        );
    }
}
