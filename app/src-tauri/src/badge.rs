//! The tray icon for the overall level, in the art for the taskbar's colour:
//! the logo alone when healthy, and a dot on it otherwise: amber, red or grey.

use std::sync::OnceLock;

use n8tray_core::model::LevelName;
use n8tray_platform::theme::Taskbar;
use tauri::image::Image;

/// The n8builds logo (white n, blue 8), for a dark taskbar.
const DARK: &[u8] = include_bytes!("../icons/tray.png");
/// The cyan and navy monogram, for a light taskbar.
const LIGHT: &[u8] = include_bytes!("../icons/tray-light.png");

/// Every level, in the order of `LevelName`'s values.
const LEVELS: [LevelName; 5] = [
    LevelName::Hidden,
    LevelName::Off,
    LevelName::Ok,
    LevelName::Attention,
    LevelName::Problem,
];
const TASKBARS: [Taskbar; 2] = [Taskbar::Dark, Taskbar::Light];

fn base(taskbar: Taskbar) -> &'static [u8] {
    match taskbar {
        Taskbar::Dark => DARK,
        Taskbar::Light => LIGHT,
    }
}

fn color(level: LevelName) -> [u8; 3] {
    match level {
        LevelName::Ok => [0x2e, 0x9e, 0x5b],
        LevelName::Attention => [0xe8, 0xa3, 0x17],
        LevelName::Problem => [0xd6, 0x45, 0x45],
        LevelName::Off | LevelName::Hidden => [0x9c, 0xa3, 0xaf],
    }
}

/// Where the icon for a taskbar and a level sits in the cache.
fn slot(taskbar: Taskbar, level: LevelName) -> usize {
    let t = TASKBARS.iter().position(|t| *t == taskbar).unwrap();
    let l = LEVELS.iter().position(|l| *l == level).unwrap();
    t * LEVELS.len() + l
}

/// The icon for a taskbar and a level, drawn once each.
pub fn icon(taskbar: Taskbar, level: LevelName) -> Image<'static> {
    static ICONS: OnceLock<Vec<Image<'static>>> = OnceLock::new();
    let icons = ICONS.get_or_init(|| {
        TASKBARS
            .iter()
            .flat_map(|t| LEVELS.iter().map(move |l| draw(base(*t), *l)))
            .collect()
    });
    icons[slot(taskbar, level)].clone()
}

fn draw(png: &[u8], level: LevelName) -> Image<'static> {
    let base = Image::from_bytes(png).expect("the tray icon is a valid PNG");
    let (w, h) = (base.width(), base.height());
    let mut rgba = base.rgba().to_vec();
    // Healthy shows the logo alone: the dot appears only when something needs
    // a look (D30), so the mark is not half covered all day.
    if level == LevelName::Ok {
        return Image::new_owned(rgba, w, h);
    }
    // A dot in the bottom-right corner with a transparent ring around it,
    // so it reads on any taskbar color.
    let r = w as f32 * 0.24;
    let ring = w as f32 * 0.07;
    let (cx, cy) = (w as f32 - r - 1.0, h as f32 - r - 1.0);
    let [cr, cg, cb] = color(level);
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let i = ((y * w + x) * 4) as usize;
            if d <= r {
                let alpha = (r - d + 0.5).clamp(0.0, 1.0);
                rgba[i] = cr;
                rgba[i + 1] = cg;
                rgba[i + 2] = cb;
                rgba[i + 3] = (alpha * 255.0) as u8;
            } else if d <= r + ring {
                rgba[i + 3] = 0;
            }
        }
    }
    Image::new_owned(rgba, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels(image: &Image<'_>) -> Vec<u8> {
        image.rgba().to_vec()
    }

    #[test]
    fn healthy_is_the_logo_alone_and_trouble_has_a_dot() {
        for taskbar in TASKBARS {
            let plain = Image::from_bytes(base(taskbar)).unwrap();
            assert_eq!(
                pixels(&icon(taskbar, LevelName::Ok)),
                pixels(&plain),
                "{taskbar:?}"
            );
            for level in [
                LevelName::Attention,
                LevelName::Problem,
                LevelName::Off,
                LevelName::Hidden,
            ] {
                let drawn = icon(taskbar, level);
                assert_eq!(
                    (drawn.width(), drawn.height()),
                    (plain.width(), plain.height())
                );
                assert_ne!(pixels(&drawn), pixels(&plain), "{taskbar:?} {level:?}");
            }
        }
    }

    #[test]
    fn each_taskbar_has_its_own_art() {
        assert_ne!(
            pixels(&icon(Taskbar::Dark, LevelName::Ok)),
            pixels(&icon(Taskbar::Light, LevelName::Ok))
        );
        // The dot is the same on both: a level has one colour on any taskbar.
        let (dark, light) = (
            icon(Taskbar::Dark, LevelName::Problem),
            icon(Taskbar::Light, LevelName::Problem),
        );
        let corner = |image: &Image<'_>| {
            let w = image.width();
            let i = (((image.height() - 8) * w + (w - 8)) * 4) as usize;
            image.rgba()[i..i + 4].to_vec()
        };
        assert_eq!(corner(&dark), corner(&light));
        assert_eq!(&corner(&dark)[..3], &color(LevelName::Problem)[..]);
    }

    #[test]
    fn every_taskbar_and_level_has_a_place_of_its_own() {
        let mut seen = Vec::new();
        for taskbar in TASKBARS {
            for level in LEVELS {
                seen.push(slot(taskbar, level));
            }
        }
        seen.sort();
        assert_eq!(seen, (0..TASKBARS.len() * LEVELS.len()).collect::<Vec<_>>());
    }
}
