//! The tray icon for the overall level, in the art for the taskbar's colour:
//! the logo alone when healthy, and a dot on it otherwise: amber, red or grey.
//! The drawing is the Tauri app's (app/src-tauri/src/badge.rs), on plain RGBA
//! for `tray_icon::Icon::from_rgba`.
#![forbid(unsafe_code)]

use std::sync::OnceLock;

use n8tray_core::model::LevelName;
use n8tray_platform::theme::Taskbar;

/// The n8builds logo (white n, blue 8), for a dark taskbar (D30).
const DARK: &[u8] = include_bytes!("../../app/src-tauri/icons/tray.png");
/// The cyan and navy monogram, for a light taskbar.
const LIGHT: &[u8] = include_bytes!("../../app/src-tauri/icons/tray-light.png");

/// Every level and every taskbar; `slot` places each pair in the cache.
const LEVELS: [LevelName; 5] = [
    LevelName::Hidden,
    LevelName::Off,
    LevelName::Ok,
    LevelName::Attention,
    LevelName::Problem,
];
const TASKBARS: [Taskbar; 2] = [Taskbar::Dark, Taskbar::Light];

/// An image as 8-bit RGBA, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

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
/// A new taskbar or level fails to compile here until it has a place.
fn slot(taskbar: Taskbar, level: LevelName) -> usize {
    let t = match taskbar {
        Taskbar::Dark => 0,
        Taskbar::Light => 1,
    };
    let l = match level {
        LevelName::Hidden => 0,
        LevelName::Off => 1,
        LevelName::Ok => 2,
        LevelName::Attention => 3,
        LevelName::Problem => 4,
    };
    t * LEVELS.len() + l
}

/// The icon for a taskbar and a level, drawn once each.
pub fn icon(taskbar: Taskbar, level: LevelName) -> &'static Rgba {
    static ICONS: OnceLock<Vec<Rgba>> = OnceLock::new();
    let icons = ICONS.get_or_init(|| {
        let mut icons = vec![None; TASKBARS.len() * LEVELS.len()];
        for t in TASKBARS {
            for l in LEVELS {
                icons[slot(t, l)] = Some(draw(base(t), l));
            }
        }
        icons
            .into_iter()
            .map(|i| i.expect("every slot is drawn"))
            .collect()
    });
    &icons[slot(taskbar, level)]
}

/// The icon for `tray-icon`.
pub fn tray_icon(taskbar: Taskbar, level: LevelName) -> Option<tray_icon::Icon> {
    let image = icon(taskbar, level);
    tray_icon::Icon::from_rgba(image.pixels.clone(), image.width, image.height).ok()
}

/// Decodes a PNG into 8-bit RGBA, whatever its colour type.
pub fn decode(png_bytes: &[u8]) -> Result<Rgba, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let size = reader.output_buffer_size().ok_or("the PNG is too large")?;
    let mut buffer = vec![0; size];
    let frame = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    buffer.truncate(frame.buffer_size());
    let pixels = match frame.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 0xff])
            .collect(),
        png::ColorType::GrayscaleAlpha => buffer
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buffer.iter().flat_map(|&g| [g, g, g, 0xff]).collect(),
        png::ColorType::Indexed => return Err("an indexed PNG was not expanded".into()),
    };
    Ok(Rgba {
        width: frame.width,
        height: frame.height,
        pixels,
    })
}

fn draw(png_bytes: &[u8], level: LevelName) -> Rgba {
    let mut image = decode(png_bytes).expect("the tray icon is a valid PNG");
    // Healthy shows the logo alone: the dot appears only when something needs
    // a look (D30), so the mark is not half covered all day.
    if level == LevelName::Ok {
        return image;
    }
    let (w, h) = (image.width, image.height);
    let rgba = &mut image.pixels;
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
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_art_decodes_to_square_rgba() {
        for taskbar in TASKBARS {
            let image = decode(base(taskbar)).unwrap();
            assert_eq!(image.width, image.height, "{taskbar:?}");
            assert!(image.width >= 16);
            assert_eq!(
                image.pixels.len(),
                (image.width * image.height * 4) as usize
            );
            // Cut out: some pixels are transparent, some are not.
            let alphas: Vec<u8> = image.pixels.chunks_exact(4).map(|p| p[3]).collect();
            assert!(alphas.contains(&0) && alphas.iter().any(|&a| a > 200));
        }
    }

    #[test]
    fn healthy_is_the_logo_alone_and_trouble_has_a_dot() {
        for taskbar in TASKBARS {
            let plain = decode(base(taskbar)).unwrap();
            assert_eq!(icon(taskbar, LevelName::Ok), &plain, "{taskbar:?}");
            for level in [
                LevelName::Attention,
                LevelName::Problem,
                LevelName::Off,
                LevelName::Hidden,
            ] {
                let drawn = icon(taskbar, level);
                assert_eq!((drawn.width, drawn.height), (plain.width, plain.height));
                assert_ne!(drawn.pixels, plain.pixels, "{taskbar:?} {level:?}");
            }
        }
    }

    #[test]
    fn each_taskbar_has_its_own_art() {
        assert_ne!(
            icon(Taskbar::Dark, LevelName::Ok).pixels,
            icon(Taskbar::Light, LevelName::Ok).pixels
        );
        // The dot is the same on both: a level has one colour on any taskbar.
        let corner = |image: &Rgba| {
            let w = image.width;
            let i = (((image.height - 8) * w + (w - 8)) * 4) as usize;
            image.pixels[i..i + 4].to_vec()
        };
        let (dark, light) = (
            icon(Taskbar::Dark, LevelName::Problem),
            icon(Taskbar::Light, LevelName::Problem),
        );
        assert_eq!(corner(dark), corner(light));
        assert_eq!(&corner(dark)[..3], &color(LevelName::Problem)[..]);
        // Fully opaque at the dot's centre, a transparent ring around it.
        assert_eq!(corner(dark)[3], 0xff);
    }

    #[test]
    fn every_level_has_its_own_dot_colour() {
        let corner = |level| {
            let image = icon(Taskbar::Dark, level);
            let w = image.width;
            let i = (((image.height - 8) * w + (w - 8)) * 4) as usize;
            image.pixels[i..i + 3].to_vec()
        };
        assert_eq!(corner(LevelName::Attention), color(LevelName::Attention));
        assert_eq!(corner(LevelName::Off), color(LevelName::Off));
        assert_ne!(corner(LevelName::Attention), corner(LevelName::Problem));
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

    #[test]
    fn the_tray_icon_takes_every_drawing() {
        for taskbar in TASKBARS {
            for level in LEVELS {
                assert!(tray_icon(taskbar, level).is_some(), "{taskbar:?} {level:?}");
            }
        }
    }

    #[test]
    fn what_is_not_a_png_is_refused() {
        assert!(decode(b"not a png").is_err());
    }
}
