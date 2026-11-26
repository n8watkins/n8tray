//! The kit's row helpers: the snapshot's levels and tones as the widgets'
//! enums, an app's `RowData` (words, figures, sparkline, action and its
//! armed state), and putting the keyboard focus back on the same app after
//! its list was reordered.
#![forbid(unsafe_code)]

use std::time::Instant;

use n8tray_core::model::{ActionKindView, AppView, LevelName};
use slint::{Image, SharedString};

use super::action::action_ui;
use super::confirm::{self, Confirms};
use super::format::{self, Tone};
use crate::ui::{Level, RowAction, RowData, RowFocus, Tone as UiTone};

pub fn level(level: LevelName) -> Level {
    match level {
        LevelName::Hidden => Level::Hidden,
        LevelName::Off => Level::Off,
        LevelName::Ok => Level::Ok,
        LevelName::Attention => Level::Attention,
        LevelName::Problem => Level::Problem,
    }
}

pub fn tone(tone: Tone) -> UiTone {
    match tone {
        Tone::Normal => UiTone::Normal,
        Tone::Attention => UiTone::Attention,
        Tone::Problem => UiTone::Problem,
    }
}

fn row_action(kind: ActionKindView) -> RowAction {
    match kind {
        ActionKindView::Start => RowAction::Start,
        ActionKindView::Stop => RowAction::Stop,
        ActionKindView::Restart => RowAction::Restart,
        ActionKindView::App => RowAction::None,
    }
}

/// An app's row. `watching`: the PowerShell tray manages the apps, so the
/// row offers no action (today's `Row`). The tooltip is the app's detail,
/// else its whole line, which the row cuts with an ellipsis when long.
pub fn row_data(
    app: &AppView,
    icon: Image,
    confirms: &Confirms,
    watching: bool,
    now: Instant,
) -> RowData {
    let (line, line_tone) = format::row_line(app);
    let figures = format::row_figures(app);
    let spark = match &app.history {
        Some(h) if app.running && format::has_history(Some(&h.memory_mb)) => {
            format::spark(&h.memory_mb)
        }
        _ => format::Drawing::default(),
    };
    let action = if watching {
        None
    } else {
        format::row_action(app)
    };
    let (memory, sub) = figures.map(|f| (f.memory, f.sub)).unwrap_or_default();
    let mut data = RowData {
        id: app.id.as_str().into(),
        name: app.name.as_str().into(),
        line: line.as_str().into(),
        tone: tone(line_tone),
        memory: memory.as_str().into(),
        sub: sub.as_str().into(),
        spark_line: spark.line.into(),
        spark_fill: spark.fill.into(),
        level: level(app.level),
        icon,
        tooltip: app.detail.clone().unwrap_or(line.clone()).into(),
        label: SharedString::from(row_label(
            &app.name,
            app.pending.as_deref().unwrap_or(&app.words),
            &line,
            &memory,
            &sub,
        )),
        ..RowData::default()
    };
    if let Some(action) = action {
        let armed = confirms.is_armed(&confirm::key(&app.id, &action.id, None), now);
        let ui = action_ui(app, action, None, None, armed);
        data.action = row_action(action.kind);
        data.action_label = ui.label.into();
        data.action_tooltip = ui.tooltip.into();
        data.action_enabled = ui.enabled;
        data.armed = ui.armed;
    }
    data
}

/// What a screen reader says for a row: its name, the app's words when the
/// line says something else (today's hidden ", Running," after the name,
/// `Hub.tsx`), the line and the figures.
pub fn row_label(name: &str, words: &str, line: &str, memory: &str, sub: &str) -> String {
    let words = if words == line { "" } else { words };
    [name, words, line, memory, sub]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(", ")
}

/// After a list was reordered (`list::sync` said `moved`), puts the focus
/// back on the app that had it. Read `focused` before the sync; call this
/// right after it, in the same turn: a row the sync moved is a new instance
/// that does not exist until the next layout, and it takes the request when
/// it is created (`RowFocus.pending`).
pub fn refocus(focus: &RowFocus<'_>, focused: &(SharedString, i32)) {
    if !focused.0.is_empty() {
        focus.invoke_to_id(focused.0.clone(), focused.1);
    }
}

