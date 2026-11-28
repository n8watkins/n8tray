//! Resource history (lane: history; owns this file, `ui/views/history.slint`,
//! `ui/views/history-model.slint` and `native/tests/goldens/view-history/`).
//! A stub until it lands.
#![forbid(unsafe_code)]

use super::router::{Ctx, Models};

/// The view's golden images (`router::Shot`): add a state here.
#[cfg(debug_assertions)]
pub const SHOTS: &[super::router::Shot] = &[super::router::Shot::of("busy", "busy")];

pub fn fill(_ui: &dyn Models, _ctx: &mut Ctx) {}

pub fn wire(_ui: &dyn Models) {}
