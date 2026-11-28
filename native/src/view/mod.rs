//! The window's views and the kit they share.
//!
//! Kit (the shell lane owns these; view lanes ask for changes):
//! `format` (today's words and numbers), `theme` (the palette), `router`
//! (which view shows, Back, Escape, filling), `list` (stable models matched
//! by id), `confirm` (armed actions), `action` (an action button's words),
//! `rows` (app rows, levels and tones), `icons` (app icons), `intent` (what
//! a view asks of N8 Tray: host commands, opening things, history, the
//! clipboard), `gallery` (the widget gallery of debug and test builds).
//!
//! Views (one lane each, one module each): `hub`, `all_apps`, `panel`,
//! `details`, `settings`, `about`, `history`.

/// The product's name as the window shows it: the title bar, the close
/// button's label, the update banner, the settings' words and N8 Tray's own
/// row. The one place to change it (the owner's rename to Traybay at the
/// native switch is this line); Slint reads it from `Product.name`, which
/// `router::wire` sets on every window. Words that name the old PowerShell
/// tray ("Your PowerShell N8 Tray ...") are not the product and keep theirs.
pub const PRODUCT: &str = "N8 Tray";

// The kit is written ahead of the views that use it: until each view lane
// lands, parts of it have no caller outside the tests.
#[allow(dead_code, reason = "the kit for the view lanes")]
pub mod action;
#[allow(dead_code, reason = "the kit for the view lanes")]
pub mod confirm;
pub mod format;
// Only where build.rs compiled the gallery (`ui/debug.slint`).
#[cfg(debug_assertions)]
pub mod gallery;
#[allow(dead_code, reason = "the kit for the view lanes")]
pub mod icons;
#[allow(dead_code, reason = "the kit for the view lanes")]
pub mod intent;
#[allow(dead_code, reason = "the kit for the view lanes")]
pub mod list;
#[allow(dead_code, reason = "the kit for the view lanes")]
pub mod router;
#[allow(dead_code, reason = "the kit for the view lanes")]
pub mod rows;
pub mod theme;

pub mod about;
pub mod all_apps;
pub mod details;
pub mod history;
pub mod hub;
pub mod panel;
pub mod settings;
