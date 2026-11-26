//! The widget gallery (`ui/gallery.slint`), only in debug and test builds:
//!
//! - `n8tray-native --gallery` opens it in a window, to look at every widget
//!   in every state (its Theme links switch light, dark and High Contrast);
//! - `n8tray-native --gallery-png <folder> [--scale <n>]` renders each
//!   widget family and the whole gallery, in each theme, to PNG files with
//!   the software renderer and no window at all;
//! - its tests compare the whole gallery with the golden images in
//!   `native/tests/goldens`.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, ModelRc, PlatformError, Rgb8Pixel, SharedString, VecModel};

use super::format;
use super::theme::{self, Choice, NIGHT_SKY};

/// Slint's generated code for `ui/gallery.slint`.
#[allow(clippy::all, unsafe_code)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/gallery.rs"));
}

use generated::{Gallery, SystemColors, Theme};

/// The widget families, as the gallery's `only` names them.
pub const FAMILIES: &[&str] = &[
    "title", "rows", "compact", "more", "status", "buttons", "switches", "banners", "sections",
    "scroll", "tooltip",
];

/// A theme to render in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Light,
    Dark,
    Contrast,
}

impl Look {
    pub const ALL: [Look; 3] = [Look::Light, Look::Dark, Look::Contrast];

    pub fn name(self) -> &'static str {
        match self {
            Look::Light => "light",
            Look::Dark => "dark",
            Look::Contrast => "contrast",
        }
    }

    fn choice(self) -> Choice {
        match self {
            Look::Light => Choice {
                dark: false,
                contrast: None,
            },
            Look::Dark => Choice {
                dark: true,
                contrast: None,
            },
            Look::Contrast => Choice {
                dark: true,
                contrast: Some(NIGHT_SKY),
            },
        }
    }
}

/// The font a render uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Font {
    /// Segoe UI Variable Text, as the app (the system's font).
    System,
    /// Inter, bundled for the golden images (`native/tests/fonts`), so they
    /// do not depend on the fonts of the machine that runs the tests.
    Test,
}

/// The snapshot the gallery's sparklines and chart are drawn from.
const BUSY: &str = include_str!("../../fixtures/busy.json");

/// Fills the gallery's drawings from the busy fixture: VoiceDock's, Lead
/// Scraper's and N8 Hotkeys' sparklines, N8 Tray's own, and VoiceDock's chart.
fn fill(ui: &Gallery) {
    let snapshot: n8tray_core::model::HostSnapshot =
        serde_json::from_str(BUSY).expect("the busy fixture reads");
    let history = |id: &str| {
        snapshot
            .apps
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.history.clone())
            .map(|h| (h.memory_mb, h.cpu_percent))
            .unwrap_or_default()
    };
    let tray = snapshot
        .tray
        .history
        .as_ref()
        .map(|h| h.memory_mb.clone())
        .unwrap_or_default();
    let sparks: Vec<format::Drawing> = ["voicedock", "leadscraper", "n8hotkeys"]
        .iter()
        .map(|id| format::spark(&history(id).0))
        .chain(std::iter::once(format::spark(&tray)))
        .collect();
    let strings = |pick: fn(&format::Drawing) -> &String| -> ModelRc<SharedString> {
        ModelRc::from(Rc::new(VecModel::from(
            sparks
                .iter()
                .map(|d| SharedString::from(pick(d).as_str()))
                .collect::<Vec<_>>(),
        )))
    };
    ui.set_spark_lines(strings(|d| &d.line));
    ui.set_spark_fills(strings(|d| &d.fill));
    let (memory, cpu) = history("voicedock");
    let chart = format::chart(&memory, &cpu);
    ui.set_chart_memory_line(chart.memory.line.into());
    ui.set_chart_memory_fill(chart.memory.fill.into());
    ui.set_chart_cpu_line(chart.cpu.into());
}

fn set_font(ui: &Gallery, font: Font) {
    if font == Font::Test {
        let theme = ui.global::<Theme>();
        theme.set_font_family("Inter".into());
        // Inter's own line (ascent plus descent) is 1.21 times its size.
        theme.set_line_factor(1.4 / 1.21);
    }
}

