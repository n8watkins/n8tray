//! The widget gallery (`ui/gallery.slint`), only in debug and test builds:
//!
//! - `n8tray-native --gallery` opens it in a window, to look at every widget
//!   in every state (its Theme links switch light, dark and High Contrast);
//! - `n8tray-native --gallery-png <folder> [--scale <n>]` renders each
//!   family (widgets, and each view on a fixture) in each theme to PNG
//!   files, with the software renderer and no window at all;
//! - its tests compare each family with its golden images in
//!   `native/tests/goldens/<family>/<theme>.png`.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

use n8tray_core::model::HostSnapshot;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, ModelRc, PlatformError, Rgb8Pixel, SharedString, VecModel};

use super::confirm::Confirms;
use super::format;
use super::icons::Icons;
use super::router::{self, Ctx, Models};
use super::theme::{self, Choice, NIGHT_SKY};
use crate::ui::{Gallery, RowData, View};

/// The widget families and the views, as the gallery's `only` names them.
/// Each has its own folder of golden images.
pub const FAMILIES: &[&str] = &[
    "title",
    "rows",
    "compact",
    "more",
    "status",
    "buttons",
    "switches",
    "banners",
    "sections",
    "blocks",
    "table",
    "scroll",
    "tooltip",
    "view-hub",
    "view-all-apps",
    "view-panel",
    "view-details",
    "view-settings",
    "view-about",
    "view-history",
];

/// The views' pages, with the view each shows.
const VIEWS: &[(&str, View)] = &[
    ("view-hub", View::Hub),
    ("view-all-apps", View::AllApps),
    ("view-panel", View::Panel),
    ("view-details", View::Details),
    ("view-settings", View::Settings),
    ("view-about", View::About),
    ("view-history", View::History),
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

    fn choice(self, font: &'static str) -> Choice {
        Choice {
            dark: self != Look::Light,
            contrast: (self == Look::Contrast).then_some(NIGHT_SKY),
            font,
        }
    }
}

/// The font a render uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Font {
    /// The app's (Segoe UI Variable Text, or Segoe UI).
    System,
    /// Inter, bundled for the golden images (`native/tests/fonts`), so they
    /// do not depend on the fonts of the machine that runs the tests.
    Test,
}

/// The snapshot the gallery's drawings and views come from.
const BUSY: &str = include_str!("../../fixtures/busy.json");

fn busy() -> HostSnapshot {
    serde_json::from_str(BUSY).expect("the busy fixture reads")
}

/// Fills the gallery's drawings from the busy fixture (VoiceDock's, Lead
/// Scraper's and N8 Hotkeys' sparklines, N8 Tray's own, VoiceDock's chart)
/// and each view's page through the view's own `fill`.
fn fill(ui: &Gallery) {
    let snapshot = busy();
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

    let confirms = Confirms::default();
    let mut icons = Icons::default();
    for (_, view) in VIEWS {
        let mut ctx = Ctx {
            snapshot: &snapshot,
            app: Some("voicedock"),
            confirms: &confirms,
            icons: &mut icons,
            now: Instant::now(),
        };
        router::fill_view(*view, ui, &mut ctx);
    }
}

