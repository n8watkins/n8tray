//! The words and numbers the window shows, ported from today's window
//! (`app/ui/src/format.ts` and the pure helpers of `app/ui/src/model.ts`).
//! No decisions live here: which actions are possible, levels and texts come
//! from the host; this only picks and words them for one place on screen.
//!
//! JavaScript's `toFixed` and `Math.round` round a tie up, where Rust's
//! formatting rounds it to even: [`to_fixed`] and [`js_round`] keep today's
//! digits exactly, so "1.25 GB" reads "1.3 GB" in both windows.
#![forbid(unsafe_code)]
#![allow(
    dead_code,
    reason = "the kit for the screen lanes: each helper is used once its screen lands"
)]

use n8tray_core::model::{
    ActionKindView, ActionView, AppView, ComponentView, HostSnapshot, LevelName, UpdateView,
};
use n8tray_core::pins;

/// `value.toFixed(digits)` as JavaScript writes it: the exact decimal value,
/// rounded half up, and never "-0.0" for a negative zero.
pub fn to_fixed(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN".into()
        } else if value > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    // The exact decimal expansion, far past any digit asked for (an f64 has
    // at most 1074 digits after the point; 40 more than asked decides a tie).
    let negative = value < 0.0;
    let exact = format!("{:.*}", digits + 40, value.abs());
    let (int_part, frac_part) = exact.split_once('.').unwrap_or((&exact, ""));
    let mut kept: Vec<u8> = int_part
        .bytes()
        .chain(frac_part.bytes().take(digits))
        .map(|b| b - b'0')
        .collect();
    let rest = &frac_part.as_bytes()[digits.min(frac_part.len())..];
    // Half up: the first dropped digit is 5 or more.
    if rest.first().is_some_and(|&d| d >= b'5') {
        let mut i = kept.len();
        loop {
            if i == 0 {
                kept.insert(0, 1);
                break;
            }
            i -= 1;
            if kept[i] == 9 {
                kept[i] = 0;
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let int_len = kept.len() - digits;
    let mut out = String::new();
    // JavaScript keeps the sign of a negative value that rounds to zero
    // ((-0.04).toFixed(1) is "-0.0") but not of a negative zero.
    if negative {
        out.push('-');
    }
    for d in &kept[..int_len] {
        out.push((b'0' + d) as char);
    }
    if digits > 0 {
        out.push('.');
        for d in &kept[int_len..] {
            out.push((b'0' + d) as char);
        }
    }
    out
}

/// `Math.round`: a tie goes up (towards positive infinity).
pub fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// Memory as the window writes it: "643 MB", "1.2 GB" from 1000 MB on
/// (a GB is 1024 MB). `None` is the empty string.
pub fn memory(mb: Option<f64>) -> String {
    match mb {
        None => String::new(),
        Some(mb) if mb >= 1000.0 => format!("{} GB", to_fixed(mb / 1024.0, 1)),
        // `+ 0.0`: JavaScript writes a negative zero as "0".
        Some(mb) => format!("{} MB", js_round(mb) + 0.0),
    }
}

/// A share under 10 with one decimal ("0.1", "4" for 4.0), else whole.
fn share(value: f64) -> String {
    if value < 10.0 {
        let text = to_fixed(value, 1);
        text.strip_suffix(".0").map(String::from).unwrap_or(text)
    } else {
        format!("{}", js_round(value) + 0.0)
    }
}

/// CPU as a row's second figure: "0.1% CPU", "12% CPU".
pub fn cpu(percent: Option<f64>) -> String {
    match percent {
        None => String::new(),
        Some(p) => format!("{}% CPU", share(p)),
    }
}

/// A percentage without the word, for tables: "12%", or "-" when unknown.
pub fn percent(value: Option<f64>) -> String {
    match value {
        None => "-".into(),
        Some(v) => format!("{}%", share(v)),
    }
}

/// The title bar's summary next to the level pill: pinned apps that run of
/// those pinned, then N8's memory in all ("3/4 · 1.9 GB", the owner's
/// short form of 2026-11-27, so it fits beside "Needs attention"); with
/// nothing pinned, "0 pinned · 1.9 GB".
pub fn title_summary(pinned_running: u32, pinned_total: u32, n8_mb: f64) -> String {
    let n8 = memory(Some(n8_mb));
    if pinned_total == 0 {
        format!("0 pinned · {n8}")
    } else {
        format!("{pinned_running}/{pinned_total} · {n8}")
    }
}

/// A paragraph's words, split at its spaces, for a `Banner` whose link
/// flows after its last word (`words`).
pub fn words(text: &str) -> slint::ModelRc<slint::SharedString> {
    let words: Vec<slint::SharedString> = text.split_whitespace().map(Into::into).collect();
    slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(words)))
}

