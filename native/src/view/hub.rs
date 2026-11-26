//! The hub (lane: hub screen; owns this file, `ui/views/hub.slint` and
//! `native/tests/goldens/view-hub/`). Until it lands, the placeholder of lane
//! N0: the level, the headline and each app's name and words.
#![forbid(unsafe_code)]

use std::rc::Rc;

use n8tray_core::model::HostSnapshot;
use slint::{ModelRc, VecModel};

use super::format;
use super::router::{Ctx, Models};
use super::rows;
use crate::ui::HubLine;

/// The view's golden images (`router::Shot`): add a state here.
#[cfg(debug_assertions)]
pub const SHOTS: &[super::router::Shot] = &[
    super::router::Shot::of("busy", "busy"),
    super::router::Shot::of("pinned", "pinned"),
    super::router::Shot {
        compact: true,
        height: 600,
        ..super::router::Shot::of("compact-tall", "compactTall")
    },
];

/// The placeholder's lines: the headline, then each app in the main view.
pub fn lines(snapshot: &HostSnapshot) -> Vec<HubLine> {
    let apps = format::main_view_apps(&snapshot.apps).into_iter().map(|a| {
        let words = a.pending.as_deref().unwrap_or(&a.words);
        HubLine {
            text: format!("{}: {words}", a.name).into(),
            strong: false,
        }
    });
    std::iter::once(HubLine {
        text: snapshot.headline.as_str().into(),
        strong: true,
    })
    .chain(apps)
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
    model.set_lines(ModelRc::from(Rc::new(VecModel::from(lines(s)))));
}

pub fn wire(_ui: &dyn Models) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    /// `HubLine` is declared in `ui/views/hub.slint`, not in the kit: a
    /// view's own struct is a plain Rust type (this module compiles under
    /// `-D warnings`, which refuses Slint's deprecated aliases).
    #[test]
    fn the_placeholder_shows_the_headline_then_each_app_in_the_main_view() {
        let s = fixture("pinned");
        let shown: Vec<&str> = s
            .apps
            .iter()
            .filter(|a| n8tray_core::pins::in_hub(a) && a.in_main_view)
            .map(|a| a.name.as_str())
            .collect();
        assert!(
            shown.len() < s.apps.iter().filter(|a| a.hosted).count(),
            "the pinned fixture has apps outside the main view"
        );
        let lines = lines(&s);
        assert_eq!(lines[0].text, s.headline.as_str());
        assert!(lines[0].strong);
        assert_eq!(lines.len(), shown.len() + 1);
        for (line, name) in lines[1..].iter().zip(shown) {
            assert!(line.text.starts_with(name), "{}", line.text);
            assert!(!line.strong);
        }
    }
}
