//! The automatic move to a compressed (MJPEG) mode when the USB port cannot
//! carry the uncompressed one.
//!
//! Once a second the app tells it whether the diagnosis says "the USB port is
//! too slow" and whether a switch is allowed at all. After a few seconds of
//! slowness it asks for the switch, and a few seconds after that it judges
//! whether the rate really improved; if not, the slowness had another cause and
//! the better-quality mode comes back. It never tries twice in a session unless
//! the user makes a choice themselves ([`AutoSwitch::reset`]).
//!
//! No windows and no settings in here: it only decides, and the app acts.

use dockview_domain::diagnostics::{switch_helped, FallbackWatch};

/// Seconds after an automatic switch before its effect is judged.
pub const CHECK_SECS: u32 = 7;

/// What the app must do after a tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SwitchEvent {
    /// Move to a compressed mode now; `fps_before` is the rate that was too low.
    ToCompressed { fps_before: f32 },
    /// The switch was judged: `helped` says whether to keep it. When it did
    /// not help, go back to the automatic mode.
    Judged { before: f32, now: f32, helped: bool },
}

#[derive(Debug, Default)]
pub struct AutoSwitch {
    watch: FallbackWatch,
    /// The app switched to a compressed mode by itself.
    switched: bool,
    /// A switch was already tried; never try twice.
    tried: bool,
    /// After a switch: seconds until its effect is judged, and the rate before.
    check: Option<(u32, f32)>,
}

impl AutoSwitch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the app is in a compressed mode because of an automatic switch.
    pub fn is_switched(&self) -> bool {
        self.switched
    }

    /// A deliberate choice by the user ends any automatic switch and allows a
    /// new attempt later (for example after changing port).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// One second has passed. `usb_slow` is the diagnosis; `allowed` says
    /// whether a switch may happen right now (automatic mode, a device that
    /// has a compressed mode, no test pattern…).
    pub fn tick(
        &mut self,
        usb_slow: bool,
        allowed: bool,
        received_fps: f32,
    ) -> Option<SwitchEvent> {
        let mut event = None;
        if self.watch.observe(usb_slow, allowed && !self.tried) {
            self.switched = true;
            self.tried = true;
            self.check = Some((CHECK_SECS, received_fps));
            event = Some(SwitchEvent::ToCompressed {
                fps_before: received_fps,
            });
        }
        event.or_else(|| self.judge(received_fps))
    }

    /// Counts down to the verdict, then gives it.
    fn judge(&mut self, fps_now: f32) -> Option<SwitchEvent> {
        let (remaining, before) = self.check?;
        if remaining > 1 {
            self.check = Some((remaining - 1, before));
            return None;
        }
        self.check = None;
        let helped = switch_helped(before, fps_now);
        if !helped {
            self.switched = false;
        }
        Some(SwitchEvent::Judged {
            before,
            now: fps_now,
            helped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ticks until something happens, at most `n` times.
    fn run_until_event(auto: &mut AutoSwitch, n: u32, fps: f32) -> Option<(u32, SwitchEvent)> {
        (1..=n).find_map(|i| auto.tick(true, true, fps).map(|e| (i, e)))
    }

    #[test]
    fn a_slow_usb_port_triggers_one_switch() {
        let mut auto = AutoSwitch::new();
        let (_, event) = run_until_event(&mut auto, 60, 12.0).expect("a switch");
        assert_eq!(event, SwitchEvent::ToCompressed { fps_before: 12.0 });
        assert!(auto.is_switched());
    }

    #[test]
    fn nothing_happens_while_the_port_is_fine_or_a_switch_is_not_allowed() {
        let mut auto = AutoSwitch::new();
        for _ in 0..60 {
            assert_eq!(auto.tick(false, true, 60.0), None);
            assert_eq!(auto.tick(true, false, 12.0), None);
        }
        assert!(!auto.is_switched());
    }

    #[test]
    fn the_switch_is_judged_after_the_check_period_and_kept_if_it_helped() {
        let mut auto = AutoSwitch::new();
        run_until_event(&mut auto, 60, 12.0).expect("a switch");
        // Quiet for the check period, then the verdict.
        let mut verdict = None;
        for second in 1..=CHECK_SECS {
            verdict = auto.tick(false, true, 50.0);
            if second < CHECK_SECS {
                assert_eq!(verdict, None, "second {second}");
            }
        }
        assert_eq!(
            verdict,
            Some(SwitchEvent::Judged {
                before: 12.0,
                now: 50.0,
                helped: true
            })
        );
        assert!(auto.is_switched(), "a switch that helped stays");
    }

    #[test]
    fn a_switch_that_did_not_help_is_undone() {
        let mut auto = AutoSwitch::new();
        run_until_event(&mut auto, 60, 12.0).expect("a switch");
        let verdict = (0..CHECK_SECS).find_map(|_| auto.tick(false, true, 12.5));
        assert!(matches!(
            verdict,
            Some(SwitchEvent::Judged { helped: false, .. })
        ));
        assert!(!auto.is_switched());
    }

    #[test]
    fn it_never_tries_twice_until_the_user_chooses() {
        let mut auto = AutoSwitch::new();
        run_until_event(&mut auto, 60, 12.0).expect("first switch");
        // Judged as not helpful, so the mode goes back to automatic…
        (0..CHECK_SECS).find_map(|_| auto.tick(false, true, 12.0));
        // …but the port is still slow and a second attempt is not made.
        assert_eq!(run_until_event(&mut auto, 120, 12.0), None);

        auto.reset();
        assert!(
            run_until_event(&mut auto, 60, 12.0).is_some(),
            "after a reset"
        );
    }

    #[test]
    fn a_reset_forgets_a_pending_verdict() {
        let mut auto = AutoSwitch::new();
        run_until_event(&mut auto, 60, 12.0).expect("a switch");
        auto.reset();
        assert!(!auto.is_switched());
        for _ in 0..CHECK_SECS + 2 {
            assert_eq!(auto.tick(false, true, 50.0), None);
        }
    }
}
