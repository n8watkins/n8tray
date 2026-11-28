//! The hub (copies `Hub.tsx`, `HubHead`, `HubFoot` and `UpdateBanner.tsx`;
//! lane: hub screen, which owns this file, `ui/views/hub.slint`,
//! `ui/views/hub-model.slint` and `native/tests/goldens/view-hub/`).
//!
//! `fill` turns the snapshot into the `HubModel`: the title bar's pill and
//! summary, the amber reason, the update banner, the main view's apps (one
//! stable model, updated in place by app id), the manifest problems, the
//! more line, N8 Tray's own row and the bottom bar. `wire` connects what the
//! view asks of the shell.
#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use n8tray_core::model::{HostSnapshot, TrayUsage};
use slint::{Model, ModelRc, SharedString, VecModel};

use super::format::{self, UpdateAction};
use super::icons;
use super::intent::{self, HostTarget, Intent};
use super::list;
use super::router::{Ctx, Models};
use super::rows;
use crate::ui::{BarLink, GlyphName, HubProblem, Level, RowData, Tone};

/// The view's golden images (`router::Shot`): add a state here.
#[cfg(debug_assertions)]
pub const SHOTS: &[super::router::Shot] = &[
    shot("busy", "busy"),
    shot("healthy", "healthy"),
    shot("trouble", "trouble"),
    // The same in the tall window: all of it, N8 Tray's own row included,
    // as today's window (no height limit) shows it.
    super::router::Shot {
        height: 600,
        ..shot("trouble-tall", "trouble")
    },
    shot("watching", "watching"),
    shot("updating", "updating"),
    shot("update-failed", "updateFailed"),
    shot("pinned", "pinned"),
    shot("empty", "empty"),
    shot("scroll", "compactTall"),
    super::router::Shot {
        stage: Some(stage_armed),
        ..shot("armed", "busy")
    },
    super::router::Shot {
        stage: Some(stage_sign_in_foreign),
        ..shot("sign-in-foreign", "healthy")
    },
    super::router::Shot {
        compact: true,
        ..shot("compact", "busy")
    },
    super::router::Shot {
        compact: true,
        ..shot("compact-trouble", "trouble")
    },
    super::router::Shot {
        compact: true,
        ..shot("compact-pinned", "pinned")
    },
    super::router::Shot {
        compact: true,
        height: 600,
        ..shot("compact-tall", "compactTall")
    },
];

/// A shot in Normal rows with the real app icons.
#[cfg(debug_assertions)]
const fn shot(state: &'static str, fixture: &'static str) -> super::router::Shot {
    super::router::Shot {
        stage: Some(stage_icons),
        ..super::router::Shot::of(state, fixture)
    }
}

/// The fixtures name their icons relative to the spec's examples; a picture
/// reads them from there, as the real window reads an absolute path.
#[cfg(debug_assertions)]
fn stage_icons(stage: &mut super::router::Stage) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../spec/examples/icons");
    for app in &mut stage.snapshot.apps {
        let file = app
            .icon
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_name());
        if let Some(file) = file {
            app.icon = Some(dir.join(file).to_string_lossy().into_owned());
        }
    }
}

/// Stop pressed once on VoiceDock: its button says Confirm.
#[cfg(debug_assertions)]
fn stage_armed(stage: &mut super::router::Stage) {
    stage_icons(stage);
    stage.confirms.press(
        super::confirm::key("voicedock", "stop", None),
        true,
        std::time::Instant::now(),
    );
}

/// A sign-in entry that another program made: the box is disabled.
#[cfg(debug_assertions)]
fn stage_sign_in_foreign(stage: &mut super::router::Stage) {
    stage_icons(stage);
    stage.snapshot.settings.sign_in_foreign = true;
}

/// N8 Tray's own row id, so it stays in the arrow-key order.
pub const TRAY_ID: &str = "n8tray";

/// What the sign-in box says when another program made the entry; `name`
/// is the product's name as the shell shows it.
fn sign_in_foreign(name: &str) -> String {
    format!("Another program made the sign-in entry; {name} leaves it alone")
}

