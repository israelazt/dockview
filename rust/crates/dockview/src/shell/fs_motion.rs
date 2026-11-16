//! The motion of going to full screen and back. Windows changes the size of the
//! window at once and that cannot be animated, so what is animated is what is
//! inside it:
//!
//! - **Going in:** the frame (title bar, tabs, rail, panel) fades out while the
//!   window is still as it was; then the window changes; then the picture grows
//!   from where it was on the screen to the whole screen.
//! - **Going out:** the window changes at once; the picture shrinks from the
//!   whole screen to its place while the frame fades in.
//!
//! This is only the clock and the arithmetic; the shell does the rest.

use dockview_domain::layout::Rect;
use dockview_ui::motion::ease_out_cubic;

/// How long the frame takes to fade out before the window changes, in seconds.
const RETRACT_SECS: f32 = 0.18;
/// How long the picture takes to grow to the whole screen, in seconds.
const GROW_SECS: f32 = 0.24;
/// How long the picture takes to go back to its place, in seconds.
const REVEAL_SECS: f32 = 0.24;

/// What the window has to do right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Enter,
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    /// The frame fades out; the window is not full screen yet.
    Retracting(f32),
    /// The window is full screen; the picture grows from `from`.
    Growing(f32),
    /// The window is back; the picture shrinks from `from` to its place.
    Revealing(f32),
}

/// Where in the motion the window is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FullscreenMotion {
    phase: Phase,
    /// The window is (or, once its change is made, will be) full screen: what
    /// the layout of the frame follows.
    fullscreen: bool,
    /// Where the picture was on the desktop when the window changed.
    from: Rect,
}

impl Default for FullscreenMotion {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            fullscreen: false,
            from: Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
        }
    }
}

/// The rectangle between `from` and `to` (`amount` 0 is `from`, 1 is `to`).
pub fn blend_rect(from: Rect, to: Rect, amount: f32) -> Rect {
    let k = amount.clamp(0.0, 1.0);
    let mix = |a: f32, b: f32| a + (b - a) * k;
    Rect {
        x: mix(from.x, to.x),
        y: mix(from.y, to.y),
        w: mix(from.w, to.w),
        h: mix(from.h, to.h),
    }
}

impl FullscreenMotion {
    /// Whether the frame is laid out as in full screen (nothing around the
    /// picture).
    pub fn layout_fullscreen(&self) -> bool {
        self.fullscreen
    }

