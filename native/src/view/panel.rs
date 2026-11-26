//! One app's panel (lane: app panel; owns this file, `ui/views/panel.slint`
//! and `native/tests/goldens/view-panel/`). A stub until it lands: the
//! title bar with the app's icon, name and words.
#![forbid(unsafe_code)]

use super::router::{Ctx, Models};
use super::rows;

pub fn fill(ui: &dyn Models, ctx: &mut Ctx) {
    let Some(app) = ctx
        .app
        .and_then(|id| ctx.snapshot.apps.iter().find(|a| a.id == id))
    else {
        return;
    };
    let model = ui.panel();
    model.set_name(app.name.as_str().into());
    model.set_icon(ctx.icons.get(app.icon.as_deref()));
    model.set_level(rows::level(app.level));
    model.set_words(app.pending.as_ref().unwrap_or(&app.words).as_str().into());
}

pub fn wire(_ui: &dyn Models) {}
