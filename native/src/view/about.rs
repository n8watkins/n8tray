//! About N8 Tray (lane: settings; owns this file, `ui/views/about.slint` and
//! `native/tests/goldens/view-about/`). A stub until it lands: the version.
#![forbid(unsafe_code)]

use super::router::{Ctx, Models};

pub fn fill(ui: &dyn Models, ctx: &mut Ctx) {
    ui.about().set_version(ctx.snapshot.version.as_str().into());
}

pub fn wire(_ui: &dyn Models) {}
