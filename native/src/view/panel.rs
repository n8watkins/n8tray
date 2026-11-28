//! One app's panel (`Panel.tsx` with `PanelHead`; lane: app panel; owns this
//! file, `ui/views/panel.slint`, `ui/views/panel-model.slint` and
//! `native/tests/goldens/view-panel/`): the title bar with the app's
//! Start or Stop and Restart, then State, Components, Last 15 minutes,
//! Processes, Open and the app's settings.
#![forbid(unsafe_code)]

use std::rc::Rc;
use std::time::Instant;

use n8tray_core::model::{ActionKindView, ActionView, AppView, LevelName};
use slint::{Model, ModelRc, VecModel};

use super::action::action_ui;
use super::confirm::{self, Confirms};
use super::format::{self, Tone};
use super::intent::{self, Intent};
use super::list;
use super::router::{Ctx, Models};
use super::rows;
use crate::ui::{
    BarAction, GlyphName, Level, OpenItem, PanelButton, PanelComponent, PanelSay, TableRow,
};

/// The view's golden images (`router::Shot`); the fixtures' icons are the
/// spec's examples (`gallery::fixture` resolves them).
#[cfg(debug_assertions)]
pub const SHOTS: &[super::router::Shot] = &[
    super::router::Shot {
        app: "voicedock",
        ..super::router::Shot::of("busy", "busy")
    },
    super::router::Shot {
        app: "voicedock",
        compact: true,
        ..super::router::Shot::of("compact", "busy")
    },
    // The tall window (600) cuts the panel and scrolls.
    super::router::Shot {
        app: "voicedock",
        height: 600,
        ..super::router::Shot::of("busy-tall", "busy")
    },
    // Every section in view: a height no setting has, for the picture only.
    super::router::Shot {
        app: "voicedock",
        height: WHOLE,
        ..super::router::Shot::of("whole", "busy")
    },
    // Stop asks for a second click (an app's own action that confirms):
    // armed in the title bar and in a component's button.
    super::router::Shot {
        app: "voicedock",
        height: WHOLE,
        stage: Some(stage_armed),
        ..super::router::Shot::of("armed", "busy")
    },
    // Not running: Start, no Restart, no processes, nothing to chart; a
    // log that is not there yet.
    super::router::Shot {
        app: "n8commentary",
        ..super::router::Shot::of("off", "busy")
    },
    // An alert on the line, memory reported inside WSL, two processes.
    super::router::Shot {
        app: "leadscraper",
        height: WHOLE,
        ..super::router::Shot::of("alert", "busy")
    },
    // Failed, with problem components and no history.
    super::router::Shot {
        app: "voicedock",
        height: WHOLE,
        ..super::router::Shot::of("trouble", "trouble")
    },
    // Busy with an action, notes, a GPU line, a long name and a long
    // detail, memory reported inside WSL.
    super::router::Shot {
        app: "voicedock",
        height: WHOLE,
        stage: Some(stage_full),
        ..super::router::Shot::of("full", "busy")
    },
    // The PowerShell tray manages the apps: the panel keeps its buttons.
    super::router::Shot {
        app: "n8hotkeys",
        ..super::router::Shot::of("watching", "watching")
    },
];

/// The height limit of the pictures that show the whole panel: more than
/// the tallest (`full`) needs, so the window fits its content and the
/// version line at the end is in the picture.
#[cfg(debug_assertions)]
const WHOLE: u32 = 1400;

#[cfg(debug_assertions)]
fn stage_armed(stage: &mut super::router::Stage) {
    let now = Instant::now();
    for (action, component) in [("stop", None), ("engine.stop", Some("kokoro"))] {
        stage
            .confirms
            .press(confirm::key("voicedock", action, component), true, now);
    }
    for action in &mut voicedock(stage).actions {
        action.confirm = action.id == "stop" || action.id == "engine.stop";
    }
}

/// The shots' app, found by its id rather than its place in the fixture.
#[cfg(debug_assertions)]
fn voicedock<'a>(stage: &'a mut super::router::Stage) -> &'a mut AppView {
    stage
        .snapshot
        .apps
        .iter_mut()
        .find(|a| a.id == "voicedock")
        .expect("the busy fixture has VoiceDock")
}

