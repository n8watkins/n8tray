//! About N8 Tray (lane: settings and about; owns this file,
//! `ui/views/about.slint`, `ui/views/about-model.slint` and
//! `native/tests/goldens/view-about/`). A stub until it lands: the version.
#![forbid(unsafe_code)]

use super::router::{Ctx, Models};

/// The view's golden images (`router::Shot`): add a state here.
#[cfg(debug_assertions)]
pub const SHOTS: &[super::router::Shot] = &[super::router::Shot::of("busy", "busy")];

pub fn fill(ui: &dyn Models, ctx: &mut Ctx) {
    ui.about().set_version(ctx.snapshot.version.as_str().into());
}

pub fn wire(_ui: &dyn Models) {}