/// Shown while the hub has no app at all.
const EMPTY: &str = "No apps yet. An app appears here once it installs its manifest.";

thread_local! {
    /// The id of the action each row's button runs, as the last fill drew
    /// it (none while watching): a button press carries only the app's id.
    static ACTIONS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// N8 Tray's own row: "This app", its private memory and its window's, as
/// today's `TrayRow`. `name` is the product's name as the shell shows it.
pub fn tray_row(t: &TrayUsage, name: &str) -> RowData {
    let memory = format::memory(Some(t.private_mb.unwrap_or(t.rss_mb)));
    let sub = if t.window_mb > 0.0 {
        format!(
            "window {}",
            format::memory(Some(t.window_private_mb.unwrap_or(t.window_mb)))
        )
    } else {
        String::new()
    };
    let spark = match &t.history {
        Some(h) if format::has_history(Some(&h.memory_mb)) => format::spark(&h.memory_mb),
        _ => format::Drawing::default(),
    };
    let line = "This app";
    RowData {
        id: TRAY_ID.into(),
        name: name.into(),
        line: line.into(),
        tone: Tone::Normal,
        memory: memory.as_str().into(),
        sub: sub.as_str().into(),
        spark_line: spark.line.into(),
        spark_fill: spark.fill.into(),
        level: Level::Ok,
        icon: icons::n8tray(),
        label: SharedString::from(rows::row_label(name, "", line, &memory, &sub)),
        ..RowData::default()
    }
}

/// The main view's rows: the apps `format::main_view_apps` keeps, each as
/// today's `Row`, without an action while the PowerShell tray manages them.
/// With each row, the id of the action its button runs, the same one
/// `rows::row_data` drew and armed.
fn app_rows(ctx: &mut Ctx) -> Vec<(RowData, Option<String>)> {
    let s = ctx.snapshot;
    format::main_view_apps(&s.apps)
        .into_iter()
        .map(|a| {
            let icon = ctx.icons.get(a.icon.as_deref());
            let row = rows::row_data(a, icon, ctx.confirms, s.legacy_tray, ctx.now);
            let action = if s.legacy_tray {
                None
            } else {
                format::row_action(a).map(|x| x.id.clone())
            };
            (row, action)
        })
        .collect()
}

/// The manifests that could not be read, after the apps.
pub fn problems(s: &HostSnapshot) -> Vec<HubProblem> {
    s.problems
        .iter()
        .map(|p| HubProblem {
            name: p.id.as_deref().unwrap_or(&p.file).into(),
            message: p.message.as_str().into(),
        })
        .collect()
}

/// The bottom bar's links, History then Log.
fn links() -> Vec<BarLink> {
    vec![
        BarLink {
            key: "history".into(),
            glyph: GlyphName::History,
            text: "History".into(),
            tooltip: SharedString::default(),
            enabled: true,
        },
        BarLink {
            key: "log".into(),
            glyph: GlyphName::File,
            text: "Log".into(),
            tooltip: SharedString::default(),
            enabled: true,
        },
    ]
}

/// Puts `items` into the model the view already holds, in place by app id
/// (`list::sync`), and the focus back on its row when rows moved.
fn sync_rows(ui: &dyn Models, items: Vec<RowData>) {
    let model = ui.hub();
    let current = model.get_rows();
    let Some(stable) = current.as_any().downcast_ref::<VecModel<RowData>>() else {
        model.set_rows(ModelRc::from(Rc::new(VecModel::from(items))));
        return;
    };
    let focus = ui.row_focus();
    let focused = rows::focused(&focus);
    let done = list::sync(stable, items, |r| r.id.clone());
    if done.moved {
        rows::refocus(&focus, &focused, stable);
    }
}

pub fn fill(ui: &dyn Models, ctx: &mut Ctx) {
    let s = ctx.snapshot;
    let model = ui.hub();
    model.set_level(rows::level(s.level));
    model.set_level_text(format::level_words(s.level).into());
    model.set_summary(
        format::title_summary(s.pinned_running, s.pinned_total, s.memory.n8_mb).into(),
    );
    // The reason only: the summary stays in the title bar.
    model.set_alert(s.reason.as_deref().unwrap_or_default().into());
    model.set_watching(s.legacy_tray);

    match format::update_line(&s.update) {
        Some(line) => {
            model.set_update_shown(true);
            model.set_update_problem(line.problem);
            model.set_update_text(line.text.as_str().into());
            model.set_update_detail(line.detail.unwrap_or_default().into());
            model.set_update_progress(line.progress.map_or(-1.0, |p| p as f32));
            model.set_update_secondary(
                match (&s.update.url, line.problem) {
                    (None, _) => "",
                    (Some(_), true) => "Release page",
                    (Some(_), false) => "What's new",
                }
                .into(),
            );
            model.set_update_primary(
                match line.action {
                    None => "",
                    Some(UpdateAction::Retry) => "Try again",
                    Some(UpdateAction::Install) => "Install",
                }
                .into(),
            );
        }
        None => {
            model.set_update_shown(false);
            model.set_update_problem(false);
            model.set_update_text("".into());
            model.set_update_detail("".into());
            model.set_update_progress(-1.0);
            model.set_update_secondary("".into());
            model.set_update_primary("".into());
        }
    }

    let (items, actions): (Vec<RowData>, Vec<Option<String>>) = app_rows(ctx).into_iter().unzip();
    ACTIONS.with_borrow_mut(|map| {
        map.clear();
        for (row, action) in items.iter().zip(actions) {
            if let Some(action) = action {
                map.insert(row.id.to_string(), action);
            }
        }
    });
    sync_rows(ui, items);
    model.set_problems(ModelRc::from(Rc::new(VecModel::from(problems(s)))));
    model.set_more(
        s.more
            .as_ref()
            .map(|m| m.text.as_str())
            .unwrap_or("")
            .into(),
    );
    let none_at_all = s.more.is_none() && format::main_view_apps(&s.apps).is_empty();
    model.set_empty(if none_at_all { EMPTY } else { "" }.into());
    // The product's name is the shell's: the hub's title (`Nav.title`).
    let name = ui.nav().get_title();
    model.set_tray(tray_row(&s.tray, &name));

    let settings = &s.settings;
    model.set_sign_in_text("Start at sign-in".into());
    model.set_sign_in_checked(settings.launch_at_sign_in);
    model.set_sign_in_enabled(!(settings.sign_in_foreign || settings.error.is_some()));
    model.set_sign_in_tooltip(if settings.sign_in_foreign {
        sign_in_foreign(&name).into()
    } else {
        SharedString::default()
    });
}

pub fn wire(ui: &dyn Models) {
    let model = ui.hub();
    model.on_act(|id| {
        let action = ACTIONS.with_borrow(|actions| actions.get(id.as_str()).cloned());
        if let Some(action) = action {
            intent::send(Intent::Action {
                app: id.to_string(),
                action,
                component: None,
            });
        }
    });
    // History then Log, set once: links made anew on every fill would take
    // the keyboard focus from them at each refresh.
    model.set_links(ModelRc::from(Rc::new(VecModel::from(links()))));
    model.on_update_secondary_clicked(|| intent::send(Intent::OpenHost(HostTarget::ReleasePage)));
    model.on_update_primary_clicked(|| intent::send(Intent::InstallUpdate));
    model.on_toggle_sign_in(|| intent::send(Intent::ToggleSignIn));
    model.on_open_log(|| intent::send(Intent::OpenHost(HostTarget::Log)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;
    #[cfg(debug_assertions)]
    use slint::ComponentHandle;

    #[test]
    fn the_tray_row_says_this_app_with_its_private_and_window_memory() {
        let mut s = fixture("busy");
        s.tray.private_mb = Some(13.4);
        s.tray.rss_mb = 40.0;
        s.tray.window_mb = 120.0;
        s.tray.window_private_mb = Some(88.2);
        let row = tray_row(&s.tray, "N8 Tray");
        assert_eq!(row.id, TRAY_ID);
        assert_eq!(row.line, "This app");
        assert_eq!(row.memory, "13 MB");
        assert_eq!(row.sub, "window 88 MB");
        assert_eq!(row.label, "N8 Tray, This app, 13 MB, window 88 MB");
        assert_eq!(row.level, Level::Ok);
        // Without a window, and without a private figure, as today.
        s.tray.window_mb = 0.0;
        s.tray.private_mb = None;
        let row = tray_row(&s.tray, "N8 Tray");
        assert_eq!(row.memory, "40 MB");
        assert_eq!(row.sub, "");
    }

    #[test]
    fn a_manifest_problem_is_named_by_its_app_or_else_its_file() {
        let s = fixture("trouble");
        assert!(!s.problems.is_empty());
        let shown = problems(&s);
        assert_eq!(shown.len(), s.problems.len());
        let p = &s.problems[0];
        assert_eq!(
            shown[0].name,
            p.id.as_deref().unwrap_or(&p.file),
            "the id, else the file"
        );
        assert_eq!(shown[0].message, p.message.as_str());
    }

    #[test]
    fn the_main_view_filter_shows_only_the_apps_in_it() {
        let s = fixture("pinned");
        let shown: Vec<&str> = format::main_view_apps(&s.apps)
            .into_iter()
            .map(|a| a.id.as_str())
            .collect();
        assert!(
            shown.len() < s.apps.iter().filter(|a| a.hosted).count(),
            "the pinned fixture has apps outside the main view"
        );
        assert!(s.more.is_some());
    }

    /// The README's keyboard test on a hub whose list scrolls: the bars'
    /// controls (Settings, Close, the summary, the sign-in box, History,
    /// Log) never move the list; every row, its button and the more line
    /// take the focus inside it.
    #[cfg(debug_assertions)]
    #[test]
    fn tab_goes_through_the_bars_and_the_scrolling_list() {
        use super::super::gallery::tests::tab_through;
        use crate::ui::View;
        let shot = SHOTS.iter().find(|s| s.state == "scroll").unwrap();
        let stops = tab_through(View::Hub, shot);
        assert!(stops.bottom > 0.0, "the list scrolls: {stops:?}");
        assert_eq!(stops.in_bars, 6, "{stops:?}");
        // Twelve apps with a button each, then N8 Tray's row.
        assert_eq!(stops.in_list, 25, "{stops:?}");
    }

    /// At the default height the trouble hub is taller than the window: N8
    /// Tray's own row is below the fold, not gone. Tab reaches it and the
    /// list scrolls to it; every shot fills it.
    #[cfg(debug_assertions)]
    #[test]
    fn n8_trays_own_row_is_in_every_shot_and_reachable_below_the_fold() {
        use super::super::gallery::tests::tab_through;
        use super::super::gallery::{Font, Look, view_window};
        use crate::ui::View;
        for shot in SHOTS {
            let w = view_window(View::Hub, shot, Look::Light, Font::Test).unwrap();
            let tray = w.ui.hub().get_tray();
            assert_eq!(tray.id, TRAY_ID, "{}", shot.state);
            assert_eq!(tray.line, "This app", "{}", shot.state);
            assert!(!tray.name.is_empty(), "{}", shot.state);
            w.ui.hide().unwrap();
        }
        let shot = SHOTS.iter().find(|s| s.state == "trouble").unwrap();
        let stops = tab_through(View::Hub, shot);
        assert!(stops.bottom > 0.0, "the trouble hub scrolls: {stops:?}");
        // The update banner's two buttons, three apps, the two whose button
        // can be pressed (Restarting disables N8 Hotkeys' Stop; the problem
        // row takes no focus), then N8 Tray's row.
        assert_eq!(stops.in_list, 8, "{stops:?}");
    }

    /// A row's button runs the action its row drew, by the action's id, and
    /// none while the PowerShell tray manages the apps. The sign-in box's
    /// tooltip is today's.
    #[cfg(debug_assertions)]
    #[test]
    fn a_rows_button_runs_the_action_it_shows_by_id() {
        use super::super::gallery::{Font, Look, view_window};
        use crate::ui::View;
        let action = |id: &str| ACTIONS.with_borrow(|a| a.get(id).cloned());
        let busy = SHOTS.iter().find(|s| s.state == "busy").unwrap();
        let w = view_window(View::Hub, busy, Look::Light, Font::Test).unwrap();
        for app in format::main_view_apps(&w.snapshot.apps) {
            assert_eq!(
                action(&app.id),
                format::row_action(app).map(|a| a.id.clone()),
                "{}",
                app.id
            );
        }
        assert_eq!(action("voicedock").as_deref(), Some("stop"));
        assert_eq!(action("n8commentary").as_deref(), Some("start"));
        w.ui.hide().unwrap();

        let watching = SHOTS.iter().find(|s| s.state == "watching").unwrap();
        let w = view_window(View::Hub, watching, Look::Light, Font::Test).unwrap();
        assert!(w.snapshot.legacy_tray);
        assert!(
            ACTIONS.with_borrow(HashMap::is_empty),
            "nothing runs while watching"
        );
        w.ui.hide().unwrap();

        let foreign = SHOTS.iter().find(|s| s.state == "sign-in-foreign").unwrap();
        let w = view_window(View::Hub, foreign, Look::Light, Font::Test).unwrap();
        let hub = w.ui.hub();
        assert!(!hub.get_sign_in_enabled());
        assert_eq!(
            hub.get_sign_in_tooltip(),
            "Another program made the sign-in entry; N8 Tray leaves it alone"
        );
        w.ui.hide().unwrap();
    }

    /// A refresh keeps the keyboard focus on the bottom bar's links: they
    /// are not made anew on every fill.
    #[cfg(debug_assertions)]
    #[test]
    fn a_refresh_keeps_the_focus_on_a_bottom_bar_link() {
        use super::super::gallery::{Font, Look, view_window};
        use crate::ui::View;
        use slint::platform::{Key, WindowEvent};
        let shot = SHOTS.iter().find(|s| s.state == "healthy").unwrap();
        let mut w = view_window(View::Hub, shot, Look::Light, Font::Test).unwrap();
        w.draw().unwrap();
        let height = w.window.size().height as f32;
        let reveal = w.ui.focus_probe();
        let tab = |w: &super::super::gallery::ViewWindow| {
            let text: SharedString = Key::Tab.into();
            w.window
                .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            w.window.dispatch_event(WindowEvent::KeyReleased { text });
            w.draw().unwrap();
        };
        // Tab to the bottom bar: the sign-in box, then History.
        let mut in_foot = 0;
        for _ in 0..40 {
            tab(&w);
            if reveal.get_focused_in_bar() && reveal.get_focused_top() > height - 40.0 {
                in_foot += 1;
                if in_foot == 2 {
                    break;
                }
            }
        }
        assert_eq!(in_foot, 2, "Tab reached History");
        let history_x = reveal.get_focused_x();
        // A new snapshot arrives while History has the focus.
        let snapshot = w.snapshot.clone();
        w.router.fill(&w.ui, &snapshot);
        w.draw().unwrap();
        let serial = reveal.get_focused_serial();
        tab(&w);
        // A Tab that lands on the view's top reports nothing (the serial
        // stays): the focus had gone back to the top with the old links.
        assert_ne!(reveal.get_focused_serial(), serial, "Tab reached a control");
        assert!(
            reveal.get_focused_top() > height - 40.0 && reveal.get_focused_x() > history_x,
            "Tab after a refresh goes on from History to Log, not from the top: at {}, {}",
            reveal.get_focused_x(),
            reveal.get_focused_top()
        );
        w.ui.hide().unwrap();
    }
}