    /// Whether anything is moving (so the window keeps drawing frames).
    pub fn is_active(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// Whether the picture is between two places (its place is not saved).
    pub fn is_blending(&self) -> bool {
        matches!(self.phase, Phase::Growing(_) | Phase::Revealing(_))
    }

    /// How visible the frame is: 1 at rest, falling to 0 as it goes out and
    /// rising again as it comes back.
    pub fn frame_opacity(&self) -> f32 {
        match self.phase {
            Phase::Idle | Phase::Growing(_) => 1.0,
            Phase::Retracting(p) => 1.0 - ease_out_cubic(p),
            Phase::Revealing(p) => ease_out_cubic(p),
        }
    }

    /// While the picture moves between places: where it started and how far
    /// along it is (0 to 1, eased).
    pub fn picture_blend(&self) -> Option<(Rect, f32)> {
        match self.phase {
            Phase::Growing(p) | Phase::Revealing(p) => Some((self.from, ease_out_cubic(p))),
            _ => None,
        }
    }

    /// Where the picture goes in this frame. `area` is its place at rest in
    /// the window (`None`: the whole window), `window` the size of the window
    /// and `inner` where the window's drawing area is on the desktop. Returns
    /// the area to draw it in (between its old place and its place while it
    /// moves) and its place at rest on the desktop.
    pub fn place_picture(
        &self,
        area: Option<Rect>,
        window: (f32, f32),
        inner: (f32, f32),
    ) -> (Option<Rect>, Rect) {
        let at_rest = area.unwrap_or(Rect {
            x: 0.0,
            y: 0.0,
            w: window.0,
            h: window.1,
        });
        let desktop = Rect {
            x: at_rest.x + inner.0,
            y: at_rest.y + inner.1,
            ..at_rest
        };
        let shown = match self.picture_blend() {
            Some((from, amount)) => {
                let b = blend_rect(from, desktop, amount);
                Some(Rect {
                    x: b.x - inner.0,
                    y: b.y - inner.1,
                    ..b
                })
            }
            None => area,
        };
        (shown, desktop)
    }

    /// What the window is heading for.
    fn heading_for_fullscreen(&self) -> bool {
        match self.phase {
            Phase::Retracting(_) | Phase::Growing(_) => true,
            Phase::Revealing(_) => false,
            Phase::Idle => self.fullscreen,
        }
    }

    /// Full screen was asked for or left. `animate` is false when there is
    /// nothing to animate (animations off, or the window is just starting);
    /// `picture` is where the picture is on the desktop now. Returns the change
    /// the window has to make right away, if any.
    pub fn request(&mut self, on: bool, animate: bool, picture: Rect) -> Option<Change> {
        if !animate {
            let was = self.fullscreen;
            self.phase = Phase::Idle;
            self.fullscreen = on;
            return (was != on).then_some(if on { Change::Enter } else { Change::Leave });
        }
        if on == self.heading_for_fullscreen() {
            return None;
        }
        if on {
            // The window is not full screen yet: the frame goes out first.
            self.phase = Phase::Retracting(0.0);
            self.fullscreen = false;
            return None;
        }
        match self.phase {
            // Changed its mind before the window changed.
            Phase::Retracting(_) => {
                self.phase = Phase::Idle;
                None
            }
            // The window is full screen (or about to leave): it leaves now.
            _ => {
                self.fullscreen = false;
                self.from = picture;
                self.phase = Phase::Revealing(0.0);
                Some(Change::Leave)
            }
        }
    }

    /// Lets `dt` seconds go by. `picture` is where the picture is on the
    /// desktop now (it is where the growing starts). Returns the change the
    /// window has to make, if the frame has gone out.
    pub fn advance(&mut self, dt: f32, picture: Rect) -> Option<Change> {
        match self.phase {
            Phase::Idle => None,
            Phase::Retracting(p) => {
                let p = p + dt / RETRACT_SECS;
                if p >= 1.0 {
                    self.fullscreen = true;
                    self.from = picture;
                    self.phase = Phase::Growing(0.0);
                    Some(Change::Enter)
                } else {
                    self.phase = Phase::Retracting(p);
                    None
                }
            }
            Phase::Growing(p) => {
                let p = p + dt / GROW_SECS;
                self.phase = if p >= 1.0 {
                    Phase::Idle
                } else {
                    Phase::Growing(p)
                };
                None
            }
            Phase::Revealing(p) => {
                let p = p + dt / REVEAL_SECS;
                self.phase = if p >= 1.0 {
                    Phase::Idle
                } else {
                    Phase::Revealing(p)
                };
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    const PICTURE: Rect = Rect {
        x: 300.0,
        y: 200.0,
        w: 800.0,
        h: 450.0,
    };

    fn run(m: &mut FullscreenMotion, seconds: f32) -> Vec<Change> {
        let mut changes = Vec::new();
        let mut left = seconds;
        while left > 0.0 {
            let dt = left.min(0.016);
            changes.extend(m.advance(dt, PICTURE));
            left -= dt;
        }
        changes
    }

    #[test]
    fn going_in_fades_the_frame_first_and_changes_the_window_only_after() {
        let mut m = FullscreenMotion::default();
        assert_eq!(m.request(true, true, PICTURE), None, "not at once");
        assert!(!m.layout_fullscreen(), "the frame is still laid out");
        assert!(m.is_active() && !m.is_blending());
        assert_eq!(run(&mut m, 0.05), []);
        assert!(m.frame_opacity() < 1.0 && m.frame_opacity() > 0.0);
        assert_eq!(run(&mut m, 0.3), [Change::Enter]);
        assert!(m.layout_fullscreen());
        assert!(m.is_blending(), "then the picture grows");
    }

    #[test]
    fn the_picture_grows_from_where_it_was_and_then_it_is_at_rest() {
        let mut m = FullscreenMotion::default();
        m.request(true, true, PICTURE);
        run(&mut m, 0.3);
        let (from, k) = m.picture_blend().expect("growing");
        assert_eq!(from, PICTURE);
        assert!((0.0..=1.0).contains(&k));
        run(&mut m, 0.5);
        assert!(!m.is_active() && m.picture_blend().is_none());
        assert_eq!(m.frame_opacity(), 1.0);
    }

    #[test]
    fn going_out_changes_the_window_at_once_and_the_frame_comes_back() {
        let mut m = FullscreenMotion::default();
        m.request(true, true, PICTURE);
        run(&mut m, 1.0);
        assert!(m.layout_fullscreen() && !m.is_active());
        let whole = rect(0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(m.request(false, true, whole), Some(Change::Leave));
        assert!(!m.layout_fullscreen());
        assert_eq!(m.frame_opacity(), 0.0, "it starts hidden");
        assert_eq!(m.picture_blend().map(|(from, _)| from), Some(whole));
        run(&mut m, 0.1);
        assert!(m.frame_opacity() > 0.0);
        run(&mut m, 1.0);
        assert!(!m.is_active());
        assert_eq!(m.frame_opacity(), 1.0);
    }

    #[test]
    fn without_animation_the_change_is_made_at_once_and_nothing_moves() {
        let mut m = FullscreenMotion::default();
        assert_eq!(m.request(true, false, PICTURE), Some(Change::Enter));
        assert!(m.layout_fullscreen() && !m.is_active());
        assert_eq!(m.request(true, false, PICTURE), None, "already there");
        assert_eq!(m.request(false, false, PICTURE), Some(Change::Leave));
        assert!(!m.layout_fullscreen());
    }

    #[test]
    fn asking_for_what_is_already_coming_does_nothing() {
        let mut m = FullscreenMotion::default();
        assert_eq!(m.request(false, true, PICTURE), None, "already windowed");
        m.request(true, true, PICTURE);
        assert_eq!(m.request(true, true, PICTURE), None);
        assert!(m.is_active());
    }

    #[test]
    fn changing_the_mind_while_the_frame_goes_out_goes_back_without_touching_the_window() {
        let mut m = FullscreenMotion::default();
        m.request(true, true, PICTURE);
        run(&mut m, 0.05);
        assert_eq!(m.request(false, true, PICTURE), None);
        assert!(!m.is_active() && !m.layout_fullscreen());
        assert_eq!(m.frame_opacity(), 1.0);
    }

    #[test]
    fn leaving_while_the_picture_still_grows_leaves_the_window_now() {
        let mut m = FullscreenMotion::default();
        m.request(true, true, PICTURE);
        run(&mut m, 0.25);
        assert!(m.is_blending() && m.layout_fullscreen());
        assert_eq!(m.request(false, true, PICTURE), Some(Change::Leave));
        assert!(!m.layout_fullscreen());
    }

    #[test]
    fn at_rest_the_picture_is_drawn_where_it_is_and_remembered_on_the_desktop() {
        let m = FullscreenMotion::default();
        let area = rect(20.0, 40.0, 800.0, 450.0);
        let (shown, desktop) = m.place_picture(Some(area), (1280.0, 720.0), (100.0, 50.0));
        assert_eq!(shown, Some(area));
        assert_eq!(desktop, rect(120.0, 90.0, 800.0, 450.0));
        let (whole, desktop) = m.place_picture(None, (1920.0, 1080.0), (0.0, 0.0));
        assert_eq!(whole, None, "no area: the renderer fits the window");
        assert_eq!(desktop, rect(0.0, 0.0, 1920.0, 1080.0));
    }

    #[test]
    fn while_it_grows_the_picture_starts_where_it_was_on_the_desktop() {
        let mut m = FullscreenMotion::default();
        m.request(true, true, PICTURE);
        run(&mut m, 0.2); // the window changes
                          // The window is now the whole screen at (0, 0).
        let (shown, _) = m.place_picture(None, (1920.0, 1080.0), (0.0, 0.0));
        let a = shown.expect("between two places");
        assert!(a.w >= PICTURE.w && a.w <= 1920.0);
        assert!(a.x <= PICTURE.x, "growing outwards from its place");
    }

    #[test]
    fn blending_rectangles_goes_from_one_to_the_other() {
        let a = rect(0.0, 0.0, 100.0, 100.0);
        let b = rect(100.0, 50.0, 200.0, 300.0);
        assert_eq!(blend_rect(a, b, 0.0), a);
        assert_eq!(blend_rect(a, b, 1.0), b);
        assert_eq!(blend_rect(a, b, 0.5), rect(50.0, 25.0, 150.0, 200.0));
        assert_eq!(blend_rect(a, b, 7.0), b, "never beyond");
    }
}
