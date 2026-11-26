//! The tray icon and its native menu, built from the host's snapshot. The
//! menu's shape is rebuilt only when it changes (an app or action appears or
//! goes); otherwise only the texts and states that changed are updated, so a
//! menu the user has open does not flicker. The icon's art follows the
//! taskbar's colour, live (`n8tray_platform::theme`).

use std::collections::HashMap;

use n8tray_core::host::Command;
use n8tray_core::model::{ActionKindView, AppView, HostSnapshot, LevelName};
use n8tray_platform::theme::{self, Taskbar};
use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tracing::{info, warn};

use crate::state::AppState;

pub const TRAY_ID: &str = "n8tray";

/// One line of the menu.
#[derive(Debug, Clone, PartialEq)]
enum Line {
    Item {
        id: String,
        text: String,
        enabled: bool,
    },
    Check {
        id: String,
        text: String,
        enabled: bool,
        checked: bool,
    },
    Sub {
        id: String,
        text: String,
        children: Vec<Line>,
    },
    Separator,
}

impl Line {
    fn item(id: impl Into<String>, text: impl Into<String>, enabled: bool) -> Line {
        Line::Item {
            id: id.into(),
            text: text.into(),
            enabled,
        }
    }

    /// The line's identity, without its text and state.
    fn shape(&self, out: &mut Vec<String>) {
        match self {
            Line::Item { id, .. } | Line::Check { id, .. } => out.push(id.clone()),
            Line::Sub { id, children, .. } => {
                out.push(format!("{id}["));
                for child in children {
                    child.shape(out);
                }
                out.push("]".into());
            }
            Line::Separator => out.push("-".into()),
        }
    }
}

enum Handle {
    Item(MenuItem<Wry>),
    Check(CheckMenuItem<Wry>),
    Sub(Submenu<Wry>),
}

#[derive(Default)]
pub struct TrayState {
    shape: Vec<String>,
    lines: HashMap<String, Line>,
    handles: HashMap<String, Handle>,
    level: Option<LevelName>,
    taskbar: Taskbar,
    tooltip: String,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    // Where the platform cannot say, the art for a dark taskbar.
    let taskbar = theme::taskbar().unwrap_or_default();
    app.state::<AppState>().tray.lock().unwrap().taskbar = taskbar;
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "status", "N8 Tray: starting", false, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?,
        ],
    )?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(crate::badge::icon(taskbar, LevelName::Off))
        .tooltip("N8 Tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                crate::hub::toggle(tray.app_handle(), Some(rect));
            }
        })
        .build(app)?;
    let handle = app.clone();
    let watch = theme::watch_taskbar(move |report| match report {
        Ok(taskbar) => set_taskbar(&handle, taskbar),
        Err(e) => warn!("The tray icon stopped following the taskbar's colour: {e}"),
    });
    match watch {
        Ok(true) => info!("The tray icon follows the taskbar's colour ({taskbar:?} now)"),
        Ok(false) => info!("The tray icon uses the art for a {taskbar:?} taskbar"),
        Err(e) => warn!("The tray icon cannot follow the taskbar's colour: {e}"),
    }
    Ok(())
}

/// Switches the icon's art when the taskbar changes colour.
fn set_taskbar(app: &AppHandle, taskbar: Taskbar) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let state = app.state::<AppState>();
    let mut ours = state.tray.lock().unwrap();
    if ours.taskbar == taskbar {
        return;
    }
    info!("The taskbar is now {taskbar:?}");
    ours.taskbar = taskbar;
    let level = ours.level.unwrap_or(LevelName::Off);
    let _ = tray.set_icon(Some(crate::badge::icon(taskbar, level)));
}

fn memory_text(mb: Option<f64>) -> String {
    match mb {
        Some(mb) if mb >= 1000.0 => format!("{:.1} GB", mb / 1024.0),
        Some(mb) => format!("{mb:.0} MB"),
        None => String::new(),
    }
}

/// The menu's first line: "N8 Tray: Healthy · 310 MB in all", N8's own memory
/// (the tray and its apps), never the PC's (D34). The native shell's
/// `tray::status_line` says the same.
fn status_line(headline: &str, n8_mb: f64) -> String {
    format!("N8 Tray: {headline} · {} in all", memory_text(Some(n8_mb)))
}