#[cfg(debug_assertions)]
fn stage_full(stage: &mut super::router::Stage) {
    let app = voicedock(stage);
    app.name = "VoiceDock Studio Edition With A Very Long Name".into();
    app.pending = Some("Restarting".into());
    app.notes = vec![
        "Voices were moved to D:\\voices".into(),
        "Update the engine before the next session".into(),
    ];
    app.gpu = Some(n8tray_core::model::GpuView {
        mem_mb: Some(2048.0),
        percent: Some(37.0),
        source: "measured".into(),
    });
    app.namespaced_mb = Some(310.0);
    app.detail = Some("Warming up the speech engine, this can take a minute".into());
}

/// The app's pill words: what is happening now, else its state.
fn words(app: &AppView) -> &str {
    app.pending.as_deref().unwrap_or(&app.words)
}

fn say_tone(tone: Tone) -> crate::ui::Tone {
    rows::tone(tone)
}

/// The State section's paragraphs: the line, the detail when it says more,
/// the figures, the GPU, the description and the notes.
pub fn say(app: &AppView) -> Vec<PanelSay> {
    let (line, tone) = format::row_line(app);
    let mut out = vec![PanelSay {
        id: "line".into(),
        text: line.as_str().into(),
        tone: say_tone(tone),
    }];
    let plain = |id: &str, text: String| PanelSay {
        id: id.into(),
        text: text.into(),
        tone: crate::ui::Tone::Normal,
    };
    if let Some(detail) = app.detail.as_ref().filter(|d| **d != line) {
        out.push(plain("detail", detail.clone()));
    }
    let figures = format::panel_figures(app);
    if !figures.is_empty() {
        out.push(plain("figures", figures));
    }
    if let Some(gpu) = app
        .gpu
        .as_ref()
        .filter(|g| g.mem_mb.is_some() || g.percent.is_some())
    {
        let share = gpu
            .percent
            .map(|p| format!(" · {}", format::percent(Some(p))))
            .unwrap_or_default();
        out.push(plain(
            "gpu",
            format!("GPU {}{share} ({})", format::memory(gpu.mem_mb), gpu.source),
        ));
    }
    if let Some(description) = app.description.as_ref().filter(|d| !d.is_empty()) {
        out.push(plain("description", description.clone()));
    }
    for (i, note) in app.notes.iter().enumerate() {
        out.push(PanelSay {
            id: format!("note-{i}").into(),
            text: note.as_str().into(),
            tone: crate::ui::Tone::Attention,
        });
    }
    out
}

fn button(
    app: &AppView,
    action: &ActionView,
    component: Option<&str>,
    label: Option<&str>,
    confirms: &Confirms,
    now: Instant,
) -> PanelButton {
    let armed = confirms.is_armed(&confirm::key(&app.id, &action.id, component), now);
    let ui = action_ui(app, action, component, label, armed);
    PanelButton {
        id: action.id.as_str().into(),
        text: ui.words.into(),
        label: ui.label.into(),
        tooltip: ui.tooltip.into(),
        enabled: ui.enabled,
        armed: ui.armed,
    }
}

/// The title bar's Start or Stop, then Restart.
pub fn bar_actions(app: &AppView, confirms: &Confirms, now: Instant) -> Vec<BarAction> {
    [format::row_action(app), format::restart_action(app)]
        .into_iter()
        .flatten()
        .map(|action| {
            let armed = confirms.is_armed(&confirm::key(&app.id, &action.id, None), now);
            let ui = action_ui(app, action, None, None, armed);
            let (glyph, size) = match action.kind {
                ActionKindView::Stop => (GlyphName::Stop, 13.0),
                ActionKindView::Restart => (GlyphName::Restart, 14.0),
                _ => (GlyphName::Play, 14.0),
            };
            BarAction {
                id: action.id.as_str().into(),
                glyph,
                glyph_size: size,
                label: ui.label.into(),
                tooltip: ui.tooltip.into(),
                enabled: ui.enabled,
                armed: ui.armed,
            }
        })
        .collect()
}