fn set_font(ui: &Gallery, font: Font) {
    if font == Font::Test {
        let theme = ui.theme();
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
    theme::apply(&ui.theme(), &choice);
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
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
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

/// The windows the headless platform made, the latest last.
type Windows = Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>;

/// The software renderer with no window system, one window per component:
/// set once per thread (Slint's platform is per thread).
struct Headless {
    windows: Windows,
}

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

/// A gallery on the headless platform, with its window.
pub struct Headed {
    pub ui: Gallery,
    pub window: Rc<MinimalSoftwareWindow>,
}

/// A new gallery on the headless platform, filled and themed.
pub fn headed(look: Look, only: &str, font: Font) -> Result<Headed, String> {
    let windows = headless()?;
    #[cfg(test)]
    tests::register_test_font();
    let ui = Gallery::new().map_err(|e| e.to_string())?;
    let window = windows
        .borrow_mut()
        .pop()
        .ok_or("the gallery has no window")?;
    fill(&ui);
    set_font(&ui, font);
    let family = if font == Font::Test {
        "Inter"
    } else {
        theme::font()
    };
    theme::apply(&ui.theme(), &look.choice(family));
    if font == Font::Test {
        set_font(&ui, font);
    }
    ui.set_only(only.into());
    Ok(Headed { ui, window })
}

/// Renders a gallery at its full size.
pub fn draw(headed: &Headed, scale: f32) -> Result<Picture, String> {
    let (ui, window) = (&headed.ui, &headed.window);
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
    slint::platform::update_timers_and_animations();
    let mut pixels = vec![Rgb8Pixel::new(0, 0, 0); (width * height) as usize];
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, width as usize);
    });
    let rgb = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    Ok(Picture { width, height, rgb })
}

