//! The window's views and the kit they share.
//!
//! Kit (the shell lane owns these; view lanes ask for changes):
//! `format` (today's words and numbers), `theme` (the palette), `router`
//! (which view shows, Back, Escape, filling), `list` (stable models matched
//! by id), `confirm` (armed actions), `action` (an action button's words),
//! `rows` (app rows, levels and tones), `icons` (app icons), `gallery` (the
//! widget gallery of debug and test builds).
//!
//! Views (one lane each, one module each): `hub`, `all_apps`, `panel`,
//! `details`, `settings`, `about`, `history`.

pub mod action;
pub mod confirm;
pub mod format;
#[cfg(any(debug_assertions, test))]
pub mod gallery;
pub mod icons;
pub mod list;
pub mod router;
pub mod rows;
pub mod theme;

pub mod about;
pub mod all_apps;
pub mod details;
pub mod history;
pub mod hub;
pub mod panel;
pub mod settings;