pub fn components(app: &AppView, confirms: &Confirms, now: Instant) -> Vec<PanelComponent> {
    app.components
        .iter()
        .map(|c| {
            let tone = match c.level {
                LevelName::Problem => Tone::Problem,
                LevelName::Attention => Tone::Attention,
                _ => Tone::Normal,
            };
            let buttons: Vec<PanelButton> = format::component_actions(app, c)
                .into_iter()
                .map(|(action, label)| {
                    button(app, action, Some(&c.id), Some(&label), confirms, now)
                })
                .collect();
            PanelComponent {
                id: c.id.as_str().into(),
                name: c.name.as_str().into(),
                line: c.detail.as_deref().unwrap_or(&c.words).into(),
                tone: say_tone(tone),
                memory: c
                    .memory_mb
                    .map(|m| format::memory(Some(m)))
                    .unwrap_or_default()
                    .into(),
                buttons: ModelRc::from(Rc::new(VecModel::from(buttons))),
            }
        })
        .collect()
}

/// The columns' names a screen reader says: the last one has no visible
/// title, but today's has "Verdict" for screen readers (`sr-only`).
const PROCESS_TITLES: [&str; 5] = ["Process", "Memory", "Private", "CPU", "Verdict"];

pub fn processes(app: &AppView) -> Vec<TableRow> {
    app.processes
        .iter()
        .map(|p| {
            let small = [
                Some(p.pid.to_string()),
                p.component.clone().or(p.role.clone()),
            ]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
            let dash = |mb: Option<f64>| mb.map(|m| format::memory(Some(m))).unwrap_or("-".into());
            let cells = [
                p.name.clone(),
                dash(p.rss_mb),
                dash(p.private_mb),
                format::percent(p.cpu_percent),
                String::new(),
            ];
            let first = if small.is_empty() {
                p.name.clone()
            } else {
                format!("{} {small}", p.name)
            };
            let spoken: Vec<&str> = std::iter::once(first.as_str())
                .chain(cells[1..4].iter().map(String::as_str))
                .chain(std::iter::once(p.verdict.as_str()))
                .collect();
            let level = match p.verdict.as_str() {
                "verified" => Level::Ok,
                "unreported" | "unverified" => Level::Attention,
                "rejected" => Level::Problem,
                _ => Level::Off,
            };
            TableRow {
                cells: ModelRc::from(Rc::new(VecModel::from(
                    cells.iter().map(|c| c.as_str().into()).collect::<Vec<_>>(),
                ))),
                small: small.into(),
                tag: p.verdict.as_str().into(),
                tag_level: level,
                tooltip: p.note.clone().unwrap_or_default().into(),
                label: format::table_row_label(&PROCESS_TITLES, &spoken).into(),
            }
        })
        .collect()
}

pub fn opens(app: &AppView) -> Vec<OpenItem> {
    let files = app.open.iter().map(|o| OpenItem {
        key: o.key.as_str().into(),
        glyph: match o.key.as_str() {
            "settings" => GlyphName::Gear,
            "folder" => GlyphName::Folder,
            _ => GlyphName::File,
        },
        text: o.label.as_str().into(),
        tooltip: if o.exists {
            o.target.clone()
        } else {
            format!("{}: not there yet", o.label)
        }
        .into(),
        enabled: o.exists,
    });
    let menu = app.menu.iter().enumerate().map(|(i, m)| OpenItem {
        key: format!("menu:{i}").into(),
        glyph: match m.kind.as_str() {
            "open-url" => GlyphName::Web,
            "action" => GlyphName::Play,
            _ => GlyphName::Folder,
        },
        text: m.label.as_str().into(),
        tooltip: Default::default(),
        enabled: m.exists,
    });
    files.chain(menu).collect()
}

/// Replaces a model's rows in place (matched by `key`), so the rows that did
/// not change keep their widgets; a model another view put there is replaced.
fn sync<T: Clone + PartialEq + 'static, K: PartialEq>(
    current: ModelRc<T>,
    items: Vec<T>,
    key: impl Fn(&T) -> K,
) -> Option<ModelRc<T>> {
    match current.as_any().downcast_ref::<VecModel<T>>() {
        Some(model) => {
            list::sync(model, items, key);
            None
        }
        None => Some(ModelRc::from(Rc::new(VecModel::from(items)))),
    }
}