fn app_lines(app: &AppView) -> Line {
    let mut text = format!(
        "{}: {}",
        app.name,
        app.pending.as_deref().unwrap_or(&app.words)
    );
    if let Some(mb) = app.memory_mb {
        text.push_str(&format!(" · {}", memory_text(Some(mb))));
    }
    let mut children = Vec::new();
    for action in &app.actions {
        let id = format!("app:{}:act:{}", app.id, action.id);
        let shown = match action.kind {
            ActionKindView::Start => !app.running,
            ActionKindView::Stop | ActionKindView::Restart => app.running,
            // Component actions need a component; they live in the window.
            ActionKindView::App => !action.per_component && !action.confirm,
        };
        if shown {
            children.push(Line::item(id, action.label.clone(), action.enabled));
        }
    }
    if !app.open.is_empty() || !app.menu.is_empty() {
        children.push(Line::Separator);
    }
    for open in &app.open {
        children.push(Line::item(
            format!("app:{}:open:{}", app.id, open.key),
            open.label.clone(),
            open.exists,
        ));
    }
    for (index, item) in app.menu.iter().enumerate() {
        children.push(Line::item(
            format!("app:{}:menu:{index}", app.id),
            item.label.clone(),
            item.exists,
        ));
    }
    if children.is_empty() {
        children.push(Line::item(
            format!("app:{}:none", app.id),
            app.summary.clone(),
            false,
        ));
    }
    Line::Sub {
        id: format!("app:{}", app.id),
        text,
        children,
    }
}

fn model(snapshot: &HostSnapshot) -> Vec<Line> {
    let mut lines = vec![
        Line::item(
            "status",
            status_line(&snapshot.headline, snapshot.memory.n8_mb),
            true,
        ),
        Line::Separator,
    ];
    let apps: Vec<&AppView> = snapshot
        .apps
        .iter()
        .filter(|a| a.hosted && a.level != LevelName::Hidden)
        .collect();
    for app in &apps {
        lines.push(app_lines(app));
    }
    if apps.is_empty() {
        lines.push(Line::item("noapps", "No apps installed", false));
    }
    for problem in &snapshot.problems {
        let name = problem.id.clone().unwrap_or_else(|| problem.file.clone());
        lines.push(Line::item(
            format!("problem:{}", problem.file),
            format!("{name}: {}", problem.message),
            false,
        ));
    }
    lines.push(Line::Separator);
    if let Some(version) = &snapshot.update.available {
        let u = &snapshot.update;
        let (text, enabled) = match u.install.as_str() {
            "downloading" => (
                match u.progress {
                    Some(p) => format!("Downloading N8 Tray {version}: {p}%"),
                    None => format!("Downloading N8 Tray {version}"),
                },
                false,
            ),
            "installing" => (format!("Installing N8 Tray {version}"), false),
            "failed" => (format!("Update to {version} failed: try again"), true),
            _ => (format!("Install update {version}"), true),
        };
        lines.push(Line::item("update", text, enabled));
    }
    lines.push(Line::Check {
        id: "signin".into(),
        text: "Start at sign-in".into(),
        enabled: !snapshot.settings.sign_in_foreign && snapshot.settings.error.is_none(),
        checked: snapshot.settings.launch_at_sign_in,
    });
    lines.push(Line::item("settings", "Open settings file", true));
    lines.push(Line::item("folder", "Open tray folder", true));
    lines.push(Line::item("log", "Open tray log", true));
    lines.push(Line::Separator);
    lines.push(Line::item("restart", "Restart N8 Tray", true));
    lines.push(Line::item("exit", "Exit (apps keep running)", true));
    lines
}

/// Brings the icon, tooltip and menu in line with a snapshot.
pub fn update(app: &AppHandle, snapshot: &HostSnapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let state = app.state::<AppState>();
    let mut ours = state.tray.lock().unwrap();
    if ours.level != Some(snapshot.level) {
        let _ = tray.set_icon(Some(crate::badge::icon(ours.taskbar, snapshot.level)));
        ours.level = Some(snapshot.level);
    }
    if ours.tooltip != snapshot.tooltip {
        let _ = tray.set_tooltip(Some(&snapshot.tooltip));
        ours.tooltip = snapshot.tooltip.clone();
    }

    let lines = model(snapshot);
    let mut shape = Vec::new();
    for line in &lines {
        line.shape(&mut shape);
    }
    if shape != ours.shape {
        match build(app, &lines) {
            Ok((menu, handles)) => {
                let _ = tray.set_menu(Some(menu));
                ours.handles = handles;
                ours.shape = shape;
                ours.lines = flatten(&lines);
            }
            Err(e) => warn!("The tray menu could not be built: {e}"),
        }
        return;
    }
    // Same shape: change only what changed.
    for (id, line) in flatten(&lines) {
        if ours.lines.get(&id) == Some(&line) {
            continue;
        }
        match (ours.handles.get(&id), &line) {
            (Some(Handle::Item(item)), Line::Item { text, enabled, .. }) => {
                let _ = item.set_text(text);
                let _ = item.set_enabled(*enabled);
            }
            (
                Some(Handle::Check(item)),
                Line::Check {
                    text,
                    enabled,
                    checked,
                    ..
                },
            ) => {
                let _ = item.set_text(text);
                let _ = item.set_enabled(*enabled);
                let _ = item.set_checked(*checked);
            }
            (Some(Handle::Sub(item)), Line::Sub { text, .. }) => {
                let _ = item.set_text(text);
            }
            _ => {}
        }
        ours.lines.insert(id, line);
    }
}

