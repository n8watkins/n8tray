//! The tray icon with a dot for the overall level: green, amber, red or grey.

use std::sync::OnceLock;

use n8tray_core::model::LevelName;
use tauri::image::Image;

const BASE: &[u8] = include_bytes!("../icons/tray.png");

fn color(level: LevelName) -> [u8; 3] {
    match level {
        LevelName::Ok => [0x2e, 0x9e, 0x5b],
        LevelName::Attention => [0xe8, 0xa3, 0x17],
        LevelName::Problem => [0xd6, 0x45, 0x45],
        LevelName::Off | LevelName::Hidden => [0x9c, 0xa3, 0xaf],
    }
}

/// The icon for a level, drawn once per level.
pub fn icon(level: LevelName) -> Image<'static> {
    static ICONS: OnceLock<[Image<'static>; 5]> = OnceLock::new();
    let icons = ICONS.get_or_init(|| {
        [
            LevelName::Hidden,
            LevelName::Off,
            LevelName::Ok,
            LevelName::Attention,
            LevelName::Problem,
        ]
        .map(draw)
    });
    icons[level as usize].clone()
}

fn draw(level: LevelName) -> Image<'static> {
    let base = Image::from_bytes(BASE).expect("the tray icon is a valid PNG");
    let (w, h) = (base.width(), base.height());
    let mut rgba = base.rgba().to_vec();
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