/// What a screen reader says for the summary: the words in full, as the
/// tray's status line has them ("3 of 4 running · N8 1.9 GB in all",
/// "Nothing pinned · N8 1.9 GB in all"), never "3 slash 4".
pub fn title_summary_label(s: &HostSnapshot) -> String {
    format!("{} · {}", s.pinned_text, s.memory.text)
}

/// The level pill's words.
pub fn level_words(level: LevelName) -> &'static str {
    match level {
        LevelName::Ok => "Healthy",
        LevelName::Attention => "Needs attention",
        LevelName::Problem => "Problem",
        LevelName::Hidden | LevelName::Off => "Idle",
    }
}

/// A level's name as the widgets take it (`Level` in `ui/theme.slint`).
pub fn level_name(level: LevelName) -> &'static str {
    match level {
        LevelName::Hidden => "hidden",
        LevelName::Off => "off",
        LevelName::Ok => "ok",
        LevelName::Attention => "attention",
        LevelName::Problem => "problem",
    }
}

/// How a line of text is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Attention,
    Problem,
}

/// Every app the hub knows (`pins::in_hub`: shown in N8 Tray and not
/// hidden), pinned or not, in the snapshot's order: what All apps lists.
pub fn hub_apps(apps: &[AppView]) -> Vec<&AppView> {
    apps.iter().filter(|a| pins::in_hub(a)).collect()
}

/// The apps the hub's main view shows (D32): those of `hub_apps` whose
/// `inMainView` is true (pinned, or pulled in by trouble). The rest sit
/// behind the snapshot's `more` line.
pub fn main_view_apps(apps: &[AppView]) -> Vec<&AppView> {
    apps.iter()
        .filter(|a| pins::in_hub(a) && a.in_main_view)
        .collect()
}

/// All apps' two groups (D32): "In the main view" and "Not in the main
/// view", each in the snapshot's order. A group with no app is left out.
pub fn all_apps_groups(apps: &[AppView]) -> Vec<(&'static str, Vec<&AppView>)> {
    let (inside, outside): (Vec<&AppView>, Vec<&AppView>) =
        hub_apps(apps).into_iter().partition(|a| a.in_main_view);
    [
        ("In the main view", inside),
        ("Not in the main view", outside),
    ]
    .into_iter()
    .filter(|(_, apps)| !apps.is_empty())
    .collect()
}

/// The row's one action: Stop while running, Start while not.
pub fn row_action(app: &AppView) -> Option<&ActionView> {
    let kind = if app.running {
        ActionKindView::Stop
    } else {
        ActionKindView::Start
    };
    app.actions.iter().find(|a| a.kind == kind)
}

/// Restart, offered while the app runs (the panel and the menu).
pub fn restart_action(app: &AppView) -> Option<&ActionView> {
    if !app.running {
        return None;
    }
    app.actions
        .iter()
        .find(|a| a.kind == ActionKindView::Restart)
}

/// The app's own actions that are not per component (the panel).
pub fn app_actions(app: &AppView) -> Vec<&ActionView> {
    app.actions
        .iter()
        .filter(|a| a.kind == ActionKindView::App && !a.per_component)
        .collect()
}

/// Per-component actions with the component's name in their label.
pub fn component_actions<'a>(
    app: &'a AppView,
    component: &ComponentView,
) -> Vec<(&'a ActionView, String)> {
    app.actions
        .iter()
        .filter(|a| a.per_component)
        .map(|a| (a, a.label.replacen("{component}", &component.name, 1)))
        .collect()
}

/// The row's second line: what happens now, a failed action, an alert, or
/// the summary.
pub fn row_line(app: &AppView) -> (String, Tone) {
    if let Some(pending) = &app.pending {
        return (pending.clone(), Tone::Normal);
    }
    if let Some(o) = &app.last_outcome
        && (o.outcome == "failed" || o.outcome == "refused")
    {
        let text = o
            .message
            .clone()
            .unwrap_or_else(|| format!("{} {}", o.action, o.outcome));
        return (text, Tone::Problem);
    }
    if let Some(alert) = &app.alert {
        let tone = if app.level == LevelName::Problem {
            Tone::Problem
        } else {
            Tone::Attention
        };
        return (alert.clone(), tone);
    }
    let tone = match app.level {
        LevelName::Problem => Tone::Problem,
        LevelName::Attention => Tone::Attention,
        _ => Tone::Normal,
    };
    (app.summary.clone(), tone)
}