/// Renders the gallery (or one family of it) without a window system.
pub fn render(look: Look, only: &str, scale: f32, font: Font) -> Result<Picture, String> {
    let headed = headed(look, only, font)?;
    let picture = draw(&headed, scale)?;
    headed.ui.hide().map_err(|e| e.to_string())?;
    Ok(picture)
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

/// Rows for the "list" page (the focus test).
pub fn set_live_rows(ui: &Gallery, model: &Rc<VecModel<RowData>>) {
    ui.set_live_rows(ModelRc::from(model.clone()));
}

#[cfg(test)]
pub mod tests {
    //! The golden images guard the layout of every widget and view (sizes,
    //! gaps, colours, which state shows what), not exact letter pixels: they
    //! are drawn with a font bundled for the tests (Inter, not Segoe UI) by
    //! the software renderer, and compared with a small tolerance (8 per
    //! channel), so anti-aliasing that differs between machines passes and a
    //! moved, resized or recoloured part fails. Each family has its own
    //! folder, `native/tests/goldens/<family>/<theme>.png`; refresh them with
    //! `N8TRAY_UPDATE_GOLDENS=1 cargo test` (or `N8TRAY_UPDATE_GOLDENS=view-hub`
    //! for one family). A failed comparison writes the picture and a
    //! difference image to `target/golden-diffs` (CI uploads them).
    use std::path::PathBuf;

    use slint::Model;

    use super::*;
    use crate::ui::RowFocus;
    use crate::view::confirm;
    use crate::view::list;
    use crate::view::rows;

    #[allow(clippy::all, unsafe_code, dead_code)]
    mod test_font {
        include!(concat!(env!("OUT_DIR"), "/test_font.rs"));
    }

    thread_local! {
        static FONT: RefCell<Option<test_font::TestFont>> = const { RefCell::new(None) };
    }

    /// Creating the test-font component registers Inter (once per thread,
    /// on the headless platform).
    pub fn register_test_font() {
        FONT.with(|cell| {
            if cell.borrow().is_none() {
                let font = test_font::TestFont::new().expect("the test font registers");
                *cell.borrow_mut() = Some(font);
            }
        });
    }

    /// A channel that differs by more than this counts as a different pixel.
    const CHANNEL_TOLERANCE: u8 = 8;
    /// The share of different pixels allowed (letter edges).
    const PIXEL_SHARE: f64 = 0.001;

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

    /// Every family in every look against its golden images. One test:
    /// Slint's platform is per thread, and this thread renders them in turn.
    #[test]
    fn every_family_matches_its_golden_images() {
        let update = std::env::var("N8TRAY_UPDATE_GOLDENS").ok();
        let mut failures = Vec::new();
        for family in FAMILIES {
            for look in Look::ALL {
                let actual = render(look, family, 1.0, Font::Test).unwrap();
                let path = goldens().join(family).join(format!("{}.png", look.name()));
                if update.as_deref().is_some_and(|u| u == "1" || u == *family) {
                    actual.write_png(&path).unwrap();
                    continue;
                }
                let name = format!("{family}-{}", look.name());
                let golden = match Picture::read_png(&path) {
                    Ok(golden) => golden,
                    Err(e) => {
                        failures.push(format!("{name}: {e}"));
                        continue;
                    }
                };
                let save = |suffix: &str, picture: &Picture| {
                    picture
                        .write_png(&diffs().join(format!("{name}{suffix}.png")))
                        .unwrap();
                };
                if (golden.width, golden.height) != (actual.width, actual.height) {
                    save("-actual", &actual);
                    failures.push(format!(
                        "{name}: {}x{} now, {}x{} in the golden image",
                        actual.width, actual.height, golden.width, golden.height
                    ));
                    continue;
                }
                let (different, diff) = compare(&golden, &actual);
                let allowed = (f64::from(golden.width * golden.height) * PIXEL_SHARE) as usize;
                if different > allowed {
                    save("-actual", &actual);
                    save("-diff", &diff);
                    failures.push(format!(
                        "{name}: {different} pixels differ (at most {allowed} may)"
                    ));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{}\nThe pictures are in {}; if the change is meant, refresh with N8TRAY_UPDATE_GOLDENS=1 (or =<family>)",
            failures.join("\n"),
            diffs().display()
        );
    }

    /// The tolerance catches a recoloured part and a 2 px shift.
    #[test]
    fn the_comparison_catches_a_colour_and_a_small_shift() {
        let base = render(Look::Light, "status", 1.0, Font::Test).unwrap();
        let mut recoloured = Picture {
            width: base.width,
            height: base.height,
            rgb: base.rgb.clone(),
        };
        // Swap red and blue everywhere a pixel is not grey.
        for px in recoloured.rgb.chunks_mut(3) {
            px.swap(0, 2);
        }
        let allowed = (f64::from(base.width * base.height) * PIXEL_SHARE) as usize;
        assert!(compare(&base, &recoloured).0 > allowed);
        let row = (base.width * 3) as usize;
        let mut shifted = vec![0; base.rgb.len()];
        shifted[2 * row..].copy_from_slice(&base.rgb[..base.rgb.len() - 2 * row]);
        let shifted = Picture {
            width: base.width,
            height: base.height,
            rgb: shifted,
        };
        assert!(compare(&base, &shifted).0 > allowed);
    }

    #[test]
    fn each_family_renders_on_its_own() {
        for only in FAMILIES {
            let picture = render(Look::Light, only, 1.0, Font::Test).unwrap();
            // One card the window's width, in the gallery's 16 px margins.
            assert_eq!(picture.width, 380 + 32, "{only}");
            assert!(picture.height > 40, "{only}: {}", picture.height);
        }
    }

    /// The keyboard focus follows the app, not the position, when its list
    /// is reordered (`list::sync` and `rows::refocus`), and the armed state
    /// stays on its app.
    #[test]
    fn focus_and_the_armed_state_follow_the_app_through_a_reorder() {
        let headed = headed(Look::Light, "list", Font::Test).unwrap();
        let snapshot = busy();
        let mut apps: Vec<_> = snapshot
            .apps
            .iter()
            .filter(|a| format::row_action(a).is_some())
            .cloned()
            .collect();
        let mut confirms = Confirms::default();
        let now = Instant::now();
        let rows_of = |apps: &[n8tray_core::model::AppView], c: &Confirms| -> Vec<RowData> {
            apps.iter()
                .map(|a| rows::row_data(a, slint::Image::default(), c, false, now))
                .collect()
        };
        let model = Rc::new(VecModel::default());
        list::sync(&model, rows_of(&apps, &confirms), |r| r.id.clone());
        set_live_rows(&headed.ui, &model);
        draw(&headed, 1.0).unwrap();

        let a = apps[0].id.clone();
        let stop = format::row_action(&apps[0]).unwrap().id.clone();
        confirms.press(confirm::key(&a, &stop, None), true, now);
        let focus = headed.ui.global::<RowFocus>();
        focus.invoke_to_id(a.as_str().into(), 1);
        draw(&headed, 1.0).unwrap();
        assert_eq!(
            focus.get_focused_id(),
            a.as_str(),
            "the action of A has the focus"
        );
        assert_eq!(focus.get_focused_part(), 1);

        let before = rows::focused(&focus);
        apps.reverse();
        let synced = list::sync(&model, rows_of(&apps, &confirms), |r| r.id.clone());
        assert!(synced.moved);
        draw(&headed, 1.0).unwrap();
        rows::refocus(&focus, &before);
        draw(&headed, 1.0).unwrap();
        assert_eq!(
            focus.get_focused_id(),
            a.as_str(),
            "the focus is on A again"
        );
        assert_eq!(focus.get_focused_part(), 1);
        let armed: Vec<String> = model
            .iter()
            .filter(|r| r.armed)
            .map(|r| r.id.to_string())
            .collect();
        assert_eq!(armed, std::slice::from_ref(&a));

        // A refresh in the same order keeps both.
        let before = rows::focused(&focus);
        let synced = list::sync(&model, rows_of(&apps, &confirms), |r| r.id.clone());
        assert!(!synced.moved);
        rows::refocus(&focus, &before);
        draw(&headed, 1.0).unwrap();
        assert_eq!(focus.get_focused_id(), a.as_str());
        headed.ui.hide().unwrap();
    }

    /// Arrow keys move between rows and between a row's parts, as
    /// keyboard.ts does.
    #[test]
    fn arrow_keys_move_between_rows_and_parts() {
        use slint::platform::{Key, WindowEvent};
        let headed = headed(Look::Light, "list", Font::Test).unwrap();
        let snapshot = busy();
        let apps: Vec<_> = snapshot
            .apps
            .iter()
            .filter(|a| format::row_action(a).is_some())
            .cloned()
            .collect();
        let model = Rc::new(VecModel::default());
        let data: Vec<RowData> = apps
            .iter()
            .map(|a| {
                rows::row_data(
                    a,
                    slint::Image::default(),
                    &Confirms::default(),
                    false,
                    Instant::now(),
                )
            })
            .collect();
        list::sync(&model, data, |r| r.id.clone());
        set_live_rows(&headed.ui, &model);
        draw(&headed, 1.0).unwrap();
        let focus = headed.ui.global::<RowFocus>();
        focus.invoke_to_index(0, 0);
        draw(&headed, 1.0).unwrap();
        let key = |k: Key| {
            let text: SharedString = k.into();
            headed
                .window
                .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            headed
                .window
                .dispatch_event(WindowEvent::KeyReleased { text });
            draw(&headed, 1.0).unwrap();
        };
        assert_eq!(focus.get_focused_id(), apps[0].id.as_str());
        key(Key::DownArrow);
        assert_eq!(focus.get_focused_id(), apps[1].id.as_str());
        key(Key::RightArrow);
        assert_eq!(focus.get_focused_part(), 1, "to the row's action");
        key(Key::UpArrow);
        assert_eq!(focus.get_focused_id(), apps[0].id.as_str());
        assert_eq!(
            focus.get_focused_part(),
            1,
            "the same part of the row above"
        );
        key(Key::LeftArrow);
        assert_eq!(focus.get_focused_part(), 0);
        key(Key::End);
        assert_eq!(focus.get_focused_id(), apps[apps.len() - 1].id.as_str());
        key(Key::Home);
        assert_eq!(focus.get_focused_id(), apps[0].id.as_str());
        headed.ui.hide().unwrap();
    }

    #[test]
    fn the_flags_are_read() {
        assert_eq!(from_args(&["--show".into()]), None);
        assert_eq!(from_args(&["--gallery-png".into()]), Some(2));
    }
}
