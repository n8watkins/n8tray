//! All apps (lane: pinned apps; owns this file, `ui/views/all-apps.slint`
//! and `native/tests/goldens/view-all-apps/`). A stub until it lands.
#![forbid(unsafe_code)]

use super::router::{Ctx, Models};

pub fn fill(_ui: &dyn Models, _ctx: &mut Ctx) {}

pub fn wire(_ui: &dyn Models) {}
