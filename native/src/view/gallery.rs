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
use super::router::{self, Ctx, Models, Router, Shot, Stage};
use super::theme::{self, Choice, NIGHT_SKY};
use super::{about, all_apps, details, history, hub, panel, settings};
use crate::ui::{FlyoutWindow, Gallery, Theme, View};

/// The widget families and the views, as the gallery's `only` names them.
/// Each has its own folder of golden images.
pub const FAMILIES: &[&str] = &[
    "title", "rows", "compact", "more", "status", "buttons", "switches", "banners", "sections",
    "blocks", "table", "scroll", "tooltip",
];

/// The views: each one's folder of golden images, the view, and the
/// pictures its module asks for (`SHOTS`, see `router::Shot`). The gallery
/// window also shows each view, on the busy fixture.
pub const VIEWS: &[(&str, View, &[Shot])] = &[
    ("view-hub", View::Hub, hub::SHOTS),
    ("view-all-apps", View::AllApps, all_apps::SHOTS),
    ("view-panel", View::Panel, panel::SHOTS),
    ("view-details", View::Details, details::SHOTS),
    ("view-settings", View::Settings, settings::SHOTS),
    ("view-about", View::About, about::SHOTS),
    ("view-history", View::History, history::SHOTS),
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
    for (_, view, _) in VIEWS {
        let mut ctx = Ctx {
            snapshot: &snapshot,
            app: Some("voicedock"),
            confirms: &confirms,
            icons: &mut icons,
            now: Instant::now(),
            arrived: true,
            warnings: &[],
        };
        router::fill_view(*view, ui, &mut ctx);
    }
}

fn set_font(theme: &Theme<'_>, font: Font) {
    if font == Font::Test {
        theme.set_font_family("Inter".into());
        // Inter's own line (ascent plus descent) is 1.21 times its size.
        theme.set_line_factor(1.4 / 1.21);
    }
}

/// Puts a look and a font on a window's theme.
fn set_look(theme: &Theme<'_>, look: Look, font: Font) {
    let family = if font == Font::Test {
        "Inter"
    } else {
        theme::font()
    };
    theme::apply(theme, &look.choice(family));
    set_font(theme, font);
}

/// A snapshot from `native/fixtures/<name>.json` (debug builds read it
/// from the source tree).
pub fn fixture(name: &str) -> Result<HostSnapshot, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The shell's window on the headless platform, showing `view` filled from
/// `shot`'s fixture by the router (so by the view's own `fill`), with the
/// shot's density, height limit and stage, in `look`. Times show in UTC on
/// a 24-hour clock (`format::fix_clock_for_pictures`), so the picture is
/// the same on every PC and runner.
pub struct ViewWindow {
    pub ui: FlyoutWindow,
    pub window: Rc<MinimalSoftwareWindow>,
    /// The router and the snapshot, for a view's test that changes either
    /// and fills again (`router.fill(&ui, &snapshot)`).
    #[allow(dead_code, reason = "for the views' tests")]
    pub router: Router,
    #[allow(dead_code, reason = "for the views' tests")]
    pub snapshot: HostSnapshot,
}

pub fn view_window(view: View, shot: &Shot, look: Look, font: Font) -> Result<ViewWindow, String> {
    let windows = headless()?;
    #[cfg(test)]
    tests::register_test_font();
    let ui = FlyoutWindow::new().map_err(|e| e.to_string())?;
    let window = windows
        .borrow_mut()
        .pop()
        .ok_or("the window has no window")?;
    let mut snapshot = fixture(shot.fixture)?;
    // The shot's density and height, whatever the fixture's settings say.
    snapshot.settings.compact = shot.compact;
    snapshot.settings.window_height_px = shot.height;
    snapshot.settings.window_height = match shot.height {
        500 => "medium",
        600 => "tall",
        _ => "short",
    }
    .into();
    let mut router = Router::default();
    router.open_at(view, shot.app);
    let mut warnings = Vec::new();
    if let Some(stage) = shot.stage {
        stage(&mut Stage {
            snapshot: &mut snapshot,
            warnings: &mut warnings,
            confirms: &mut router.confirms,
        });
    }
    router.warnings = Some(warnings);
    format::fix_clock_for_pictures();
    let theme = ui.global::<Theme>();
    set_look(&theme, look, font);
    theme::apply_settings(&theme, &snapshot.settings);
    router::wire(&ui);
    router.fill(&ui, &snapshot);
    Ok(ViewWindow {
        ui,
        window,
        router,
        snapshot,
    })
}