/// `--gallery` or `--gallery-png <folder> [--scale <n>]`: runs it and says
/// the exit code; `None` for any other start.
pub fn from_args(args: &[String]) -> Option<i32> {
    let at = |flag: &str| args.iter().position(|a| a == flag);
    if at("--gallery").is_some() {
        return Some(match show() {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("The gallery could not open: {e}");
                1
            }
        });
    }
    let i = at("--gallery-png")?;
    let Some(folder) = args.get(i + 1) else {
        eprintln!("--gallery-png needs a folder");
        return Some(2);
    };
    let scale = at("--scale")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(2.0);
    Some(match write_pngs(Path::new(folder), scale) {
        Ok(count) => {
            println!("Wrote {count} pictures to {folder}");
            0
        }
        Err(e) => {
            eprintln!("The gallery could not be rendered: {e}");
            1
        }
    })
}

/// The gallery in a window of its own, in the system's theme.
fn show() -> Result<(), PlatformError> {
    let ui = Gallery::new()?;
    fill(&ui);
    let choice = theme::choose(
        std::env::var("N8TRAY_THEME").ok().as_deref(),
        None,
        n8tray_platform::contrast::high_contrast(),
    );
    theme::apply!(ui.global::<Theme>(), SystemColors, choice);
    ui.run()
}

/// A rendered picture, RGB, row after row.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl Picture {
    pub fn write_png(&self, path: &Path) -> Result<(), String> {
        let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(&self.rgb)
            .map_err(|e| e.to_string())
    }

    #[cfg(test)]
    pub fn read_png(path: &Path) -> Result<Picture, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let mut buffer = vec![0; reader.output_buffer_size().ok_or("too large")?];
        let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
        if info.color_type != png::ColorType::Rgb {
            return Err(format!(
                "{} is {:?}, not RGB",
                path.display(),
                info.color_type
            ));
        }
        buffer.truncate(info.buffer_size());
        Ok(Picture {
            width: info.width,
            height: info.height,
            rgb: buffer,
        })
    }
}

/// The software renderer with no window system, one window per component:
/// set once per thread (Slint's platform is per thread).
struct Headless {
    windows: Windows,
}

/// The windows the headless platform made, the latest last.
type Windows = Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>;

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.windows.borrow_mut().push(window.clone());
        Ok(window)
    }
}

thread_local! {
    static WINDOWS: RefCell<Option<Windows>> = const { RefCell::new(None) };
}

fn headless() -> Result<Windows, String> {
    WINDOWS.with(|cell| {
        if let Some(windows) = cell.borrow().as_ref() {
            return Ok(windows.clone());
        }
        let windows = Rc::new(RefCell::new(Vec::new()));
        slint::platform::set_platform(Box::new(Headless {
            windows: windows.clone(),
        }))
        .map_err(|e| format!("the headless platform could not be set: {e}"))?;
        *cell.borrow_mut() = Some(windows.clone());
        Ok(windows)
    })
}

/// Renders the gallery (or one family of it) at its full size with the
/// software renderer, without a window system.
pub fn render(look: Look, only: &str, scale: f32, font: Font) -> Result<Picture, String> {
    let windows = headless()?;
    let ui = Gallery::new().map_err(|e| e.to_string())?;
    let window = windows
        .borrow_mut()
        .pop()
        .ok_or("the gallery has no window")?;
    fill(&ui);
    set_font(&ui, font);
    theme::apply!(ui.global::<Theme>(), SystemColors, look.choice());
    ui.set_only(only.into());
    window.dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    // A first frame brings the conditional parts up to date, so the size
    // read next is the one family's, not the whole gallery's.
    window.set_size(slint::PhysicalSize::new(16, 16));
    ui.show().map_err(|e| e.to_string())?;
    let mut scratch = vec![Rgb8Pixel::new(0, 0, 0); 16 * 16];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut scratch, 16);
    });
    let width = (ui.get_content_width() * scale).ceil() as u32;
    let height = (ui.get_content_height() * scale).ceil() as u32;
    window.set_size(slint::PhysicalSize::new(width, height));
    ui.show().map_err(|e| e.to_string())?;
    slint::platform::update_timers_and_animations();
    let mut pixels = vec![Rgb8Pixel::new(0, 0, 0); (width * height) as usize];
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, width as usize);
    });
    ui.hide().map_err(|e| e.to_string())?;
    let rgb = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    Ok(Picture { width, height, rgb })
}