/// The figures at the right of a row: memory, then CPU or where it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Figures {
    pub memory: String,
    /// "12% CPU", or "in WSL" when most of it is reported inside WSL.
    pub sub: String,
}

pub fn row_figures(app: &AppView) -> Option<Figures> {
    if app.memory_mb.is_none() && app.namespaced_mb.is_none() {
        return None;
    }
    let measured = app.memory_mb.unwrap_or(0.0);
    let reported = app.namespaced_mb.unwrap_or(0.0);
    if !app.running && measured + reported == 0.0 {
        return None;
    }
    Some(Figures {
        memory: memory(Some(measured + reported)),
        sub: if reported > measured {
            "in WSL".into()
        } else {
            cpu(app.cpu_percent)
        },
    })
}

/// Points of a sparkline or chart: values scaled into a `w` x `h` box, from
/// index `from` on (earlier steps keep their place on the time axis). The
/// numbers are today's `points()`, before they are written as text.
pub fn point_list(
    values: &[f64],
    w: f64,
    h: f64,
    max: Option<f64>,
    pad: f64,
    from: usize,
) -> Vec<(f64, f64)> {
    if values.is_empty() || from >= values.len() {
        return Vec::new();
    }
    let highest = max.unwrap_or_else(|| values.iter().copied().fold(f64::NEG_INFINITY, f64::max));
    let top = highest * 1.15;
    // `|| 1`: zero (or NaN) scales nothing.
    let top = if top == 0.0 || top.is_nan() { 1.0 } else { top };
    let step = if values.len() > 1 {
        w / (values.len() - 1) as f64
    } else {
        0.0
    };
    values[from..]
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let x = (i + from) as f64 * step;
            let y = h - 1.0 - (v.min(top) / top) * (h - pad - 1.0);
            (x, y)
        })
        .collect()
}

