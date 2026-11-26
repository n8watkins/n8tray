//! The hub (lane: hub screen; owns this file, `ui/views/hub.slint` and
//! `native/tests/goldens/view-hub/`). Until it lands, the placeholder of lane
//! N0: the level, the headline and each app's name and words.
#![forbid(unsafe_code)]

use std::rc::Rc;

use n8tray_core::model::{HostSnapshot, LevelName};
use slint::{ModelRc, SharedString, VecModel};

use super::format;
use super::router::{Ctx, Models};
use super::rows;

/// The placeholder's lines: the headline, then each hosted app.
pub fn lines(snapshot: &HostSnapshot) -> Vec<String> {
    format::hub_apps(&snapshot.apps)
        .iter()
        .map(|a| {
            format!(
                "{}: {}",
                a.name,
                a.pending.clone().unwrap_or(a.words.clone())
            )
        })
        .collect()
}

pub fn fill(ui: &dyn Models, ctx: &mut Ctx) {
    let s = ctx.snapshot;
    let model = ui.hub();
    model.set_level(rows::level(s.level));
    model.set_level_text(format::level_words(s.level).into());
    model.set_summary(
        format::title_summary(s.pinned_running, s.pinned_total, s.memory.n8_mb).into(),
    );
    model.set_headline(s.headline.as_str().into());
    let lines: Vec<SharedString> = lines(s).into_iter().map(SharedString::from).collect();
    model.set_lines(ModelRc::from(Rc::new(VecModel::from(lines))));
}

pub fn wire(_ui: &dyn Models) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn the_placeholder_shows_each_hosted_app() {
        let s = fixture("trouble");
        let hosted: Vec<&str> = s
            .apps
            .iter()
            .filter(|a| a.hosted && a.level != LevelName::Hidden)
            .map(|a| a.name.as_str())
            .collect();
        let lines = lines(&s);
        assert_eq!(lines.len(), hosted.len());
        for (line, name) in lines.iter().zip(hosted) {
            assert!(line.starts_with(name), "{line}");
        }
    }
}
