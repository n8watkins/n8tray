//! The tray icon and its native menu, built from the host's snapshot: a port
//! of the Tauri app's tray (app/src-tauri/src/tray.rs) to `tray-icon` and
//! `muda`, with the same items, order, labels and states. The menu's shape
//! is rebuilt only when it changes (an app or action appears or goes);
//! otherwise only the texts and states that changed are updated, so a menu
//! the user has open does not flicker (D17). The icon's art follows the
//! taskbar's colour (D30).

use std::collections::HashMap;

use n8tray_core::model::{ActionKindView, AppView, HostSnapshot, LevelName};
use n8tray_platform::theme::Taskbar;
use tracing::{info, warn};
use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};

/// One line of the menu.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
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

/// The identity of a whole menu: equal shapes update in place.
pub fn shape(lines: &[Line]) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines {
        line.shape(&mut out);
    }
    out
}

/// The menu before the host's first snapshot.
pub fn starting() -> Vec<Line> {
    vec![
        Line::item("status", "N8 Tray: starting", false),
        Line::Separator,
        Line::item("exit", "Exit", true),
    ]
}

fn memory_text(mb: Option<f64>) -> String {
    match mb {
        Some(mb) if mb >= 1000.0 => format!("{:.1} GB", mb / 1024.0),
        Some(mb) => format!("{mb:.0} MB"),
        None => String::new(),
    }
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

/// The menu for a snapshot.
pub fn model(snapshot: &HostSnapshot) -> Vec<Line> {
    let mut lines = vec![
        Line::item(
            "status",
            format!("N8 Tray: {} · {}", snapshot.headline, snapshot.memory.text),
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

/// Every line with an id, by id. A submenu's own entry compares by text only.
pub fn flatten(lines: &[Line]) -> HashMap<String, Line> {
    let mut out = HashMap::new();
    for line in lines {
        match line {
            Line::Item { id, .. } | Line::Check { id, .. } => {
                out.insert(id.clone(), line.clone());
            }
            Line::Sub { id, children, .. } => {
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

/// What a click on a menu line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Clicked {
    /// The status line: the window on its details view.
    Status,
    SignIn,
    Settings,
    Folder,
    Log,
    Update,
    Restart,
    Exit,
    /// An action of an app (`app:<id>:act:<action>`).
    Action {
        app: String,
        action: String,
    },
    /// An app's `open` entry or `menu` line (`app:<id>:open:<key>`, `app:<id>:menu:<n>`).
    Open {
        app: String,
        kind: String,
        key: String,
    },
    /// A line that does nothing (a disabled or informative one).
    Nothing,
}

/// The click for a menu line's id.
pub fn clicked(id: &str) -> Clicked {
    match id {
        "status" => Clicked::Status,
        "signin" => Clicked::SignIn,
        "settings" => Clicked::Settings,
        "folder" => Clicked::Folder,
        "log" => Clicked::Log,
        "update" => Clicked::Update,
        "restart" => Clicked::Restart,
        "exit" => Clicked::Exit,
        other => {
            let Some(rest) = other.strip_prefix("app:") else {
                return Clicked::Nothing;
            };
            let mut parts = rest.splitn(3, ':');
            let (Some(app), Some(kind), Some(key)) = (parts.next(), parts.next(), parts.next())
            else {
                return Clicked::Nothing;
            };
            match kind {
                "act" => Clicked::Action {
                    app: app.into(),
                    action: key.into(),
                },
                "open" | "menu" => Clicked::Open {
                    app: app.into(),
                    kind: kind.into(),
                    key: key.into(),
                },
                _ => Clicked::Nothing,
            }
        }
    }
}

enum Handle {
    Item(MenuItem),
    Check(CheckMenuItem),
    Sub(Submenu),
}

/// The tray icon and what it shows now.
pub struct Tray {
    icon: TrayIcon,
    shape: Vec<String>,
    lines: HashMap<String, Line>,
    handles: HashMap<String, Handle>,
    level: LevelName,
    taskbar: Taskbar,
    tooltip: String,
}

impl Tray {
    /// Puts the icon in the notification area with the starting menu.
    pub fn create(taskbar: Taskbar) -> Result<Tray, String> {
        let lines = starting();
        let (menu, handles) = build(&lines).map_err(|e| e.to_string())?;
        let mut builder = TrayIconBuilder::new()
            .with_id("n8tray")
            .with_tooltip("N8 Tray")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false);
        if let Some(icon) = crate::badge::tray_icon(taskbar, LevelName::Off) {
            builder = builder.with_icon(icon);
        }
        let icon = builder.build().map_err(|e| e.to_string())?;
        Ok(Tray {
            icon,
            shape: shape(&lines),
            lines: flatten(&lines),
            handles,
            level: LevelName::Off,
            taskbar,
            tooltip: "N8 Tray".into(),
        })
    }

    /// The icon's rectangle on screen, in physical pixels.
    pub fn rect(&self) -> Option<n8tray_core::placement::Rect> {
        self.icon.rect().map(physical)
    }

    /// Switches the icon's art when the taskbar changes colour.
    pub fn set_taskbar(&mut self, taskbar: Taskbar) {
        if self.taskbar == taskbar {
            return;
        }
        info!("The taskbar is now {taskbar:?}");
        self.taskbar = taskbar;
        self.set_icon();
    }

    fn set_icon(&self) {
        if let Err(e) = self
            .icon
            .set_icon(crate::badge::tray_icon(self.taskbar, self.level))
        {
            warn!("The tray icon could not be changed: {e}");
        }
    }

    /// Brings the icon, tooltip and menu in line with a snapshot.
    pub fn update(&mut self, snapshot: &HostSnapshot) {
        if self.level != snapshot.level {
            self.level = snapshot.level;
            self.set_icon();
        }
        if self.tooltip != snapshot.tooltip {
            let _ = self.icon.set_tooltip(Some(&snapshot.tooltip));
            self.tooltip = snapshot.tooltip.clone();
        }
        let lines = model(snapshot);
        let new_shape = shape(&lines);
        if new_shape != self.shape {
            match build(&lines) {
                Ok((menu, handles)) => {
                    self.icon.set_menu(Some(Box::new(menu)));
                    self.handles = handles;
                    self.shape = new_shape;
                    self.lines = flatten(&lines);
                }
                Err(e) => warn!("The tray menu could not be built: {e}"),
            }
            return;
        }
        // Same shape: change only what changed.
        for (id, line) in flatten(&lines) {
            if self.lines.get(&id) == Some(&line) {
                continue;
            }
            match (self.handles.get(&id), &line) {
                (Some(Handle::Item(item)), Line::Item { text, enabled, .. }) => {
                    item.set_text(text);
                    item.set_enabled(*enabled);
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
                    item.set_text(text);
                    item.set_enabled(*enabled);
                    item.set_checked(*checked);
                }
                (Some(Handle::Sub(item)), Line::Sub { text, .. }) => item.set_text(text),
                _ => {}
            }
            self.lines.insert(id, line);
        }
    }
}

/// `tray-icon` reports physical pixels.
fn physical(rect: tray_icon::Rect) -> n8tray_core::placement::Rect {
    n8tray_core::placement::Rect::new(
        rect.position.x.round() as i32,
        rect.position.y.round() as i32,
        rect.size.width as i32,
        rect.size.height as i32,
    )
}

/// The rectangle a click event carries, in physical pixels.
pub fn click_rect(rect: tray_icon::Rect) -> n8tray_core::placement::Rect {
    physical(rect)
}

type Built = (Menu, HashMap<String, Handle>);

fn build(lines: &[Line]) -> tray_icon::menu::Result<Built> {
    let mut handles = HashMap::new();
    let items = build_items(lines, &mut handles)?;
    let refs: Vec<&dyn IsMenuItem> = items.iter().map(|b| b.as_ref()).collect();
    Ok((Menu::with_items(&refs)?, handles))
}

fn build_items(
    lines: &[Line],
    handles: &mut HashMap<String, Handle>,
) -> tray_icon::menu::Result<Vec<Box<dyn IsMenuItem>>> {
    let mut items: Vec<Box<dyn IsMenuItem>> = Vec::new();
    for line in lines {
        match line {
            Line::Item { id, text, enabled } => {
                let item = MenuItem::with_id(id.as_str(), text, *enabled, None);
                handles.insert(id.clone(), Handle::Item(item.clone()));
                items.push(Box::new(item));
            }
            Line::Check {
                id,
                text,
                enabled,
                checked,
            } => {
                let item = CheckMenuItem::with_id(id.as_str(), text, *enabled, *checked, None);
                handles.insert(id.clone(), Handle::Check(item.clone()));
                items.push(Box::new(item));
            }
            Line::Sub { id, text, children } => {
                let built = build_items(children, handles)?;
                let refs: Vec<&dyn IsMenuItem> = built.iter().map(|b| b.as_ref()).collect();
                let sub = Submenu::with_id_and_items(id.as_str(), text, true, &refs)?;
                handles.insert(id.clone(), Handle::Sub(sub.clone()));
                items.push(Box::new(sub));
            }
            Line::Separator => items.push(Box::new(PredefinedMenuItem::separator())),
        }
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    fn texts(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| match l {
                Line::Item { text, .. } | Line::Check { text, .. } | Line::Sub { text, .. } => {
                    text.clone()
                }
                Line::Separator => "-".into(),
            })
            .collect()
    }

    /// The line with this id, at any depth.
    fn find<'a>(lines: &'a [Line], wanted: &str) -> Option<&'a Line> {
        for line in lines {
            match line {
                Line::Item { id, .. } | Line::Check { id, .. } if id == wanted => {
                    return Some(line);
                }
                Line::Sub { id, children, .. } => {
                    if id == wanted {
                        return Some(line);
                    }
                    if let Some(found) = find(children, wanted) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }

    #[test]
    fn the_menu_has_todays_lines_in_todays_order() {
        let s = fixture("healthy");
        let lines = model(&s);
        let t = texts(&lines);
        assert_eq!(
            t[0],
            format!("N8 Tray: {} · {}", s.headline, s.memory.text),
            "the status line"
        );
        assert_eq!(t[1], "-");
        let hosted: Vec<_> = s
            .apps
            .iter()
            .filter(|a| a.hosted && a.level != LevelName::Hidden)
            .collect();
        for (i, app) in hosted.iter().enumerate() {
            assert!(t[2 + i].starts_with(&format!("{}: ", app.name)), "{t:?}");
        }
        let tail: Vec<&str> = t[t.len() - 7..].iter().map(String::as_str).collect();
        assert_eq!(
            tail,
            [
                "Start at sign-in",
                "Open settings file",
                "Open tray folder",
                "Open tray log",
                "-",
                "Restart N8 Tray",
                "Exit (apps keep running)"
            ]
        );
    }

    #[test]
    fn an_app_line_shows_its_state_and_memory() {
        let mut s = fixture("healthy");
        s.apps[0].hosted = true;
        s.apps[0].level = LevelName::Ok;
        s.apps[0].memory_mb = Some(1536.0);
        s.apps[0].pending = None;
        let words = s.apps[0].words.clone();
        let name = s.apps[0].name.clone();
        let id = format!("app:{}", s.apps[0].id);
        let lines = model(&s);
        let Some(Line::Sub { text, .. }) = find(&lines, &id) else {
            panic!("no submenu for {id}");
        };
        assert_eq!(text, &format!("{name}: {words} · 1.5 GB"));
        s.apps[0].memory_mb = Some(84.4);
        s.apps[0].pending = Some("Stopping".into());
        let lines = model(&s);
        let Some(Line::Sub { text, .. }) = find(&lines, &id) else {
            panic!("no submenu for {id}");
        };
        assert_eq!(text, &format!("{name}: Stopping · 84 MB"));
    }

    #[test]
    fn start_shows_only_when_stopped_and_stop_only_when_running() {
        let mut s = fixture("healthy");
        let app = &mut s.apps[0];
        app.hosted = true;
        app.level = LevelName::Ok;
        let id = app.id.clone();
        let has_kind = |s: &HostSnapshot, kind: &str| {
            let lines = model(s);
            let action = s.apps[0]
                .actions
                .iter()
                .find(|a| format!("{:?}", a.kind).eq_ignore_ascii_case(kind))
                .map(|a| a.id.clone());
            action.is_some_and(|a| find(&lines, &format!("app:{id}:act:{a}")).is_some())
        };
        s.apps[0].running = true;
        assert!(!has_kind(&s, "start"));
        assert!(has_kind(&s, "stop") && has_kind(&s, "restart"));
        s.apps[0].running = false;
        assert!(has_kind(&s, "start"));
        assert!(!has_kind(&s, "stop") && !has_kind(&s, "restart"));
    }

    #[test]
    fn hidden_and_own_icon_apps_are_left_out() {
        let mut s = fixture("healthy");
        for app in &mut s.apps {
            app.hosted = false;
        }
        let lines = model(&s);
        assert!(find(&lines, "noapps").is_some());
        assert_eq!(
            find(&lines, "noapps"),
            Some(&Line::item("noapps", "No apps installed", false))
        );
        let mut s = fixture("healthy");
        s.apps[0].level = LevelName::Hidden;
        let lines = model(&s);
        assert!(find(&lines, &format!("app:{}", s.apps[0].id)).is_none());
    }

    #[test]
    fn an_app_with_nothing_to_do_shows_its_summary() {
        let mut s = fixture("healthy");
        let app = &mut s.apps[0];
        app.hosted = true;
        app.level = LevelName::Ok;
        app.actions.clear();
        app.open.clear();
        app.menu.clear();
        let (id, summary) = (app.id.clone(), app.summary.clone());
        let lines = model(&s);
        assert_eq!(
            find(&lines, &format!("app:{id}:none")),
            Some(&Line::item(format!("app:{id}:none"), summary, false))
        );
    }

    #[test]
    fn the_update_line_follows_the_install() {
        let mut s = fixture("healthy");
        s.update.available = Some("0.6.0".into());
        let line = |s: &HostSnapshot| find(&model(s), "update").cloned();
        s.update.install = "idle".into();
        assert_eq!(
            line(&s),
            Some(Line::item("update", "Install update 0.6.0", true))
        );
        s.update.install = "downloading".into();
        s.update.progress = Some(42);
        assert_eq!(
            line(&s),
            Some(Line::item(
                "update",
                "Downloading N8 Tray 0.6.0: 42%",
                false
            ))
        );
        s.update.progress = None;
        assert_eq!(
            line(&s),
            Some(Line::item("update", "Downloading N8 Tray 0.6.0", false))
        );
        s.update.install = "installing".into();
        assert_eq!(
            line(&s),
            Some(Line::item("update", "Installing N8 Tray 0.6.0", false))
        );
        s.update.install = "failed".into();
        assert_eq!(
            line(&s),
            Some(Line::item(
                "update",
                "Update to 0.6.0 failed: try again",
                true
            ))
        );
        s.update.available = None;
        assert_eq!(line(&s), None);
    }

    #[test]
    fn sign_in_is_checked_and_enabled_from_the_settings() {
        let mut s = fixture("healthy");
        let check = |s: &HostSnapshot| match find(&model(s), "signin") {
            Some(Line::Check {
                enabled, checked, ..
            }) => (*enabled, *checked),
            other => panic!("{other:?}"),
        };
        s.settings.launch_at_sign_in = true;
        s.settings.sign_in_foreign = false;
        s.settings.error = None;
        assert_eq!(check(&s), (true, true));
        s.settings.launch_at_sign_in = false;
        assert_eq!(check(&s), (true, false));
        // Another program owns the entry, or the settings file is broken.
        s.settings.sign_in_foreign = true;
        assert_eq!(check(&s), (false, false));
        s.settings.sign_in_foreign = false;
        s.settings.error = Some("bad JSON".into());
        assert_eq!(check(&s), (false, false));
    }

    #[test]
    fn problems_are_listed_disabled() {
        let s = fixture("trouble");
        let lines = model(&s);
        for problem in &s.problems {
            let id = format!("problem:{}", problem.file);
            match find(&lines, &id) {
                Some(Line::Item { enabled, text, .. }) => {
                    assert!(!enabled);
                    assert!(text.ends_with(&problem.message));
                }
                other => panic!("{id}: {other:?}"),
            }
        }
    }

    #[test]
    fn every_fixture_makes_a_menu_with_unique_ids() {
        for name in crate::tests::fixture_names() {
            let s = fixture(&name);
            let lines = model(&s);
            let ids: Vec<String> = shape(&lines)
                .into_iter()
                .filter(|i| i != "-" && i != "]")
                .collect();
            let mut unique = ids.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(ids.len(), unique.len(), "{name}: {ids:?}");
            assert_eq!(texts(&lines).last().unwrap(), "Exit (apps keep running)");
        }
    }

    #[test]
    fn a_text_change_keeps_the_shape_and_an_app_change_does_not() {
        let s = fixture("healthy");
        let before = model(&s);
        let mut changed = s.clone();
        changed.headline = "Something else".into();
        changed.apps[0].words = "Busy".into();
        let after = model(&changed);
        assert_eq!(shape(&before), shape(&after));
        let (old, new) = (flatten(&before), flatten(&after));
        let differing: Vec<_> = new
            .iter()
            .filter(|(id, l)| old.get(*id) != Some(l))
            .collect();
        assert!(!differing.is_empty());
        // A submenu's own entry compares by text only, not by its children.
        assert!(flatten(&after).values().all(|l| match l {
            Line::Sub { children, .. } => children.is_empty(),
            _ => true,
        }));

        let mut fewer = s.clone();
        fewer.apps.remove(0);
        assert_ne!(shape(&before), shape(&model(&fewer)));
    }

    #[test]
    fn clicks_name_what_they_do() {
        assert_eq!(clicked("status"), Clicked::Status);
        assert_eq!(clicked("signin"), Clicked::SignIn);
        assert_eq!(clicked("exit"), Clicked::Exit);
        assert_eq!(clicked("restart"), Clicked::Restart);
        assert_eq!(
            clicked("app:voicedock:act:restart"),
            Clicked::Action {
                app: "voicedock".into(),
                action: "restart".into()
            }
        );
        assert_eq!(
            clicked("app:voicedock:open:logs"),
            Clicked::Open {
                app: "voicedock".into(),
                kind: "open".into(),
                key: "logs".into()
            }
        );
        // A key keeps its own colons.
        assert_eq!(
            clicked("app:voicedock:menu:2"),
            Clicked::Open {
                app: "voicedock".into(),
                kind: "menu".into(),
                key: "2".into()
            }
        );
        for nothing in [
            "app:voicedock",
            "app:voicedock:none",
            "noapps",
            "problem:x.json",
            "app:a:exec:x",
        ] {
            assert_eq!(clicked(nothing), Clicked::Nothing, "{nothing}");
        }
    }

    #[test]
    fn the_starting_menu_offers_exit() {
        assert_eq!(texts(&starting()), ["N8 Tray: starting", "-", "Exit"]);
        assert_eq!(clicked("exit"), Clicked::Exit);
    }
}