fn flatten(lines: &[Line]) -> HashMap<String, Line> {
    let mut out = HashMap::new();
    for line in lines {
        match line {
            Line::Item { id, .. } | Line::Check { id, .. } => {
                out.insert(id.clone(), line.clone());
            }
            Line::Sub { id, children, .. } => {
                // The submenu's own entry compares by text only.
                let mut own = line.clone();
                if let Line::Sub { children, .. } = &mut own {
                    children.clear();
                }
                out.insert(id.clone(), own);
                out.extend(flatten(children));
            }
            Line::Separator => {}
        }
    }
    out
}

type Built = (Menu<Wry>, HashMap<String, Handle>);

fn build(app: &AppHandle, lines: &[Line]) -> tauri::Result<Built> {
    let mut handles = HashMap::new();
    let items = build_items(app, lines, &mut handles)?;
    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|b| b.as_ref()).collect();
    Ok((Menu::with_items(app, &refs)?, handles))
}

fn build_items(
    app: &AppHandle,
    lines: &[Line],
    handles: &mut HashMap<String, Handle>,
) -> tauri::Result<Vec<Box<dyn IsMenuItem<Wry>>>> {
    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    for line in lines {
        match line {
            Line::Item { id, text, enabled } => {
                let item = MenuItem::with_id(app, id, text, *enabled, None::<&str>)?;
                handles.insert(id.clone(), Handle::Item(item.clone()));
                items.push(Box::new(item));
            }
            Line::Check {
                id,
                text,
                enabled,
                checked,
            } => {
                let item = CheckMenuItem::with_id(app, id, text, *enabled, *checked, None::<&str>)?;
                handles.insert(id.clone(), Handle::Check(item.clone()));
                items.push(Box::new(item));
            }
            Line::Sub { id, text, children } => {
                let built = build_items(app, children, handles)?;
                let refs: Vec<&dyn IsMenuItem<Wry>> = built.iter().map(|b| b.as_ref()).collect();
                let sub = Submenu::with_id_and_items(app, id, text, true, &refs)?;
                handles.insert(id.clone(), Handle::Sub(sub.clone()));
                items.push(Box::new(sub));
            }
            Line::Separator => items.push(Box::new(PredefinedMenuItem::separator(app)?)),
        }
    }
    Ok(items)
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref().to_string();
    let state = app.state::<AppState>();
    let snapshot = state.latest();
    let folders = n8tray_core::folders::Folders::from_env();
    match id.as_str() {
        "status" => crate::hub::show(app, None, Some("details")),
        "signin" => {
            let on = snapshot
                .as_ref()
                .is_some_and(|s| !s.settings.launch_at_sign_in);
            state.send(Command::SetLaunchAtSignIn(on));
        }
        "settings" => {
            if let Some(f) = &folders {
                if f.settings_file.exists() {
                    crate::open::open(&f.settings_file.display().to_string());
                } else {
                    crate::open::open(&f.base().display().to_string());
                }
            }
        }
        "folder" => {
            if let Some(f) = &folders {
                crate::open::open(&f.base().display().to_string());
            }
        }
        "log" => {
            if let Some(f) = &folders {
                match crate::logs::latest(&f.logs) {
                    Some(file) => crate::open::open(&file.display().to_string()),
                    None => crate::open::open(&f.logs.display().to_string()),
                }
            }
        }
        "update" => {
            if snapshot
                .as_ref()
                .is_some_and(|s| s.update.available.is_some())
            {
                crate::updater::install(app);
            }
        }
        "restart" => restart(app),
        "exit" => {
            info!("Exit from the tray menu; apps keep running");
            crate::state::shutdown(app);
            app.exit(0);
        }
        other => {
            let Some(rest) = other.strip_prefix("app:") else {
                return;
            };
            let mut parts = rest.splitn(3, ':');
            let (Some(app_id), Some(kind), Some(key)) = (parts.next(), parts.next(), parts.next())
            else {
                return;
            };
            match kind {
                "act" => state.send(Command::Action {
                    app: app_id.into(),
                    action: key.into(),
                    component: None,
                }),
                "open" | "menu" => {
                    let _ =
                        crate::commands::open_item(state, app_id.into(), kind.into(), key.into());
                }
                _ => {}
            }
        }
    }
}

/// Starts a new copy that waits for this one, then exits.
fn restart(app: &AppHandle) {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            warn!("Cannot restart: {e}");
            return;
        }
    };
    info!("Restarting N8 Tray");
    crate::state::shutdown(app);
    let spawned = std::process::Command::new(exe)
        .args(["--restart-after", &std::process::id().to_string()])
        .spawn();
    match spawned {
        Ok(_) => app.exit(0),
        Err(e) => warn!("Cannot restart: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::status_line;

    /// N8's own memory, said once: the line already starts with "N8 Tray"
    /// (D34). The native shell's test checks the same words.
    #[test]
    fn the_status_line_says_n8s_memory_once() {
        assert_eq!(
            status_line("Healthy", 310.0),
            "N8 Tray: Healthy · 310 MB in all"
        );
        assert_eq!(
            status_line("Needs attention", 1950.0),
            "N8 Tray: Needs attention · 1.9 GB in all"
        );
    }
}