/// The row and part that have the focus now, to give to `refocus`.
pub fn focused(focus: &RowFocus<'_>) -> (SharedString, i32) {
    (focus.get_focused_id(), focus.get_focused_part())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use slint::{Model, VecModel};

    use super::super::confirm::Press;
    use super::super::list;
    use super::*;
    use crate::tests::fixture;

    fn rows(apps: &[AppView], confirms: &Confirms, now: Instant) -> Vec<RowData> {
        apps.iter()
            .map(|a| row_data(a, Image::default(), confirms, false, now))
            .collect()
    }

    #[test]
    fn a_row_has_todays_words_figures_and_action() {
        let s = fixture("busy");
        let vd = s.apps.iter().find(|a| a.id == "voicedock").unwrap();
        let r = row_data(
            vd,
            Image::default(),
            &Confirms::default(),
            false,
            Instant::now(),
        );
        assert_eq!(r.memory, "643 MB");
        assert_eq!(r.sub, "12% CPU");
        assert_eq!(r.action, RowAction::Stop);
        assert!(r.action_label.ends_with("VoiceDock"));
        assert!(!r.spark_line.is_empty());
        let watching = row_data(
            vd,
            Image::default(),
            &Confirms::default(),
            true,
            Instant::now(),
        );
        assert_eq!(watching.action, RowAction::None);
    }

    /// Today's screen-reader text: the app's words after its name when the
    /// line shows something else (a detail, an alert), not twice otherwise.
    #[test]
    fn a_rows_label_says_the_words_only_when_the_line_differs() {
        assert_eq!(
            row_label("VoiceDock", "Running", "Running", "643 MB", "12% CPU"),
            "VoiceDock, Running, 643 MB, 12% CPU"
        );
        assert_eq!(
            row_label("VoiceDock", "Running", "Over its 1 GB warning", "1.2 GB", ""),
            "VoiceDock, Running, Over its 1 GB warning, 1.2 GB"
        );
        assert_eq!(row_label("Lead Scraper", "Off", "Off", "", ""), "Lead Scraper, Off");
    }

    /// Arm Stop on app A, reorder the list: the armed state stays on A's
    /// row, and the second click still runs A's Stop. A refresh keeps it.
    #[test]
    fn the_armed_state_follows_the_app_through_a_reorder_and_a_refresh() {
        let s = fixture("busy");
        let mut apps: Vec<AppView> = s
            .apps
            .iter()
            .filter(|a| format::row_action(a).is_some())
            .cloned()
            .collect();
        assert!(apps.len() >= 2);
        let mut confirms = Confirms::default();
        let t = Instant::now();
        let model = VecModel::default();
        list::sync(&model, rows(&apps, &confirms, t), |r| r.id.clone());
        let a = apps[0].clone();
        let stop = format::row_action(&a).unwrap().id.clone();
        assert_eq!(
            confirms.press(confirm::key(&a.id, &stop, None), true, t),
            Press::Armed
        );
        apps.reverse();
        let moved = list::sync(&model, rows(&apps, &confirms, t), |r| r.id.clone());
        assert!(moved.moved);
        let armed: Vec<String> = model
            .iter()
            .filter(|r| r.armed)
            .map(|r| r.id.to_string())
            .collect();
        assert_eq!(armed, std::slice::from_ref(&a.id));
        assert_eq!(model.row_data(apps.len() - 1).unwrap().id, a.id.as_str());
        // A refresh with the same order keeps it, and the second click runs A.
        let later = t + Duration::from_secs(2);
        list::sync(&model, rows(&apps, &confirms, later), |r| r.id.clone());
        assert!(model.iter().any(|r| r.id == a.id.as_str() && r.armed));
        assert_eq!(
            confirms.press(confirm::key(&a.id, &stop, None), true, later),
            Press::Run
        );
    }
}