/// A row's inner list for its next data: the list it shows now, updated in
/// place by `key` to hold what `new` holds, so a row whose figures changed
/// keeps the widgets of its inner list (a component's buttons). `None` when
/// the row has no list of its own yet.
fn keep<T: Clone + PartialEq + 'static, K: PartialEq>(
    shown: &ModelRc<T>,
    new: &ModelRc<T>,
    key: impl Fn(&T) -> K,
) -> Option<ModelRc<T>> {
    let model = shown.as_any().downcast_ref::<VecModel<T>>()?;
    list::sync(model, new.iter().collect(), key);
    Some(shown.clone())
}

/// `keep` for a list whose items have no key but their place (a table
/// row's cells).
fn keep_by_place<T: Clone + PartialEq + 'static>(
    shown: &ModelRc<T>,
    new: &ModelRc<T>,
) -> Option<ModelRc<T>> {
    let model = shown.as_any().downcast_ref::<VecModel<T>>()?;
    let items: Vec<T> = new.iter().collect();
    for (i, item) in items.iter().enumerate() {
        match model.row_data(i) {
            Some(current) if current == *item => {}
            Some(_) => model.set_row_data(i, item.clone()),
            None => model.push(item.clone()),
        }
    }
    while model.row_count() > items.len() {
        model.remove(model.row_count() - 1);
    }
    Some(shown.clone())
}

/// The router shows a panel only while its app is in the snapshot (it
/// falls back to the hub when the app leaves).
pub fn fill(ui: &dyn Models, ctx: &mut Ctx) {
    let Some(app) = ctx
        .app
        .and_then(|id| ctx.snapshot.apps.iter().find(|a| a.id == id))
    else {
        return;
    };
    let m = ui.panel();
    if m.get_app_id() != app.id.as_str() {
        // Another app's panel: no row of the last one stays.
        m.set_actions(ModelRc::default());
        m.set_components(ModelRc::default());
        m.set_buttons(ModelRc::default());
        m.set_processes(ModelRc::default());
        m.set_opens(ModelRc::default());
        m.set_lines(ModelRc::default());
    }
    m.set_app_id(app.id.as_str().into());
    m.set_name(app.name.as_str().into());
    m.set_icon(ctx.icons.get(app.icon.as_deref()));
    m.set_level(rows::level(app.level));
    m.set_words(words(app).into());
    // Every list below is updated in place, nested lists too: a new model
    // makes Slint build that list's widgets anew, and the control that had
    // the keyboard focus (Stop, just armed by Space) would lose it.
    if let Some(new) = sync(
        m.get_actions(),
        bar_actions(app, ctx.confirms, ctx.now),
        |a| a.id.clone(),
    ) {
        m.set_actions(new);
    }

    if let Some(new) = sync(m.get_lines(), say(app), |l| l.id.clone()) {
        m.set_lines(new);
    }
    let buttons: Vec<PanelButton> = format::app_actions(app)
        .into_iter()
        .map(|a| button(app, a, None, None, ctx.confirms, ctx.now))
        .collect();
    if let Some(new) = sync(m.get_buttons(), buttons, |b| b.id.clone()) {
        m.set_buttons(new);
    }
    let before: Vec<PanelComponent> = m.get_components().iter().collect();
    let comps = components(app, ctx.confirms, ctx.now)
        .into_iter()
        .map(|mut c| {
            if let Some(kept) = before
                .iter()
                .find(|b| b.id == c.id)
                .and_then(|b| keep(&b.buttons, &c.buttons, |x| x.id.clone()))
            {
                c.buttons = kept;
            }
            c
        })
        .collect();
    if let Some(new) = sync(m.get_components(), comps, |c| c.id.clone()) {
        m.set_components(new);
    }

    let history = app
        .history
        .as_ref()
        .filter(|h| format::has_history(Some(&h.memory_mb)));
    m.set_has_history(history.is_some());
    if let Some(h) = history {
        let chart = format::chart(&h.memory_mb, &h.cpu_percent);
        let words = format::chart_words(&app.name, None, &h.memory_mb, &h.cpu_percent);
        m.set_memory_line(chart.memory.line.into());
        m.set_memory_fill(chart.memory.fill.into());
        m.set_cpu_line(chart.cpu.into());
        m.set_chart_label(words.label.into());
        m.set_legend_memory(words.memory.into());
        m.set_legend_cpu(words.cpu.into());
    }

    // A process by its name and pid (`small` starts with the pid), as
    // today's rows are keyed; its figures change on every snapshot.
    let process_key = |r: &TableRow| (r.cells.row_data(0), r.small.clone());
    let before: Vec<TableRow> = m.get_processes().iter().collect();
    let procs = processes(app)
        .into_iter()
        .map(|mut r| {
            if let Some(kept) = before
                .iter()
                .find(|b| process_key(b) == process_key(&r))
                .and_then(|b| keep_by_place(&b.cells, &r.cells))
            {
                r.cells = kept;
            }
            r
        })
        .collect();
    if let Some(new) = sync(m.get_processes(), procs, process_key) {
        m.set_processes(new);
    }
    if let Some(new) = sync(m.get_opens(), opens(app), |o| o.key.clone()) {
        m.set_opens(new);
    }
    m.set_auto_start(app.auto_start);
    m.set_hosted(app.hosted);
    m.set_enabled(app.enabled);
    m.set_version_line(format!("{} {}", app.name, app.version).into());
}

