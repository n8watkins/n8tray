//! App icons for the views: an app's own icon (SVG or PNG, at most 256 KB,
//! as today's `app_icon`), read once per path while the window lives, and N8
//! Tray's own (the real N8 icon). An icon that cannot be read is an empty
//! image, which `AppIcon` draws as the generic glyph.
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

/// Today's limit on an icon file.
pub const MAX_BYTES: u64 = 256 * 1024;

const N8TRAY_PNG: &[u8] = include_bytes!("../../../app/src-tauri/icons/128x128.png");

#[derive(Default)]
pub struct Icons {
    cache: HashMap<PathBuf, Image>,
}

impl Icons {
    /// The icon at `path` (an app's `icon`), or an empty image.
    pub fn get(&mut self, path: Option<&str>) -> Image {
        let Some(path) = path else {
            return Image::default();
        };
        self.cache
            .entry(PathBuf::from(path))
            .or_insert_with_key(|p| load(p))
            .clone()
    }
}

fn load(path: &Path) -> Image {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let fits = std::fs::metadata(path).is_ok_and(|m| m.len() <= MAX_BYTES);
    if !fits || !matches!(ext.as_deref(), Some("svg" | "png")) {
        return Image::default();
    }
    Image::load_from_path(path).unwrap_or_default()
}

/// N8 Tray's own icon.
pub fn n8tray() -> Image {
    decode_png(N8TRAY_PNG).unwrap_or_default()
}

fn decode_png(bytes: &[u8]) -> Option<Image> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    if info.color_type != png::ColorType::Rgba {
        return None;
    }
    let pixels = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        &buffer[..info.buffer_size()],
        info.width,
        info.height,
    );
    Some(Image::from_rgba8(pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n8_trays_icon_decodes() {
        let icon = n8tray();
        assert_eq!((icon.size().width, icon.size().height), (128, 128));
    }

    #[test]
    fn a_missing_or_foreign_icon_is_empty() {
        let mut icons = Icons::default();
        assert_eq!(icons.get(None).size().width, 0);
        assert_eq!(icons.get(Some("/nowhere/icon.svg")).size().width, 0);
        assert_eq!(icons.get(Some("/etc/hostname")).size().width, 0);
        let svg = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../spec/examples/icons/voicedock.svg"
        );
        assert!(icons.get(Some(svg)).size().width > 0);
    }
}
