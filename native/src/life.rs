//! The window's life as plain state, with no toolkit and no clock of its own,
//! so every rule is tested: show and hide, the 400 ms guard after a blur,
//! generations (a timer set for one hide does nothing after a later show),
//! waiting for the native window, and what happens 10 s after hiding (drop
//! then trim, or keep and trim). `window::Hub` carries out the steps with
//! real timers and the real window (D17, D25).
#![forbid(unsafe_code)]

use std::time::{Duration, Instant};

/// A tray click this soon after a blur hid the window is the click that
/// caused the blur: it must not reopen it.
pub const REOPEN_GUARD: Duration = Duration::from_millis(400);
/// How many event loop turns (10 ms apart) to wait for the native window
/// after Slint's first show before giving up on it.
pub const WAIT_TURNS: u32 = 200;

/// What happens to a window hidden for `window::KEEP_HIDDEN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhenHidden {
    /// Dropped, then the working set trimmed (the default, D17).
    Drop,
    /// Kept, and the working set trimmed (`N8TRAY_WINDOW=keep`, for measuring).
    Keep,
}

impl WhenHidden {
    /// `N8TRAY_WINDOW`'s value: only `keep` keeps the window.
    pub fn parse(value: Option<&str>) -> WhenHidden {
        match value {
            Some("keep") => WhenHidden::Keep,
            _ => WhenHidden::Drop,
        }
    }
}

/// One thing for the window to do, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Stop the wait, retire and trim timers.
    StopTimers,
    /// Create the Slint window (shown once off screen, never activated).
    Create,
    /// Put the latest snapshot in it.
    Fill,
    /// Look for the native window again on the next turn.
    WaitForNative,
    /// Place it and show it, activated or not.
    Reveal { activate: bool },
    /// Hide it.
    Hide,
    /// Tell the host the window opened or closed (it samples faster while open).
    Open(bool),
    /// In `KEEP_HIDDEN`, call [`Life::retire`] with this generation.
    StartRetire { generation: u64 },
    /// Drop the Slint window and forget the native one.
    Drop,
    /// Trim the working set in `TRIM_AFTER_DROP`.
    StartTrim,
    /// Trim the working set now.
    TrimNow,
}

#[derive(Debug)]
pub struct Life {
    when_hidden: WhenHidden,
    created: bool,
    native: bool,
    visible: bool,
    /// Counts shows and hides.
    generation: u64,
    hidden_by_blur: Option<Instant>,
    /// Show (with activation or not) once the native window is there.
    pending: Option<bool>,
    waited: u32,
}