/// Today's `points()`: "x,y x,y" with one decimal.
pub fn points(values: &[f64], w: f64, h: f64, max: Option<f64>, pad: f64, from: usize) -> String {
    point_list(values, w, h, max, pad, from)
        .iter()
        .map(|&(x, y)| format!("{},{}", to_fixed(x + 0.0, 1), to_fixed(y + 0.0, 1)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Points as SVG path commands for a Slint `Path`: "M x y L x y ...", with
/// today's one-decimal numbers. `close` ends it with `Z` (a filled shape).
fn commands(points: &[(f64, f64)], close: bool) -> String {
    let mut out = String::new();
    for (i, &(x, y)) in points.iter().enumerate() {
        out.push_str(if i == 0 { "M " } else { " L " });
        out.push_str(&to_fixed(x + 0.0, 1));
        out.push(' ');
        out.push_str(&to_fixed(y + 0.0, 1));
    }
    if close && !out.is_empty() {
        out.push_str(" Z");
    }
    out
}

/// A drawing's outline and filled area, as `Path` commands (`Spark` and
/// `Chart` in ui/widgets/charts.slint). Empty strings draw nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drawing {
    pub line: String,
    pub fill: String,
}

/// A row's sparkline (44 x 20) from the latest run of a memory history.
/// Under a minute of data (6 steps of 10 s) draws nothing: a tick, not a
/// trend (Charts.tsx).
pub fn spark(values: &[f64]) -> Drawing {
    const W: f64 = 44.0;
    const H: f64 = 20.0;
    let from = run_start(values);
    if values.len() - from < 6 {
        return Drawing::default();
    }
    let line = point_list(values, W, H, None, 2.0, from);
    let x0 = from as f64 * W / (values.len() - 1) as f64;
    let mut area = vec![(x0, H)];
    area.extend(&line);
    area.push((W, H));
    Drawing {
        line: commands(&line, false),
        fill: commands(&area, true),
    }
}

/// A panel's chart (336 x 64): memory filled with its line, and the CPU
/// line against at least 10% of the machine (drawn dashed).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChartDrawing {
    pub memory: Drawing,
    pub cpu: String,
}

pub fn chart(memory_mb: &[f64], cpu_percent: &[f64]) -> ChartDrawing {
    const W: f64 = 336.0;
    const H: f64 = 64.0;
    let mem = point_list(memory_mb, W, H, None, 4.0, 0);
    let cpu_max = cpu_percent.iter().copied().fold(10.0, f64::max);
    let cpu = point_list(cpu_percent, W, H, Some(cpu_max), 4.0, 0);
    let mut area = Vec::new();
    if !mem.is_empty() {
        area.push((0.0, H));
        area.extend(&mem);
        area.push((W, H));
    }
    ChartDrawing {
        memory: Drawing {
            line: commands(&mem, false),
            fill: commands(&area, true),
        },
        cpu: commands(&cpu, false),
    }
}

/// Where the latest run starts: steps before it are zero because the app was
/// not running, and a line through them would read as a drop to nothing.
pub fn run_start(values: &[f64]) -> usize {
    let mut i = values.len();
    while i > 0 && values[i - 1] > 0.0 {
        i -= 1;
    }
    i
}

/// Whether a history has anything to draw.
pub fn has_history(values: Option<&[f64]>) -> bool {
    values.is_some_and(|v| v.iter().any(|&x| x > 0.0))
}

#[cfg(debug_assertions)]
thread_local! {
    /// This thread draws golden pictures (`fix_clock_for_pictures`).
    static PICTURE_CLOCK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Times on this thread show in UTC on a 24-hour clock from now on, the
/// same on every PC and runner whatever its time zone and clock setting:
/// the golden pictures' thread (`gallery::view_window`) calls it.
#[cfg(debug_assertions)]
pub fn fix_clock_for_pictures() {
    PICTURE_CLOCK.with(|fixed| fixed.set(true));
}

fn picture_clock() -> bool {
    #[cfg(debug_assertions)]
    {
        PICTURE_CLOCK.with(std::cell::Cell::get)
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

/// A start time as the local hour and minute ("14:05"), or "-".
pub fn since(started_at: Option<&str>) -> String {
    let Some(text) = started_at else {
        return "-".into();
    };
    match chrono::DateTime::parse_from_rfc3339(text) {
        Ok(at) => {
            use chrono::Timelike;
            if picture_clock() {
                let utc = at.with_timezone(&chrono::Utc);
                return clock_time(
                    utc.hour(),
                    utc.minute(),
                    &n8tray_platform::locale::Clock::twenty_four(),
                );
            }
            let local = at.with_timezone(&chrono::Local);
            clock_time(
                local.hour(),
                local.minute(),
                &n8tray_platform::locale::clock(),
            )
        }
        Err(_) => "-".into(),
    }
}

/// An hour and minute as today's `toLocaleTimeString([], { hour:
/// "2-digit", minute: "2-digit" })` writes them for the user's clock:
/// "14:05", or "02:05 PM" on a 12-hour clock (the marker where the user's
/// time format puts it).
pub fn clock_time(hour: u32, minute: u32, clock: &n8tray_platform::locale::Clock) -> String {
    if !clock.twelve_hour {
        return format!("{hour:02}:{minute:02}");
    }
    let marker = if hour < 12 { &clock.am } else { &clock.pm };
    let h12 = match hour % 12 {
        0 => 12,
        h => h,
    };
    let time = format!("{h12:02}:{minute:02}");
    if marker.is_empty() {
        time
    } else if clock.marker_first {
        format!("{marker} {time}")
    } else {
        format!("{time} {marker}")
    }
}

/// A chart's words for a screen reader and its legend: the panel's "Last 15
/// minutes" (`span` None) or the history's ("24 hours", "7 days").
pub struct ChartWords {
    pub label: String,
    pub memory: String,
    pub cpu: String,
}

pub fn chart_words(
    name: &str,
    span: Option<&str>,
    memory_mb: &[f64],
    cpu_percent: &[f64],
) -> ChartWords {
    let mem_max = memory_mb.iter().copied().fold(0.0, f64::max);
    let cpu_max = cpu_percent.iter().copied().fold(0.0, f64::max);
    let (m, c) = (memory(Some(mem_max)), percent(Some(cpu_max)));
    match span {
        None => ChartWords {
            label: format!("Memory over the last 15 minutes, at most {m}; CPU at most {c}"),
            memory: format!("Memory, max {m}"),
            cpu: format!("CPU, dashed, max {c}"),
        },
        Some(span) => ChartWords {
            label: format!("{name} over the last {span}: memory at most {m}, CPU at most {c}"),
            memory: format!("Memory, max {m}"),
            cpu: format!("CPU, max {c}"),
        },
    }
}

/// The panel's figures line: "643 MB · 12% CPU", and what the app reports
/// inside WSL.
pub fn panel_figures(app: &AppView) -> String {
    let parts: Vec<String> = [memory(app.memory_mb), cpu(app.cpu_percent)]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    let mut line = parts.join(" · ");
    if !line.is_empty()
        && let Some(reported) = app.namespaced_mb.filter(|m| *m != 0.0)
    {
        line.push_str(&format!(
            " · {} reported inside WSL",
            memory(Some(reported))
        ));
    }
    line
}

/// The details table's memory cell: what the app reports inside WSL with a
/// `*` when it measures nothing itself, else its memory, else "-".
pub fn details_memory(app: &AppView) -> String {
    let reported = app.memory_mb.unwrap_or(0.0) == 0.0 && app.namespaced_mb.unwrap_or(0.0) > 0.0;
    if reported {
        format!("{}*", memory(app.namespaced_mb))
    } else if app.memory_mb.is_some() {
        memory(app.memory_mb)
    } else {
        "-".into()
    }
}

/// A table row's words for a screen reader (Slint has no table roles):
/// each column's title, then its cell ("Process voicedock-hub, Memory 643
/// MB"). Columns without a title give their cell alone.
pub fn table_row_label(titles: &[&str], cells: &[&str]) -> String {
    titles
        .iter()
        .zip(cells)
        .filter(|(_, c)| !c.is_empty())
        .map(|(t, c)| {
            if t.is_empty() {
                c.to_string()
            } else {
                format!("{t} {c}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A duration in words, for "for 3 minutes" and the like: seconds under a
/// minute, then minutes, hours and days, whole and singular where one.
pub fn duration(seconds: u64) -> String {
    let (n, unit) = match seconds {
        0..60 => (seconds, "second"),
        60..3600 => (seconds / 60, "minute"),
        3600..86_400 => (seconds / 3600, "hour"),
        _ => (seconds / 86_400, "day"),
    };
    if n == 1 {
        format!("1 {unit}")
    } else {
        format!("{n} {unit}s")
    }
}

/// What the window says about an update, and which button it offers.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateLine {
    pub text: String,
    /// A second, smaller line: why it failed.
    pub detail: Option<String>,
    pub action: Option<UpdateAction>,
    /// The download's share (0 to 1), while it is known.
    pub progress: Option<f64>,
    pub problem: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateAction {
    Install,
    Retry,
}

pub fn update_line(u: &UpdateView) -> Option<UpdateLine> {
    let v = u.available.as_deref()?;
    let name = super::PRODUCT;
    Some(match u.install.as_str() {
        "downloading" => UpdateLine {
            text: match u.progress {
                None => format!("Downloading {name} {v}"),
                Some(p) => format!("Downloading {name} {v}: {p}%"),
            },
            detail: None,
            action: None,
            progress: u.progress.map(|p| f64::from(p) / 100.0),
            problem: false,
        },
        "installing" => UpdateLine {
            text: format!("Installing {name} {v}"),
            detail: Some(format!("{name} starts again by itself")),
            action: None,
            progress: Some(1.0),
            problem: false,
        },
        "failed" => UpdateLine {
            text: format!("Update to {v} failed"),
            detail: u.install_error.clone(),
            action: Some(UpdateAction::Retry),
            progress: None,
            problem: true,
        },
        _ => UpdateLine {
            text: format!("{name} {v} is available"),
            detail: None,
            action: Some(UpdateAction::Install),
            progress: None,
            problem: false,
        },
    })
}

/// Keys that move through a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKey {
    Down,
    Up,
    Home,
    End,
}

/// The next index for a key in a list of `count` (`current` is -1 when
/// nothing in it has the focus), or `None` for an empty list.
pub fn next_index(key: ListKey, current: i64, count: i64) -> Option<i64> {
    if count <= 0 {
        return None;
    }
    Some(match key {
        ListKey::Down if current < 0 => 0,
        ListKey::Down => (count - 1).min(current + 1),
        ListKey::Up if current < 0 => count - 1,
        ListKey::Up => 0.max(current - 1),
        ListKey::Home => 0,
        ListKey::End => count - 1,
    })
}

#[cfg(test)]
mod tests {
    //! Ported case by case from `app/ui/tests/unit/model.test.ts`, on the
    //! same snapshots (`native/fixtures`, kept equal to the interface's).
    use super::*;
    use crate::tests::fixture;

    fn app(id: &str, fixture_name: &str) -> AppView {
        fixture(fixture_name)
            .apps
            .into_iter()
            .find(|a| a.id == id)
            .unwrap_or_else(|| panic!("{id} in {fixture_name}"))
    }

    /// D32: the main view shows `inMainView` apps; All apps lists every app
    /// of the hub in two groups, and nothing else.
    #[test]
    fn the_main_view_and_all_apps_follow_in_main_view() {
        let s = fixture("pinned");
        let all: Vec<&str> = hub_apps(&s.apps).iter().map(|a| a.id.as_str()).collect();
        let main: Vec<&str> = main_view_apps(&s.apps)
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert!(main.len() < all.len(), "{main:?} of {all:?}");
        assert!(
            s.more.is_some(),
            "the line shows exactly when apps are left out"
        );
        let groups = all_apps_groups(&s.apps);
        assert_eq!(groups[0].0, "In the main view");
        assert_eq!(groups[1].0, "Not in the main view");
        let ids = |g: &[&AppView]| g.iter().map(|a| a.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&groups[0].1), main);
        assert_eq!(groups[0].1.len() + groups[1].1.len(), all.len());
        assert!(groups[1].1.iter().all(|a| !a.in_main_view));
        // Every app in the main view: one group, and no more line.
        let h = fixture("healthy");
        assert!(h.more.is_none());
        let groups = all_apps_groups(&h.apps);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), hub_apps(&h.apps).len());
    }

    // describe("rows")

    #[test]
    fn rows_offer_one_action_stop_while_running_start_while_not() {
        assert_eq!(
            row_action(&app("voicedock", "busy")).map(|a| a.kind),
            Some(ActionKindView::Stop)
        );
        assert_eq!(
            row_action(&app("n8commentary", "busy")).map(|a| a.kind),
            Some(ActionKindView::Start)
        );
        let mut none = app("voicedock", "busy");
        none.actions.clear();
        assert_eq!(row_action(&none), None);
    }

    #[test]
    fn rows_keep_restart_for_the_panel_only_while_running() {
        assert_eq!(
            restart_action(&app("voicedock", "busy")).map(|a| a.id.as_str()),
            Some("restart")
        );
        assert_eq!(restart_action(&app("n8commentary", "busy")), None);
    }

    #[test]
    fn rows_show_memory_measured_plus_reported_and_say_when_most_is_in_wsl() {
        assert_eq!(
            row_figures(&app("voicedock", "busy")),
            Some(Figures {
                memory: "643 MB".into(),
                sub: "12% CPU".into()
            })
        );
        assert_eq!(
            row_figures(&app("leadscraper", "busy")),
            Some(Figures {
                memory: "1.2 GB".into(),
                sub: "in WSL".into()
            })
        );
        assert_eq!(row_figures(&app("n8commentary", "busy")), None);
    }

    #[test]
    fn rows_put_what_happens_now_first_then_failures_alerts_and_the_summary() {
        assert_eq!(
            row_line(&app("n8hotkeys", "trouble")),
            ("Restarting".into(), Tone::Normal)
        );
        assert_eq!(row_line(&app("voicedock", "trouble")).1, Tone::Problem);
        assert_eq!(
            row_line(&app("leadscraper", "busy")),
            ("Over its 1 GB warning".into(), Tone::Attention)
        );
        assert_eq!(
            row_line(&app("n8hotkeys", "busy")).0,
            "6 hotkeys registered"
        );
    }

    #[test]
    fn rows_are_one_list_in_the_snapshots_order_hidden_and_own_icon_apps_left_out() {
        let mut apps = fixture("busy").apps;
        let mut own = app("voicedock", "busy");
        own.id = "x".into();
        own.hosted = false;
        apps.push(own);
        let shown: Vec<&str> = hub_apps(&apps).iter().map(|a| a.id.as_str()).collect();
        assert!(!shown.contains(&"x"), "{shown:?}");
    }

    #[test]
    fn rows_name_the_component_in_per_component_actions() {
        let vd = app("voicedock", "busy");
        let labels: Vec<String> = component_actions(&vd, &vd.components[0])
            .into_iter()
            .map(|(_, label)| label)
            .collect();
        assert_eq!(labels, ["Start Kokoro", "Stop Kokoro"]);
    }

    // describe("drawing and words")

    #[test]
    fn points_scale_into_the_box_from_zero() {
        assert_eq!(
            points(&[0.0, 10.0], 10.0, 10.0, None, 2.0, 0),
            "0.0,9.0 10.0,2.9"
        );
        assert_eq!(points(&[], 10.0, 10.0, None, 2.0, 0), "");
        assert_eq!(
            points(&[0.0, 0.0, 10.0], 10.0, 10.0, None, 2.0, 2),
            "10.0,2.9"
        );
    }

    #[test]
    fn drawings_are_todays_points_as_path_commands() {
        // Under a minute: nothing.
        assert_eq!(spark(&[0.0, 0.0, 5.0, 6.0]), Drawing::default());
        let s = spark(&[0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let today = points(
            &[0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            44.0,
            20.0,
            None,
            2.0,
            2,
        );
        let as_commands = today
            .replace(',', " ")
            .split(' ')
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|p| p.join(" "))
            .collect::<Vec<_>>();
        assert_eq!(s.line, format!("M {}", as_commands.join(" L ")));
        assert!(s.fill.starts_with("M 12.6 20.0 L "), "{}", s.fill);
        assert!(s.fill.ends_with(" L 44.0 20.0 Z"), "{}", s.fill);
        let c = chart(&[1.0, 2.0], &[0.0, 5.0]);
        assert_eq!(c.memory.line, "M 0.0 37.3 L 336.0 11.7");
        assert_eq!(
            c.memory.fill,
            "M 0.0 64.0 L 0.0 37.3 L 336.0 11.7 L 336.0 64.0 Z"
        );
        // CPU against at least 10%: 5% of a 10% top (x 1.15).
        assert_eq!(c.cpu, "M 0.0 63.0 L 336.0 37.3");
        assert_eq!(chart(&[], &[]), ChartDrawing::default());
    }

    #[test]
    fn a_sparkline_starts_where_the_latest_run_starts() {
        assert_eq!(run_start(&[0.0, 0.0, 5.0, 6.0]), 2);
        assert_eq!(run_start(&[3.0, 0.0, 5.0]), 2);
        assert_eq!(run_start(&[1.0, 2.0]), 0);
        assert_eq!(run_start(&[0.0, 0.0]), 2);
    }

    #[test]
    fn memory_and_cpu_are_formatted() {
        assert_eq!(memory(Some(1229.0)), "1.2 GB");
        assert_eq!(memory(Some(42.4)), "42 MB");
        assert_eq!(cpu(Some(0.0)), "0% CPU");
        assert_eq!(cpu(Some(0.14)), "0.1% CPU");
        assert_eq!(cpu(Some(12.4)), "12% CPU");
        assert_eq!(percent(None), "-");
    }

    #[test]
    fn lists_are_moved_through_with_arrows_home_and_end() {
        assert_eq!(next_index(ListKey::Down, -1, 3), Some(0));
        assert_eq!(next_index(ListKey::Down, 2, 3), Some(2));
        assert_eq!(next_index(ListKey::Up, 0, 3), Some(0));
        assert_eq!(next_index(ListKey::End, 0, 3), Some(2));
        assert_eq!(next_index(ListKey::Home, 2, 3), Some(0));
        // Any other key is not a `ListKey`: the caller ignores it, as today's
        // `nextIndex("a", ...)` returns null.
        assert_eq!(next_index(ListKey::Down, 0, 0), None);
    }

    // describe("updates")

    #[test]
    fn updates_say_nothing_without_one() {
        assert_eq!(update_line(&fixture("busy").update), None);
    }

    #[test]
    fn updates_offer_install_then_show_the_download_then_try_again_after_a_failure() {
        assert_eq!(
            update_line(&fixture("trouble").update),
            Some(UpdateLine {
                text: "N8 Tray 0.3.0 is available".into(),
                detail: None,
                action: Some(UpdateAction::Install),
                progress: None,
                problem: false
            })
        );
        assert_eq!(
            update_line(&fixture("updating").update),
            Some(UpdateLine {
                text: "Downloading N8 Tray 0.3.0: 42%".into(),
                detail: None,
                action: None,
                progress: Some(0.42),
                problem: false
            })
        );
        let mut unknown = fixture("updating").update;
        unknown.progress = None;
        assert_eq!(update_line(&unknown).unwrap().progress, None);
        let mut installing = fixture("trouble").update;
        installing.install = "installing".into();
        assert_eq!(update_line(&installing).unwrap().action, None);
        let failed = update_line(&fixture("updateFailed").update).unwrap();
        assert_eq!(failed.action, Some(UpdateAction::Retry));
        assert!(failed.problem);
        assert_eq!(failed.text, "Update to 0.3.0 failed");
        assert!(failed.detail.unwrap().contains("signature"));
    }

    // Beyond today's tests: the places where Rust and JavaScript differ.

    #[test]
    fn ties_round_up_as_in_javascript() {
        assert_eq!(to_fixed(0.25, 1), "0.3");
        assert_eq!(to_fixed(1.25, 1), "1.3");
        assert_eq!(to_fixed(9.95, 1), "9.9", "9.95 is 9.9499... in binary");
        assert_eq!(to_fixed(9.96, 1), "10.0");
        assert_eq!(to_fixed(0.0, 1), "0.0");
        assert_eq!(to_fixed(-0.0, 1), "0.0");
        assert_eq!(to_fixed(-0.04, 1), "-0.0");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(memory(Some(1280.0)), "1.3 GB");
        assert_eq!(memory(Some(999.5)), "1000 MB");
        assert_eq!(memory(Some(-0.0)), "0 MB");
        assert_eq!(memory(Some(-0.4)), "0 MB");
        assert_eq!(memory(None), "");
        assert_eq!(cpu(Some(4.0)), "4% CPU");
        assert_eq!(
            cpu(Some(9.96)),
            "10% CPU",
            "as today: \"10.0\" loses its \".0\""
        );
        assert_eq!(percent(Some(55.5)), "56%");
    }

    #[test]
    fn times_follow_the_users_clock_as_to_locale_time_string_does() {
        use n8tray_platform::locale::Clock;
        let us = Clock {
            twelve_hour: true,
            am: "AM".into(),
            pm: "PM".into(),
            marker_first: false,
        };
        assert_eq!(clock_time(14, 5, &us), "02:05 PM");
        assert_eq!(clock_time(0, 7, &us), "12:07 AM");
        assert_eq!(clock_time(12, 0, &us), "12:00 PM");
        let gb = Clock::twenty_four();
        assert_eq!(clock_time(14, 5, &gb), "14:05");
        assert_eq!(clock_time(0, 7, &gb), "00:07");
        let first = Clock {
            twelve_hour: true,
            am: "上午".into(),
            pm: "下午".into(),
            marker_first: true,
        };
        assert_eq!(clock_time(9, 30, &first), "上午 09:30");
    }

    #[test]
    fn chart_panel_and_table_words_are_todays() {
        let w = chart_words("VoiceDock", None, &[600.0, 643.0], &[3.0, 12.4]);
        assert_eq!(
            w.label,
            "Memory over the last 15 minutes, at most 643 MB; CPU at most 12%"
        );
        assert_eq!(w.memory, "Memory, max 643 MB");
        assert_eq!(w.cpu, "CPU, dashed, max 12%");
        let h = chart_words("VoiceDock", Some("24 hours"), &[], &[]);
        assert_eq!(
            h.label,
            "VoiceDock over the last 24 hours: memory at most 0 MB, CPU at most 0%"
        );
        assert_eq!(h.cpu, "CPU, max 0%");
        let vd = app("voicedock", "busy");
        assert_eq!(panel_figures(&vd), "643 MB · 12% CPU");
        let ls = app("leadscraper", "busy");
        assert_eq!(details_memory(&ls), "1.2 GB*");
        assert_eq!(details_memory(&vd), "643 MB");
        assert_eq!(
            table_row_label(&["Process", "Memory", ""], &["hub", "643 MB", "verified"]),
            "Process hub, Memory 643 MB, verified"
        );
    }

    #[test]
    fn the_title_bar_summary_counts_pinned_apps_and_n8s_memory() {
        assert_eq!(title_summary(3, 3, 1945.6), "3/3 · 1.9 GB");
        assert_eq!(title_summary(0, 0, 1945.6), "0 pinned · 1.9 GB");
        assert_eq!(title_summary(1, 2, 312.0), "1/2 · 312 MB");
        let s = fixture("pinned");
        assert_eq!(
            title_summary(s.pinned_running, s.pinned_total, s.memory.n8_mb),
            "1/1 · 768 MB"
        );
        assert_eq!(title_summary_label(&s), "1 of 1 running · N8 768 MB in all");
        let s = fixture("busy");
        assert_eq!(
            title_summary(s.pinned_running, s.pinned_total, s.memory.n8_mb),
            "3/4 · 1.9 GB"
        );
        assert_eq!(title_summary_label(&s), "3 of 4 running · N8 1.9 GB in all");
    }

    #[test]
    fn levels_have_words_and_names() {
        assert_eq!(level_words(LevelName::Ok), "Healthy");
        assert_eq!(level_words(LevelName::Attention), "Needs attention");
        assert_eq!(level_words(LevelName::Problem), "Problem");
        assert_eq!(level_words(LevelName::Off), "Idle");
        assert_eq!(level_words(LevelName::Hidden), "Idle");
        assert_eq!(level_name(LevelName::Attention), "attention");
    }

    #[test]
    fn histories_and_times() {
        assert!(has_history(Some(&[0.0, 1.0])));
        assert!(!has_history(Some(&[0.0, 0.0])));
        assert!(!has_history(None));
        assert_eq!(since(None), "-");
        assert_eq!(since(Some("not a time")), "-");
        assert_ne!(since(Some("2026-11-25T10:05:00Z")), "-");
        assert_eq!(duration(0), "0 seconds");
        // A thread of its own: the clock of golden pictures stays on it.
        std::thread::spawn(|| {
            fix_clock_for_pictures();
            assert_eq!(since(Some("2026-11-25T10:05:00+02:00")), "08:05");
            assert_eq!(since(Some("2026-11-25T23:59:00Z")), "23:59");
        })
        .join()
        .unwrap();
        assert_eq!(duration(1), "1 second");
        assert_eq!(duration(59), "59 seconds");
        assert_eq!(duration(60), "1 minute");
        assert_eq!(duration(7200), "2 hours");
        assert_eq!(duration(86_400 * 3), "3 days");
    }
}