impl ViewWindow {
    /// Sizes the window as the shell does (its content, up to the height
    /// limit, at least 80 px) and draws it at 1x.
    #[cfg(test)]
    pub fn draw(&self) -> Result<Picture, String> {
        self.draw_at(1.0)
    }

    /// The same at a display scale.
    pub fn draw_at(&self, scale: f32) -> Result<Picture, String> {
        let (ui, window) = (&self.ui, &self.window);
        window.dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
            scale_factor: scale,
        });
        let width = (380.0 * scale).ceil() as u32;
        if !window.is_visible() {
            window.set_size(slint::PhysicalSize::new(width, 80));
            ui.show().map_err(|e| e.to_string())?;
        }
        // A first frame at the size it has brings the view up to date.
        let now = window.size();
        let mut scratch = vec![Rgb8Pixel::new(0, 0, 0); (now.width * now.height) as usize];
        window.draw_if_needed(|renderer| {
            renderer.render(&mut scratch, now.width as usize);
        });
        let limit = ui.global::<Theme>().get_height_limit();
        let logical = ui.get_content_height().clamp(80.0, limit.max(80.0));
        let height = (logical * scale).ceil() as u32;
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
}

/// One view's picture for a shot (its golden image).
#[cfg(test)]
pub fn render_view(view: View, shot: &Shot, look: Look, font: Font) -> Result<Picture, String> {
    let w = view_window(view, shot, look, font)?;
    let picture = w.draw()?;
    w.ui.hide().map_err(|e| e.to_string())?;
    Ok(picture)
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
pub type Windows = Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>;

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

/// The headless platform on this thread (set once), with the windows it made.
pub fn headless() -> Result<Windows, String> {
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
    set_look(&ui.theme(), look, font);
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

/// Every family, the whole gallery and every view's shots in every theme,
/// as `<family>-<theme>.png`, `gallery-<theme>.png` and
/// `view-<name>-<state>-<theme>.png`.
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
        for (name, view, shots) in VIEWS {
            for shot in *shots {
                let w = view_window(*view, shot, look, Font::System)?;
                let picture = w.draw_at(scale)?;
                w.ui.hide().map_err(|e| e.to_string())?;
                let file = format!("{name}-{}-{}.png", shot.state, look.name());
                picture.write_png(&folder.join(file))?;
                count += 1;
            }
        }
    }
    Ok(count)
}

