//! Settings (lane: settings and about; owns this file,
//! `ui/views/settings.slint`, `ui/views/settings-model.slint` and
//! `native/tests/goldens/view-settings/`). A stub until it lands.
#![forbid(unsafe_code)]

use super::router::{Ctx, Models};

/// The view's golden images (`router::Shot`): add a state here.
#[cfg(debug_assertions)]
pub const SHOTS: &[super::router::Shot] = &[super::router::Shot::of("busy", "busy")];

pub fn fill(_ui: &dyn Models, _ctx: &mut Ctx) {}

pub fn wire(_ui: &dyn Models) {}
