//! An action button's words and state, as today's `ActionButton` gives
//! them: the accessible name ("Stop VoiceDock", "Confirm: Stop VoiceDock"),
//! the tooltip ("Restarting" while something happens, "Stop: not possible
//! now", "Click again to confirm"), and whether it can be clicked.
#![forbid(unsafe_code)]

use n8tray_core::model::{ActionView, AppView};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionUi {
    /// What a screen reader says.
    pub label: String,
    pub tooltip: String,
    pub enabled: bool,
    pub armed: bool,
    /// The button's own words (a text button; "Confirm" while armed).
    pub words: String,
}

/// `label` overrides the action's own label (per-component actions name
/// their component); `component` is set for a component's button, whose
/// name then leaves out the app's.
pub fn action_ui(
    app: &AppView,
    action: &ActionView,
    component: Option<&str>,
    label: Option<&str>,
    armed: bool,
) -> ActionUi {
    let busy = app.pending.is_some();
    let words = label.unwrap_or(&action.label).to_string();
    let name = if component.is_some() {
        if armed {
            format!("Confirm: {words}")
        } else {
            words.clone()
        }
    } else if armed {
        format!("Confirm: {words} {}", app.name)
    } else {
        format!("{words} {}", app.name)
    };
    let tooltip = if let Some(pending) = &app.pending {
        pending.clone()
    } else if !action.enabled {
        format!("{words}: not possible now")
    } else if armed {
        "Click again to confirm".into()
    } else {
        words.clone()
    };
    ActionUi {
        label: name.trim().to_string(),
        tooltip,
        enabled: action.enabled && !busy,
        armed,
        words: if armed { "Confirm".into() } else { words },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    fn app(id: &str, name: &str) -> AppView {
        fixture(name).apps.into_iter().find(|a| a.id == id).unwrap()
    }

    #[test]
    fn names_and_tooltips_are_todays() {
        let vd = app("voicedock", "busy");
        let stop = vd.actions.iter().find(|a| a.id == "stop").unwrap();
        let ui = action_ui(&vd, stop, None, None, false);
        assert_eq!(ui.label, format!("{} VoiceDock", stop.label));
        assert_eq!(ui.tooltip, stop.label);
        assert!(ui.enabled);
        let armed = action_ui(&vd, stop, None, None, true);
        assert_eq!(armed.label, format!("Confirm: {} VoiceDock", stop.label));
        assert_eq!(armed.tooltip, "Click again to confirm");
        assert_eq!(armed.words, "Confirm");
        let comp = action_ui(&vd, stop, Some("kokoro"), Some("Stop Kokoro"), false);
        assert_eq!(comp.label, "Stop Kokoro");
    }

    #[test]
    fn a_busy_app_and_an_impossible_action_say_why() {
        let mut vd = app("voicedock", "busy");
        let mut stop = vd.actions.iter().find(|a| a.id == "stop").unwrap().clone();
        stop.enabled = false;
        let ui = action_ui(&vd, &stop, None, None, false);
        assert_eq!(ui.tooltip, format!("{}: not possible now", stop.label));
        assert!(!ui.enabled);
        vd.pending = Some("Restarting".into());
        stop.enabled = true;
        let busy = action_ui(&vd, &stop, None, None, false);
        assert_eq!(busy.tooltip, "Restarting");
        assert!(!busy.enabled);
    }
}
