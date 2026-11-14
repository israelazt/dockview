//! What the selection overlay does with the pointer: the mode buttons and the
//! selector together. A press on a button changes the mode (and its release
//! and moves are not for the selector); anything else is the selector's. It is
//! the same on every platform: the window only turns its events into [`Input`]s.

use crate::desktop::{DesktopMonitor, PxRect};
use crate::select::{toolbar_layout, Input, Mode, Selector, Step, Toolbar, ToolbarHit, View};

pub struct OverlayController {
    selector: Selector,
    toolbar: Toolbar,
    scale_percent: u32,
    pointer: Option<(i32, i32)>,
    /// A press that began on a button.
    on_button: bool,
}

impl OverlayController {
    /// `monitor` is where the buttons go (the one with the cursor).
    pub fn new(mode: Mode, bounds: PxRect, windows: Vec<PxRect>, monitor: &DesktopMonitor) -> Self {
        let toolbar = toolbar_layout(monitor.bounds, monitor.scale_percent);
        Self {
            toolbar,
            selector: Selector::new(mode, bounds, windows),
            scale_percent: monitor.scale_percent.max(100),
            pointer: None,
            on_button: false,
        }
    }

    /// What the mode asks for before any input (the whole screen needs none).
    pub fn start(&self) -> Step {
        self.selector.start()
    }

    pub fn mode(&self) -> Mode {
        self.selector.mode()
    }

    pub fn view(&self) -> View {
        self.selector.view()
    }

    pub fn toolbar(&self) -> &Toolbar {
        &self.toolbar
    }

    /// The display scale the buttons are drawn at, in percent.
    pub fn scale_percent(&self) -> u32 {
        self.scale_percent
    }

    /// Where the pointer is, in virtual-screen pixels.
    pub fn pointer(&self) -> Option<(i32, i32)> {
        self.pointer
    }

    /// The button under the pointer.
    pub fn hovered(&self) -> Option<ToolbarHit> {
        let (x, y) = self.pointer?;
        self.toolbar.hit(x, y)
    }

    /// The mode button under the pointer.
    pub fn hovered_button(&self) -> Option<Mode> {
        match self.hovered() {
            Some(ToolbarHit::Mode(mode)) => Some(mode),
            _ => None,
        }
    }

    /// Whether the pointer is on the close button.
    pub fn close_hovered(&self) -> bool {
        self.hovered() == Some(ToolbarHit::Close)
    }

    /// The button being pressed (a click that is not a selection).
    pub fn pressed(&self) -> Option<ToolbarHit> {
        self.on_button.then(|| self.hovered()).flatten()
    }

    /// The mode button being pressed.
    pub fn button_pressed(&self) -> Option<Mode> {
        match self.pressed() {
            Some(ToolbarHit::Mode(mode)) => Some(mode),
            _ => None,
        }
    }

    /// Whether the close button is being pressed.
    pub fn close_pressed(&self) -> bool {
        self.pressed() == Some(ToolbarHit::Close)
    }

    pub fn apply(&mut self, input: Input) -> Step {
        match input {
            Input::Move(x, y) => {
                self.pointer = Some((x, y));
                if self.on_button {
                    return Step::Continue;
                }
            }
            Input::Down(x, y) => {
                self.pointer = Some((x, y));
                match self.toolbar.hit(x, y) {
                    Some(ToolbarHit::Mode(mode)) => {
                        self.on_button = true;
                        return self.selector.apply(Input::SetMode(mode));
                    }
                    Some(ToolbarHit::Close) => return Step::Cancel,
                    None => {}
                }
            }
            Input::Up(x, y) => {
                self.pointer = Some((x, y));
                if std::mem::take(&mut self.on_button) {
                    return Step::Continue;
                }
            }
            _ => {}
        }
        self.selector.apply(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::select::Selection;

    fn monitor() -> DesktopMonitor {
        DesktopMonitor {
            bounds: PxRect::new(0, 0, 1920, 1080),
            name: "M".into(),
            primary: true,
            scale_percent: 100,
        }
    }

    fn controller() -> OverlayController {
        OverlayController::new(
            Mode::Rectangle,
            PxRect::new(0, 0, 1920, 1080),
            vec![],
            &monitor(),
        )
    }

    fn centre_of(c: &OverlayController, mode: Mode) -> (i32, i32) {
        let r = c
            .toolbar()
            .buttons
            .iter()
            .find(|(m, _)| *m == mode)
            .unwrap()
            .1;
        (r.x + r.w as i32 / 2, r.y + r.h as i32 / 2)
    }

    #[test]
    fn a_press_on_a_button_changes_the_mode_and_starts_no_selection() {
        let mut c = controller();
        let (x, y) = centre_of(&c, Mode::Freeform);
        assert_eq!(c.apply(Input::Down(x, y)), Step::Continue);
        assert_eq!(c.mode(), Mode::Freeform);
        assert_eq!(c.button_pressed(), Some(Mode::Freeform));
        // Dragging off the button after the press selects nothing either.
        c.apply(Input::Move(x + 50, y + 300));
        assert_eq!(c.apply(Input::Up(x + 50, y + 300)), Step::Continue);
        assert_eq!(c.view().path.len(), 0);
        assert_eq!(c.button_pressed(), None);
    }

    #[test]
    fn full_screen_on_the_toolbar_ends_the_selection_at_once() {
        let mut c = controller();
        let (x, y) = centre_of(&c, Mode::Full);
        assert_eq!(
            c.apply(Input::Down(x, y)),
            Step::Done(Selection::Rect(PxRect::new(0, 0, 1920, 1080)))
        );
    }

    #[test]
    fn everything_else_is_the_selectors() {
        let mut c = controller();
        c.apply(Input::Down(200, 300));
        c.apply(Input::Move(500, 600));
        assert!(c.view().rect.is_some());
        assert_eq!(
            c.apply(Input::Up(500, 600)),
            Step::Done(Selection::Rect(PxRect::new(200, 300, 300, 300)))
        );
        assert_eq!(controller().apply(Input::Escape), Step::Cancel);
    }

    #[test]
    fn the_close_button_cancels_and_is_hovered_before() {
        let mut c = controller();
        let r = c.toolbar().close;
        c.apply(Input::Move(r.x + 5, r.y + 5));
        assert!(c.close_hovered());
        assert_eq!(c.hovered_button(), None);
        assert_eq!(c.apply(Input::Down(r.x + 5, r.y + 5)), Step::Cancel);
    }

    #[test]
    fn the_hovered_button_follows_the_pointer() {
        let mut c = controller();
        assert_eq!(c.hovered_button(), None);
        let (x, y) = centre_of(&c, Mode::Window);
        c.apply(Input::Move(x, y));
        assert_eq!(c.hovered_button(), Some(Mode::Window));
        assert_eq!(c.pointer(), Some((x, y)));
        c.apply(Input::Move(5, 900));
        assert_eq!(c.hovered_button(), None);
    }

    #[test]
    fn the_buttons_are_laid_out_on_the_monitor_given() {
        let right = DesktopMonitor {
            bounds: PxRect::new(1920, 0, 2560, 1440),
            scale_percent: 150,
            primary: false,
            name: "R".into(),
        };
        let c = OverlayController::new(Mode::Window, PxRect::new(0, 0, 4480, 1440), vec![], &right);
        let all = c.toolbar().back;
        assert!(all.x > 1920 && all.right() < 4480);
        assert_eq!(c.scale_percent(), 150);
        assert_eq!(c.mode(), Mode::Window);
    }
}