/// Rows for the "list" page (the focus test).
#[cfg(test)]
pub fn set_live_rows(ui: &Gallery, model: &Rc<VecModel<crate::ui::RowData>>) {
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
    use crate::ui::{RowData, RowFocus};
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

    /// Compares one picture with its golden image at `path` (or writes it
    /// there when `N8TRAY_UPDATE_GOLDENS` names its folder or is 1); says
    /// what failed. `name` names the picture in the report and the diffs.
    fn check(name: &str, folder: &str, path: &Path, actual: &Picture) -> Option<String> {
        let update = std::env::var("N8TRAY_UPDATE_GOLDENS").ok();
        if update.as_deref().is_some_and(|u| u == "1" || u == folder) {
            actual.write_png(path).unwrap();
            return None;
        }
        let golden = match Picture::read_png(path) {
            Ok(golden) => golden,
            Err(e) => return Some(format!("{name}: {e}")),
        };
        let save = |suffix: &str, picture: &Picture| {
            picture
                .write_png(&diffs().join(format!("{name}{suffix}.png")))
                .unwrap();
        };
        if (golden.width, golden.height) != (actual.width, actual.height) {
            save("-actual", actual);
            return Some(format!(
                "{name}: {}x{} now, {}x{} in the golden image",
                actual.width, actual.height, golden.width, golden.height
            ));
        }
        let (different, diff) = compare(&golden, actual);
        let allowed = (f64::from(golden.width * golden.height) * PIXEL_SHARE) as usize;
        if different > allowed {
            save("-actual", actual);
            save("-diff", &diff);
            return Some(format!(
                "{name}: {different} pixels differ (at most {allowed} may)"
            ));
        }
        None
    }

    /// Every family and every view's shots in every look against their
    /// golden images. One test: Slint's platform is per thread, and this
    /// thread renders them in turn.
    #[test]
    fn every_family_and_view_matches_its_golden_images() {
        let mut failures = Vec::new();
        for family in FAMILIES {
            for look in Look::ALL {
                let actual = render(look, family, 1.0, Font::Test).unwrap();
                let path = goldens().join(family).join(format!("{}.png", look.name()));
                let name = format!("{family}-{}", look.name());
                failures.extend(check(&name, family, &path, &actual));
            }
        }
        for (folder, view, shots) in VIEWS {
            assert!(!shots.is_empty(), "{folder} has no shot");
            for shot in *shots {
                for look in Look::ALL {
                    let actual = render_view(*view, shot, look, Font::Test).unwrap();
                    let file = format!("{}-{}.png", shot.state, look.name());
                    let path = goldens().join(folder).join(&file);
                    let name = format!("{folder}-{}-{}", shot.state, look.name());
                    failures.extend(check(&name, folder, &path, &actual));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{}\nThe pictures are in {}; if the change is meant, refresh with N8TRAY_UPDATE_GOLDENS=1 (or =<folder>)",
            failures.join("\n"),
            diffs().display()
        );
    }

    /// Every file in a view's golden folder is one of its shots: a state
    /// that was renamed or dropped leaves no stale picture behind.
    #[test]
    fn each_view_folder_holds_only_its_shots() {
        for (folder, _, shots) in VIEWS {
            let dir = goldens().join(folder);
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let file = entry.file_name().to_string_lossy().to_string();
                let known = shots.iter().any(|s| {
                    Look::ALL
                        .iter()
                        .any(|l| file == format!("{}-{}.png", s.state, l.name()))
                });
                assert!(known, "{folder}/{file} is no shot of the view");
            }
        }
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

    /// Presses a key in a headless gallery and draws.
    fn press(headed: &Headed, k: slint::platform::Key) {
        use slint::platform::WindowEvent;
        let text: SharedString = k.into();
        headed
            .window
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        headed
            .window
            .dispatch_event(WindowEvent::KeyReleased { text });
        draw(headed, 1.0).unwrap();
    }

    /// What `tab_through` saw.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Stops {
        /// Controls in a bar (`in-bar`: they never scroll the list).
        pub in_bars: usize,
        /// Controls in the scroll area.
        pub in_list: usize,
        /// How far the list was scrolled at the bottom (0: it fits).
        pub bottom: f32,
    }

    /// Scrolls the list to the bottom with the mouse wheel at `wheel_at`,
    /// then Tabs through every focus stop until the first comes again, and
    /// checks each: a control in a bar never moves the list (it is outside
    /// the list's visible part, often), and a control in the list is
    /// scrolled into view.
    fn tab_core(
        models: &dyn Models,
        window: &MinimalSoftwareWindow,
        draw: &dyn Fn(),
        wheel_at: (f32, f32),
        at_top: &dyn Fn() -> bool,
    ) -> Stops {
        use slint::platform::{Key, WindowEvent};
        let reveal = models.reveal();
        window.dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(wheel_at.0, wheel_at.1),
            delta_x: 0.0,
            delta_y: -100_000.0,
        });
        draw();
        let bottom = reveal.get_scrolled();
        let tab = || {
            let text: SharedString = Key::Tab.into();
            window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            window.dispatch_event(WindowEvent::KeyReleased { text });
            draw();
        };
        let mut stops = Stops {
            in_bars: 0,
            in_list: 0,
            bottom,
        };
        let mut first = None;
        for _ in 0..200 {
            let before = reveal.get_scrolled();
            let serial = reveal.get_focused_serial();
            tab();
            if reveal.get_focused_serial() == serial {
                // No kit control took the focus. Only the shell window's top
                // (`focus-top`), where the round started, may: anything else
                // is a focusable thing that is not a kit control (a bare
                // FocusScope or TextInput), which reports nothing.
                assert!(
                    at_top(),
                    "Tab landed on something that is not a kit control (it does not report its focus)"
                );
                assert!(first.is_some(), "Tab found no control to focus");
                assert_eq!(
                    reveal.get_scrolled(),
                    before,
                    "the view's top moved the list"
                );
                return stops;
            }
            let spot = (
                reveal.get_focused_x(),
                reveal.get_focused_top(),
                reveal.get_focused_bottom(),
            );
            if reveal.get_focused_reveals() {
                stops.in_list += 1;
                if bottom > 0.0 {
                    assert_eq!(
                        reveal.get_revealed_serial(),
                        reveal.get_serial(),
                        "the list did not answer the control at {spot:?}"
                    );
                    let (top, end) = (reveal.get_revealed_top(), reveal.get_revealed_bottom());
                    assert!(
                        top >= reveal.get_view_top() - 0.5 && end <= reveal.get_view_bottom() + 0.5,
                        "the control at {spot:?} is at {top}..{end}, outside the list's {}..{}",
                        reveal.get_view_top(),
                        reveal.get_view_bottom()
                    );
                }
            } else {
                assert_eq!(
                    reveal.get_scrolled(),
                    before,
                    "the bar control at {spot:?} moved the list"
                );
                if first == Some(spot) {
                    return stops;
                }
                if first.is_none() {
                    first = Some(spot);
                }
                stops.in_bars += 1;
            }
            assert!(
                first.is_some(),
                "the first control after the view's top must be in its title bar"
            );
        }
        panic!("Tab did not come back to the first control in 200 presses");
    }

    /// For a view lane's own test: the view of `shot` in the shell's window,
    /// scrolled to the bottom, Tabbed through every focus stop from the
    /// view's top. Fails if a control outside the list (in a bar) moves the
    /// list, or one in the list is not brought into view; a control that is
    /// in neither (README: "every control is in a bar or in the
    /// ScrollArea") shows as the first of those failures. Say what you
    /// expect of the result, such as `assert!(stops.in_list >= 4)`.
    pub fn tab_through(view: View, shot: &Shot) -> Stops {
        let w = view_window(view, shot, Look::Light, Font::Test).unwrap();
        w.draw().unwrap();
        let height = w.window.size().height as f32;
        crate::window::arrived(&w.ui);
        let draw = || {
            w.draw().unwrap();
        };
        let at_top = || w.ui.get_top_focused();
        let stops = tab_core(&w.ui, &w.window, &draw, (190.0, height / 2.0), &at_top);
        w.ui.hide().unwrap();
        stops
    }

    /// The keyboard test itself, on a view's shape: a title bar, a list
    /// that scrolls, a bottom bar.
    #[test]
    fn tab_through_keeps_bars_out_of_the_list_and_brings_rows_into_view() {
        let headed = headed(Look::Light, "keys", Font::Test).unwrap();
        let snapshot = fixture("compactTall").unwrap();
        let model = Rc::new(VecModel::default());
        let data: Vec<RowData> = snapshot
            .apps
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
        assert!(data.len() >= 5, "enough rows to scroll");
        list::sync(&model, data, |r| r.id.clone());
        set_live_rows(&headed.ui, &model);
        draw(&headed, 1.0).unwrap();
        let redraw = || {
            draw(&headed, 1.0).unwrap();
        };
        // The gallery has no view top: every stop is a kit control.
        let stops = tab_core(&headed.ui, &headed.window, &redraw, (200.0, 160.0), &|| {
            false
        });
        assert!(stops.bottom > 0.0, "the list scrolled: {stops:?}");
        assert!(stops.in_list >= 5, "{stops:?}");
        assert!(stops.in_bars >= 3, "{stops:?}");
        headed.ui.hide().unwrap();
    }

    /// A shot's stage sets up a state no fixture has, in the shot's own
    /// copy: the snapshot, the warnings the view gets and the armed actions.
    #[test]
    fn a_shots_stage_changes_only_its_own_copy() {
        fn stage(s: &mut Stage) {
            s.snapshot.apps[0].name = "A very long name for a staged app".into();
            s.warnings.push(n8tray_core::model::WarningView {
                at: "18:02".into(),
                level: "warn".into(),
                message: "VoiceDock did not answer".into(),
                count: 3,
            });
            let app = s.snapshot.apps[0].id.clone();
            s.confirms
                .press(confirm::key(&app, "stop", None), true, Instant::now());
        }
        let shot = Shot {
            stage: Some(stage),
            ..Shot::of("staged", "busy")
        };
        let w = view_window(View::Hub, &shot, Look::Light, Font::Test).unwrap();
        assert_eq!(w.snapshot.apps[0].name, "A very long name for a staged app");
        assert_ne!(
            fixture("busy").unwrap().apps[0].name,
            w.snapshot.apps[0].name
        );
        assert_eq!(w.router.warnings.as_ref().map(Vec::len), Some(1));
        let key = confirm::key(&w.snapshot.apps[0].id, "stop", None);
        assert!(w.router.confirms.is_armed(&key, Instant::now()));
        w.ui.hide().unwrap();
    }

    /// The shell's window: the hub's bars never move its list.
    #[test]
    fn tab_through_the_hub() {
        // Lower than any preset, so the placeholder's lines scroll.
        let shot = Shot {
            compact: true,
            height: 200,
            ..Shot::of("compact-low", "compactTall")
        };
        let stops = tab_through(View::Hub, &shot);
        assert!(stops.bottom > 0.0, "the list scrolled: {stops:?}");
        assert!(stops.in_bars >= 2, "{stops:?}");
    }

    /// The keyboard focus follows the app, not the position, when its list
    /// is reordered (`list::sync` and `rows::refocus`), and the armed state
    /// stays on its app. As in the window, the sync and the refocus happen
    /// in the same turn, before any layout, and the focused row is one that
    /// `sync` moved: its widgets are new, created only at the next layout.
    #[test]
    fn focus_and_the_armed_state_follow_the_app_through_a_reorder() {
        use slint::platform::Key;
        let headed = headed(Look::Light, "list", Font::Test).unwrap();
        let snapshot = busy();
        let mut apps: Vec<_> = snapshot
            .apps
            .iter()
            .filter(|a| format::row_action(a).is_some())
            .cloned()
            .collect();
        assert!(apps.len() >= 3);
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

        // The last row, D: reversing the list moves it to the top, so `sync`
        // takes it out and puts it back (the others shift in place).
        let last = apps.len() - 1;
        let d = apps[last].id.clone();
        let stop = format::row_action(&apps[last]).unwrap().id.clone();
        confirms.press(confirm::key(&d, &stop, None), true, now);
        let focus = headed.ui.global::<RowFocus>();
        focus.invoke_to_id(d.as_str().into(), 1);
        draw(&headed, 1.0).unwrap();
        assert_eq!(
            focus.get_focused_id(),
            d.as_str(),
            "the action of D has the focus"
        );
        assert_eq!(focus.get_focused_part(), 1);

        let before = rows::focused(&focus);
        apps.reverse();
        let synced = list::sync(&model, rows_of(&apps, &confirms), |r| r.id.clone());
        assert!(synced.moved);
        rows::refocus(&focus, &before, &*model);
        draw(&headed, 1.0).unwrap();
        assert_eq!(
            focus.get_focused_id(),
            d.as_str(),
            "the focus is on D again, in its new place"
        );
        assert_eq!(focus.get_focused_part(), 1);
        assert!(!focus.get_pending(), "the new row took the request");
        let armed: Vec<String> = model
            .iter()
            .filter(|r| r.armed)
            .map(|r| r.id.to_string())
            .collect();
        assert_eq!(armed, std::slice::from_ref(&d));

        // The arrow keys go on from D's new place: the row under it is the
        // one that was above it.
        press(&headed, Key::DownArrow);
        assert_eq!(focus.get_focused_id(), apps[1].id.as_str());
        assert_eq!(focus.get_focused_part(), 1, "the same part");
        press(&headed, Key::UpArrow);
        assert_eq!(focus.get_focused_id(), d.as_str());

        // A refresh in the same order keeps both.
        let before = rows::focused(&focus);
        let synced = list::sync(&model, rows_of(&apps, &confirms), |r| r.id.clone());
        assert!(!synced.moved);
        rows::refocus(&focus, &before, &*model);
        draw(&headed, 1.0).unwrap();
        assert_eq!(focus.get_focused_id(), d.as_str());
        headed.ui.hide().unwrap();
    }

    /// A focus request never sticks: the focused row leaves the list (a
    /// sync removes it), the list is reordered, then the row comes back,
    /// and the focus does not jump to it. A request no row takes is dropped
    /// soon after (`RowFocusExpiry`), so a row added later takes nothing.
    #[test]
    fn a_focus_request_for_a_gone_row_never_steals_the_focus_later() {
        let headed = headed(Look::Light, "list", Font::Test).unwrap();
        let snapshot = busy();
        let all: Vec<_> = snapshot
            .apps
            .iter()
            .filter(|a| format::row_action(a).is_some())
            .cloned()
            .collect();
        assert!(all.len() >= 3);
        let rows_of = |apps: &[n8tray_core::model::AppView]| -> Vec<RowData> {
            apps.iter()
                .map(|a| {
                    rows::row_data(
                        a,
                        slint::Image::default(),
                        &Confirms::default(),
                        false,
                        Instant::now(),
                    )
                })
                .collect()
        };
        let model = Rc::new(VecModel::default());
        list::sync(&model, rows_of(&all), |r| r.id.clone());
        set_live_rows(&headed.ui, &model);
        draw(&headed, 1.0).unwrap();
        let focus = headed.ui.global::<RowFocus>();
        let reveal = headed.ui.global::<crate::ui::Reveal>();
        let gone = all[all.len() - 1].id.clone();
        focus.invoke_to_id(gone.as_str().into(), 0);
        draw(&headed, 1.0).unwrap();
        assert_eq!(focus.get_focused_id(), gone.as_str());

        // The focused row leaves: no request for it.
        let before = rows::focused(&focus);
        let mut rest: Vec<_> = all[..all.len() - 1].to_vec();
        list::sync(&model, rows_of(&rest), |r| r.id.clone());
        rows::refocus(&focus, &before, &*model);
        assert!(!focus.get_pending(), "no request for a row that left");
        draw(&headed, 1.0).unwrap();
        // Reordered: still nothing asked.
        let before = rows::focused(&focus);
        rest.reverse();
        list::sync(&model, rows_of(&rest), |r| r.id.clone());
        rows::refocus(&focus, &before, &*model);
        assert!(!focus.get_pending());
        draw(&headed, 1.0).unwrap();
        // The row comes back: it does not take the focus.
        let serial = reveal.get_focused_serial();
        list::sync(&model, rows_of(&all), |r| r.id.clone());
        draw(&headed, 1.0).unwrap();
        assert_eq!(
            reveal.get_focused_serial(),
            serial,
            "no control took the focus when the row came back"
        );

        // A request for a row that is not drawn yet and never is: dropped.
        list::sync(&model, rows_of(&rest), |r| r.id.clone());
        focus.invoke_to_id(gone.as_str().into(), 0);
        assert!(focus.get_pending());
        // A frame starts the expiry timer (as the window's next frame does).
        draw(&headed, 1.0).unwrap();
        assert!(focus.get_pending(), "still pending within the wait");
        std::thread::sleep(std::time::Duration::from_millis(300));
        slint::platform::update_timers_and_animations();
        draw(&headed, 1.0).unwrap();
        assert!(!focus.get_pending(), "the request expired");
        let serial = reveal.get_focused_serial();
        list::sync(&model, rows_of(&all), |r| r.id.clone());
        draw(&headed, 1.0).unwrap();
        assert_eq!(reveal.get_focused_serial(), serial, "nothing jumps later");
        headed.ui.hide().unwrap();
    }

    /// Arrow keys move between rows and between a row's parts, as
    /// keyboard.ts does.
    #[test]
    fn arrow_keys_move_between_rows_and_parts() {
        use slint::platform::Key;
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
        let key = |k: Key| press(&headed, k);
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