/// Every family and the whole gallery in every theme, as
/// `<family>-<theme>.png` and `gallery-<theme>.png`.
pub fn write_pngs(folder: &Path, scale: f32) -> Result<usize, String> {
    std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let mut count = 0;
    for look in Look::ALL {
        for only in std::iter::once("").chain(FAMILIES.iter().copied()) {
            let picture = render(look, only, scale, Font::System)?;
            let name = if only.is_empty() { "gallery" } else { only };
            picture.write_png(&folder.join(format!("{name}-{}.png", look.name())))?;
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    //! The golden images guard the layout of every widget (sizes, gaps,
    //! colours, which state shows what), not exact letter pixels: they are
    //! drawn with a font bundled for the tests (Inter, not Segoe UI) by the
    //! software renderer, and compared with a small tolerance, so a change of
    //! anti-aliasing between machines passes and a moved or recoloured part
    //! fails. Refresh them with `N8TRAY_UPDATE_GOLDENS=1 cargo test`; a failed
    //! comparison writes the picture and a difference image to
    //! `target/golden-diffs` (CI uploads them).
    use std::path::PathBuf;

    use super::*;

    /// A channel that differs by more than this counts as a different pixel.
    const CHANNEL_TOLERANCE: u8 = 48;
    /// The share of different pixels allowed.
    const PIXEL_SHARE: f64 = 0.002;

    fn goldens() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
    }

    fn diffs() -> PathBuf {
        std::env::var_os("N8TRAY_GOLDEN_DIFFS")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/golden-diffs"))
    }

    /// How many pixels differ beyond the tolerance, and a picture of them
    /// (red where they differ, the golden image faded elsewhere).
    fn compare(golden: &Picture, actual: &Picture) -> (usize, Picture) {
        let mut different = 0;
        let mut rgb = Vec::with_capacity(golden.rgb.len());
        for (g, a) in golden.rgb.chunks(3).zip(actual.rgb.chunks(3)) {
            let far = g
                .iter()
                .zip(a)
                .any(|(x, y)| x.abs_diff(*y) > CHANNEL_TOLERANCE);
            if far {
                different += 1;
                rgb.extend([255, 0, 0]);
            } else {
                rgb.extend(g.iter().map(|v| 160 + v / 3));
            }
        }
        (
            different,
            Picture {
                width: golden.width,
                height: golden.height,
                rgb,
            },
        )
    }

    /// One test for every look: Slint's platform is per thread, and a test
    /// thread renders each look in turn.
    #[test]
    fn the_gallery_matches_its_golden_images() {
        let update = std::env::var_os("N8TRAY_UPDATE_GOLDENS").is_some();
        let mut failures = Vec::new();
        for look in Look::ALL {
            let name = format!("gallery-{}.png", look.name());
            let actual = render(look, "", 1.0, Font::Test).unwrap();
            let path = goldens().join(&name);
            if update {
                actual.write_png(&path).unwrap();
                continue;
            }
            let golden = Picture::read_png(&path).unwrap_or_else(|e| {
                panic!("{e}: run `N8TRAY_UPDATE_GOLDENS=1 cargo test` to make it")
            });
            let save = |suffix: &str, picture: &Picture| {
                std::fs::create_dir_all(diffs()).unwrap();
                picture
                    .write_png(&diffs().join(name.replace(".png", suffix)))
                    .unwrap();
            };
            if (golden.width, golden.height) != (actual.width, actual.height) {
                save("-actual.png", &actual);
                failures.push(format!(
                    "{name}: {}x{} now, {}x{} in the golden image",
                    actual.width, actual.height, golden.width, golden.height
                ));
                continue;
            }
            let (different, diff) = compare(&golden, &actual);
            let allowed = (f64::from(golden.width * golden.height) * PIXEL_SHARE) as usize;
            if different > allowed {
                save("-actual.png", &actual);
                save("-diff.png", &diff);
                failures.push(format!(
                    "{name}: {different} pixels differ (at most {allowed} may)"
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{}\nThe pictures are in {}; if the change is meant, refresh with N8TRAY_UPDATE_GOLDENS=1",
            failures.join("\n"),
            diffs().display()
        );
    }

    #[test]
    fn each_family_renders_on_its_own() {
        // In the same thread as no other render: a test thread of its own.
        for only in FAMILIES {
            let picture = render(Look::Light, only, 1.0, Font::Test).unwrap();
            // One card the window's width, in the gallery's 16 px margins.
            assert_eq!(picture.width, 380 + 32, "{only}");
            assert!(picture.height > 60, "{only}: {}", picture.height);
        }
    }

    #[test]
    fn the_flags_are_read() {
        assert_eq!(from_args(&["--show".into()]), None);
        assert_eq!(from_args(&["--gallery-png".into()]), Some(2));
    }
}