/// An action or a setting of the app the panel shows, as an intent.
pub fn action_intent(app: &str, action: &str, component: &str) -> Intent {
    Intent::Action {
        app: app.into(),
        action: action.into(),
        component: (!component.is_empty()).then(|| component.into()),
    }
}

/// An item to open: an `open` entry by key, or `menu:<index>`.
pub fn open_intent(app: &str, key: &str) -> Intent {
    let (kind, key) = match key.strip_prefix("menu:") {
        Some(index) => ("menu", index),
        None => ("open", key),
    };
    Intent::OpenItem {
        app: app.into(),
        kind: kind.into(),
        key: key.into(),
    }
}

pub fn setting_intent(app: &str, key: &str, on: bool) -> Intent {
    Intent::Setting {
        key: key.into(),
        on,
        app: Some(app.into()),
    }
}

pub fn wire(ui: &dyn Models) {
    let m = ui.panel();
    m.on_action(|app, action, component| {
        intent::send(action_intent(&app, &action, &component));
    });
    m.on_open(|app, key| intent::send(open_intent(&app, &key)));
    m.on_setting(|app, key, on| intent::send(setting_intent(&app, &key, on)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;
    use crate::view::gallery::tests::tab_through;
    use crate::view::router::Shot;
    use slint::ComponentHandle;

    fn app(fixture_name: &str, id: &str) -> AppView {
        fixture(fixture_name)
            .apps
            .into_iter()
            .find(|a| a.id == id)
            .unwrap()
    }

    fn texts(lines: &[PanelSay]) -> Vec<String> {
        lines.iter().map(|l| l.text.to_string()).collect()
    }

    #[test]
    fn the_state_section_says_todays_paragraphs() {
        let vd = app("busy", "voicedock");
        let lines = say(&vd);
        let t = texts(&lines);
        assert_eq!(t[0], vd.summary);
        assert!(t.contains(&format::panel_figures(&vd)));
        assert!(t.contains(vd.description.as_ref().unwrap()));
        // An alert replaces the line and keeps the detail apart.
        let ls = app("busy", "leadscraper");
        let lines = say(&ls);
        assert_eq!(lines[0].text, ls.alert.as_deref().unwrap());
        assert_eq!(lines[0].tone, crate::ui::Tone::Attention);
        assert!(
            texts(&lines)
                .iter()
                .any(|l| l.ends_with("reported inside WSL"))
        );
    }

    #[test]
    fn gpu_and_notes_are_said_as_today() {
        let mut vd = app("busy", "voicedock");
        vd.gpu = Some(n8tray_core::model::GpuView {
            mem_mb: Some(2048.0),
            percent: Some(37.0),
            source: "measured".into(),
        });
        vd.notes = vec!["Heads up".into()];
        let lines = say(&vd);
        let t = texts(&lines);
        assert!(
            t.contains(&"GPU 2.0 GB · 37% (measured)".to_string()),
            "{t:?}"
        );
        assert_eq!(lines.last().unwrap().text, "Heads up");
        assert_eq!(lines.last().unwrap().tone, crate::ui::Tone::Attention);
        // No reading, no line.
        vd.gpu.as_mut().unwrap().mem_mb = None;
        vd.gpu.as_mut().unwrap().percent = None;
        assert!(!texts(&say(&vd)).iter().any(|l| l.starts_with("GPU")));
    }

    #[test]
    fn the_title_bar_offers_stop_and_restart_while_running_and_start_when_not() {
        let now = Instant::now();
        let running = bar_actions(&app("busy", "voicedock"), &Confirms::default(), now);
        let ids: Vec<String> = running.iter().map(|a| a.id.to_string()).collect();
        assert_eq!(ids, ["stop", "restart"]);
        assert_eq!(running[0].glyph, GlyphName::Stop);
        assert_eq!(running[1].glyph, GlyphName::Restart);
        assert_eq!(running[0].label, "Stop VoiceDock");
        let off = bar_actions(&app("busy", "n8commentary"), &Confirms::default(), now);
        assert_eq!(off.len(), 1);
        assert_eq!(off[0].id, "start");
        assert_eq!(off[0].glyph, GlyphName::Play);
    }

    #[test]
    fn components_carry_their_own_buttons_and_armed_state() {
        let vd = app("busy", "voicedock");
        let mut confirms = Confirms::default();
        let now = Instant::now();
        let comps = components(&vd, &confirms, now);
        assert_eq!(comps.len(), 3);
        assert_eq!(comps[0].name, "Kokoro");
        assert_eq!(comps[0].memory, "598 MB");
        assert_eq!(comps[2].memory, "");
        assert_eq!(comps[2].tone, crate::ui::Tone::Normal);
        let buttons: Vec<PanelButton> = comps[0].buttons.iter().collect();
        assert_eq!(buttons.len(), 2);
        assert!(buttons[0].text.contains("Kokoro"));
        assert!(!buttons[0].armed);
        confirms.press(
            confirm::key("voicedock", "engine.stop", Some("kokoro")),
            true,
            now,
        );
        let armed = components(&vd, &confirms, now);
        let stop = armed[0]
            .buttons
            .iter()
            .find(|b| b.id == "engine.stop")
            .unwrap();
        assert!(stop.armed);
        assert_eq!(stop.text, "Confirm");
        assert!(!armed[1].buttons.iter().any(|b| b.armed));
    }

    #[test]
    fn processes_open_and_chart_follow_todays_words() {
        let vd = app("busy", "voicedock");
        let rows = processes(&vd);
        assert_eq!(rows.len(), vd.processes.len());
        let first: Vec<_> = rows[0].cells.iter().collect();
        assert_eq!(first.len(), 5);
        assert!(rows[0].label.starts_with("Process "));
        assert!(rows[0].label.contains("Memory "));
        assert!(
            rows[0].label.ends_with(", Verdict verified"),
            "{}",
            rows[0].label
        );
        let items = opens(&vd);
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].glyph, GlyphName::Gear);
        assert_eq!(items[1].glyph, GlyphName::File);
        assert_eq!(items[2].key, "menu:0");
        assert_eq!(items[2].glyph, GlyphName::Web);
        assert_eq!(items[3].glyph, GlyphName::Folder);
        let off = opens(&app("busy", "n8commentary"));
        assert!(off[0].enabled);
        assert!(!off[1].enabled);
        assert_eq!(off[1].tooltip, "Log: not there yet");
    }

    #[test]
    fn what_the_callbacks_ask_for_is_an_intent_by_ids_and_keys() {
        assert_eq!(
            action_intent("voicedock", "stop", ""),
            Intent::Action {
                app: "voicedock".into(),
                action: "stop".into(),
                component: None
            }
        );
        assert!(matches!(
            action_intent("voicedock", "engine.stop", "kokoro"),
            Intent::Action { component: Some(c), .. } if c == "kokoro"
        ));
        assert_eq!(
            open_intent("voicedock", "logs"),
            Intent::OpenItem {
                app: "voicedock".into(),
                kind: "open".into(),
                key: "logs".into()
            }
        );
        assert_eq!(
            open_intent("voicedock", "menu:1"),
            Intent::OpenItem {
                app: "voicedock".into(),
                kind: "menu".into(),
                key: "1".into()
            }
        );
        assert_eq!(
            setting_intent("voicedock", "hosted", false),
            Intent::Setting {
                key: "hosted".into(),
                on: false,
                app: Some("voicedock".into())
            }
        );
    }

    /// The panel of `busy`'s VoiceDock, whole, with Stop asking to confirm.
    fn focus_window() -> crate::view::gallery::ViewWindow {
        use crate::view::gallery::{Font, Look, view_window};
        let shot = Shot {
            app: "voicedock",
            height: WHOLE,
            ..Shot::of("focus", "busy")
        };
        let mut w = view_window(crate::ui::View::Panel, &shot, Look::Light, Font::Test).unwrap();
        for app in &mut w.snapshot.apps {
            for action in &mut app.actions {
                action.confirm = action.id == "stop" || action.id == "engine.stop";
            }
        }
        w.router.fill(&w.ui, &w.snapshot);
        w.draw().unwrap();
        crate::window::arrived(&w.ui);
        w.draw().unwrap();
        w
    }

    /// Presses Tab and says where the focus went: (x, top, in the list).
    fn tab(w: &crate::view::gallery::ViewWindow) -> (f32, f32, bool) {
        use slint::platform::{Key, WindowEvent};
        let text: slint::SharedString = Key::Tab.into();
        w.window
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        w.window.dispatch_event(WindowEvent::KeyReleased { text });
        w.draw().unwrap();
        let r = w.ui.focus_probe();
        (
            r.get_focused_x(),
            r.get_focused_top(),
            !r.get_focused_in_bar(),
        )
    }

    /// A refill (a new snapshot, or a click that arms an action) keeps the
    /// keyboard focus on the control that had it, in the title bar and in
    /// the list: Tab then goes on to the next control, as it would have.
    #[test]
    fn the_focus_stays_where_it_was_through_a_refill() {
        let w = focus_window();
        let stops: Vec<_> = (0..14).map(|_| tab(&w)).collect();
        w.ui.hide().unwrap();
        // Back, Stop, Restart, Close, then the list.
        assert!(!stops[1].2 && !stops[2].2, "{stops:?}");
        let first_in_list = stops.iter().position(|s| s.2).unwrap();
        for k in [1, first_in_list, first_in_list + 1, first_in_list + 3] {
            for arm in [false, true] {
                let mut w = focus_window();
                for _ in 0..=k {
                    tab(&w);
                }
                // The next snapshot: new figures everywhere.
                let vd = w
                    .snapshot
                    .apps
                    .iter_mut()
                    .find(|a| a.id == "voicedock")
                    .unwrap();
                vd.memory_mb = vd.memory_mb.map(|m| m + 1.0);
                for c in &mut vd.components {
                    c.memory_mb = c.memory_mb.map(|m| m + 1.0);
                }
                for p in &mut vd.processes {
                    p.rss_mb = p.rss_mb.map(|m| m + 1.0);
                }
                if arm {
                    let now = Instant::now();
                    w.router
                        .confirms
                        .press(confirm::key("voicedock", "stop", None), true, now);
                    w.router.confirms.press(
                        confirm::key("voicedock", "engine.stop", Some("kokoro")),
                        true,
                        now,
                    );
                }
                w.router.fill(&w.ui, &w.snapshot);
                w.draw().unwrap();
                let next = tab(&w);
                w.ui.hide().unwrap();
                assert_eq!(
                    next,
                    stops[k + 1],
                    "after stop {k} ({:?}, armed: {arm}) the focus was lost",
                    stops[k]
                );
            }
        }
    }

    /// The panel's bars never move its list, and its controls scroll into
    /// view (a low window, so the list scrolls).
    #[test]
    fn tab_through_the_panel() {
        let shot = Shot {
            app: "voicedock",
            height: 400,
            ..Shot::of("tab", "busy")
        };
        let stops = tab_through(crate::ui::View::Panel, &shot);
        assert!(stops.bottom > 0.0, "the list scrolled: {stops:?}");
        assert!(stops.in_bars >= 4, "{stops:?}");
        assert!(stops.in_list >= 10, "{stops:?}");
    }
}