impl Life {
    pub fn new(when_hidden: WhenHidden) -> Life {
        Life {
            when_hidden,
            created: false,
            native: false,
            visible: false,
            generation: 0,
            hidden_by_blur: None,
            pending: None,
            waited: 0,
        }
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn created(&self) -> bool {
        self.created
    }

    /// A left click on the tray icon: open, or close when open.
    pub fn toggle(&mut self, now: Instant, activate: bool) -> Vec<Step> {
        let just_hidden = self
            .hidden_by_blur
            .is_some_and(|t| now.saturating_duration_since(t) < REOPEN_GUARD);
        if self.visible {
            self.hide(now, false)
        } else if just_hidden {
            Vec::new()
        } else {
            self.show(activate)
        }
    }

    /// Open the window. The caller then calls [`Life::reveal`].
    pub fn show(&mut self, activate: bool) -> Vec<Step> {
        self.generation += 1;
        let mut steps = vec![Step::StopTimers];
        if !self.created {
            self.created = true;
            self.native = false;
            steps.push(Step::Create);
        }
        steps.push(Step::Fill);
        self.pending = Some(activate);
        self.waited = 0;
        steps
    }

    /// Creating the window failed: nothing to show.
    pub fn create_failed(&mut self) {
        self.created = false;
        self.native = false;
        self.pending = None;
    }

    /// The native window is there.
    pub fn native_found(&mut self) {
        if self.created {
            self.native = true;
        }
    }

    pub fn has_native(&self) -> bool {
        self.native
    }

    /// Show the window if one is wanted and the native window is there;
    /// wait for it otherwise, and give up (drop it) after [`WAIT_TURNS`].
    pub fn reveal(&mut self) -> Vec<Step> {
        if !self.created || self.pending.is_none() {
            return Vec::new();
        }
        if !self.native {
            if self.waited < WAIT_TURNS {
                self.waited += 1;
                return vec![Step::WaitForNative];
            }
            self.pending = None;
            self.created = false;
            return vec![Step::StopTimers, Step::Drop];
        }
        let activate = self.pending.take().unwrap_or(true);
        let mut steps = vec![Step::Reveal { activate }];
        if !self.visible {
            self.visible = true;
            steps.push(Step::Open(true));
        }
        steps
    }

    /// Hide the window; `from_blur` when focus loss hid it.
    pub fn hide(&mut self, now: Instant, from_blur: bool) -> Vec<Step> {
        // A show still waiting for its native window is called off too.
        self.pending = None;
        if !self.visible {
            return Vec::new();
        }
        if from_blur {
            self.hidden_by_blur = Some(now);
        }
        self.visible = false;
        self.generation += 1;
        vec![
            Step::Hide,
            Step::Open(false),
            Step::StartRetire {
                generation: self.generation,
            },
        ]
    }

    /// The window lost the focus: a shown window hides (D25).
    pub fn blurred(&mut self, now: Instant) -> Vec<Step> {
        if self.visible {
            self.hide(now, true)
        } else {
            Vec::new()
        }
    }

    /// The window has been hidden for `KEEP_HIDDEN` since this generation.
    pub fn retire(&mut self, generation: u64) -> Vec<Step> {
        if generation != self.generation || self.visible || !self.created {
            return Vec::new();
        }
        match self.when_hidden {
            WhenHidden::Drop => {
                self.created = false;
                self.native = false;
                vec![Step::Drop, Step::StartTrim]
            }
            WhenHidden::Keep => vec![Step::TrimNow],
        }
    }

    /// N8 Tray is exiting.
    pub fn close(&mut self) -> Vec<Step> {
        self.created = false;
        self.native = false;
        self.visible = false;
        self.pending = None;
        vec![Step::StopTimers, Step::Drop]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opens a window whose native window turns up at once.
    fn opened(life: &mut Life) -> Vec<Step> {
        let mut steps = life.show(true);
        life.native_found();
        steps.extend(life.reveal());
        steps
    }

    #[test]
    fn the_first_show_creates_fills_and_reveals() {
        let mut life = Life::new(WhenHidden::Drop);
        assert_eq!(
            opened(&mut life),
            [
                Step::StopTimers,
                Step::Create,
                Step::Fill,
                Step::Reveal { activate: true },
                Step::Open(true)
            ]
        );
        assert!(life.visible());
        // Shown again while open: placed again, nothing new created or announced.
        assert_eq!(
            opened(&mut life),
            [
                Step::StopTimers,
                Step::Fill,
                Step::Reveal { activate: true }
            ]
        );
    }

    #[test]
    fn a_click_toggles_and_the_click_that_blurred_it_does_not_reopen() {
        let t0 = Instant::now();
        let mut life = Life::new(WhenHidden::Drop);
        life.toggle(t0, false);
        life.native_found();
        assert_eq!(
            life.reveal(),
            [Step::Reveal { activate: false }, Step::Open(true)]
        );
        // Clicking the icon blurs the window first, then the click arrives.
        let hidden = life.blurred(t0);
        assert_eq!(hidden[..2], [Step::Hide, Step::Open(false)]);
        assert!(
            life.toggle(t0 + Duration::from_millis(399), true)
                .is_empty()
        );
        // Later, a click opens it again.
        let steps = life.toggle(t0 + REOPEN_GUARD, true);
        assert!(steps.contains(&Step::Fill));
        // A click while open closes it, not by blur: no guard after it.
        life.reveal();
        let t1 = t0 + Duration::from_secs(5);
        assert_eq!(life.toggle(t1, true)[..2], [Step::Hide, Step::Open(false)]);
        assert!(!life.toggle(t1, true).is_empty());
    }

    #[test]
    fn a_blur_hides_only_a_shown_window() {
        let mut life = Life::new(WhenHidden::Drop);
        assert!(life.blurred(Instant::now()).is_empty());
        // Created but still waiting for its native window: a lost focus it
        // never had must not hide it.
        life.show(true);
        assert!(life.blurred(Instant::now()).is_empty());
        life.native_found();
        assert!(!life.reveal().is_empty());
    }

    #[test]
    fn a_hidden_window_is_dropped_then_the_memory_trimmed() {
        let mut life = Life::new(WhenHidden::Drop);
        opened(&mut life);
        let steps = life.hide(Instant::now(), false);
        let Some(Step::StartRetire { generation }) = steps.last().copied() else {
            panic!("{steps:?}");
        };
        assert_eq!(life.retire(generation), [Step::Drop, Step::StartTrim]);
        assert!(!life.created());
        // Twice is nothing, and the next show creates it again.
        assert!(life.retire(generation).is_empty());
        assert!(opened(&mut life).contains(&Step::Create));
    }

    #[test]
    fn a_kept_window_is_only_trimmed() {
        let mut life = Life::new(WhenHidden::Keep);
        opened(&mut life);
        let steps = life.hide(Instant::now(), false);
        let Some(Step::StartRetire { generation }) = steps.last().copied() else {
            panic!("{steps:?}");
        };
        assert_eq!(life.retire(generation), [Step::TrimNow]);
        assert!(life.created());
        // Shown again: no new window.
        assert!(!opened(&mut life).contains(&Step::Create));
    }

    #[test]
    fn a_show_after_a_hide_cancels_that_hides_retire() {
        let mut life = Life::new(WhenHidden::Drop);
        opened(&mut life);
        let steps = life.hide(Instant::now(), false);
        let Some(Step::StartRetire { generation }) = steps.last().copied() else {
            panic!("{steps:?}");
        };
        // Opened again within 10 s: the old timer fires into nothing.
        assert!(opened(&mut life).starts_with(&[Step::StopTimers]));
        assert!(life.retire(generation).is_empty());
        // Hidden again: only the newest generation retires it.
        let steps = life.hide(Instant::now(), false);
        let Some(Step::StartRetire { generation: newer }) = steps.last().copied() else {
            panic!("{steps:?}");
        };
        assert!(newer > generation);
        assert!(life.retire(generation).is_empty());
        assert_eq!(life.retire(newer), [Step::Drop, Step::StartTrim]);
    }

    #[test]
    fn a_show_while_waiting_for_the_native_window_takes_the_latest_wish() {
        let mut life = Life::new(WhenHidden::Drop);
        assert!(life.show(true).contains(&Step::Create));
        assert_eq!(life.reveal(), [Step::WaitForNative]);
        // Shown again before the native window exists (activation now off):
        // no second window, and the wait starts over.
        assert!(!life.show(false).contains(&Step::Create));
        for _ in 0..WAIT_TURNS - 1 {
            assert_eq!(life.reveal(), [Step::WaitForNative]);
        }
        life.native_found();
        assert_eq!(
            life.reveal(),
            [Step::Reveal { activate: false }, Step::Open(true)]
        );
    }

    #[test]
    fn a_native_window_that_never_comes_is_dropped() {
        let mut life = Life::new(WhenHidden::Drop);
        life.show(true);
        for _ in 0..WAIT_TURNS {
            assert_eq!(life.reveal(), [Step::WaitForNative]);
        }
        assert_eq!(life.reveal(), [Step::StopTimers, Step::Drop]);
        assert!(!life.created() && !life.visible());
        assert!(life.reveal().is_empty());
        // The next show tries a new window.
        assert!(life.show(true).contains(&Step::Create));
    }

    #[test]
    fn a_hide_while_waiting_calls_the_show_off() {
        let mut life = Life::new(WhenHidden::Drop);
        life.show(true);
        assert!(life.hide(Instant::now(), false).is_empty());
        life.native_found();
        assert!(life.reveal().is_empty(), "nothing is shown after the hide");
    }

    #[test]
    fn a_failed_creation_shows_nothing() {
        let mut life = Life::new(WhenHidden::Drop);
        life.show(true);
        life.create_failed();
        life.native_found();
        assert!(life.reveal().is_empty());
        assert!(life.show(true).contains(&Step::Create));
    }

    #[test]
    fn closing_drops_everything() {
        let mut life = Life::new(WhenHidden::Keep);
        opened(&mut life);
        assert_eq!(life.close(), [Step::StopTimers, Step::Drop]);
        assert!(!life.visible() && !life.created());
    }

    #[test]
    fn only_keep_keeps_the_window() {
        assert_eq!(WhenHidden::parse(None), WhenHidden::Drop);
        assert_eq!(WhenHidden::parse(Some("drop")), WhenHidden::Drop);
        assert_eq!(WhenHidden::parse(Some("")), WhenHidden::Drop);
        assert_eq!(WhenHidden::parse(Some("Keep")), WhenHidden::Drop);
        assert_eq!(WhenHidden::parse(Some("keep")), WhenHidden::Keep);
    }
}
