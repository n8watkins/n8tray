//! All apps (lane: pinned apps; owns this file, `ui/views/all-apps.slint`,
//! `ui/views/all-apps-model.slint` and `native/tests/goldens/view-all-apps/`).
//! A stub until it lands.
#![forbid(unsafe_code)]

use super::router::{Ctx, Models};

/// The view's golden images (`router::Shot`): add a state here.
#[cfg(debug_assertions)]
pub const SHOTS: &[super::router::Shot] = &[super::router::Shot::of("pinned", "pinned")];

pub fn fill(_ui: &dyn Models, _ctx: &mut Ctx) {}

pub fn wire(_ui: &dyn Models) {}
