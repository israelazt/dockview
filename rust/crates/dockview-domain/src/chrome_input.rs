//! What the pointer does to the window's own frame: press a button and let go
//! over it to use it, press the bar to drag the window, press it twice to
//! maximize, press an edge to resize. Pure state, so every rule is tested
//! without a window; the shell turns the commands into calls on the window.

use crate::window_chrome::{Chrome, Edge, WindowButton, Zone};

/// Two presses of the bar this close in time (milliseconds) are a double click.
pub const DOUBLE_CLICK_MS: u64 = 400;
/// ... and this close in space (physical pixels).
pub const DOUBLE_CLICK_SLOP: f32 = 6.0;

/// What the window is asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCommand {
    Minimize,
    /// Maximize, or restore a window that is.
    ToggleMaximize,
    Close,
    /// Start moving the window with the pointer.
    Drag,
    /// Start resizing the window from this edge.
    Resize(Edge),
    /// Show the system menu of the window.
    Menu,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ChromeInput {
    hover: Option<WindowButton>,
    pressed: Option<WindowButton>,
    /// When and where the bar was last pressed (for the double click).
    last_bar_press: Option<(u64, (f32, f32))>,
}

impl ChromeInput {
    /// The button the pointer is over.
    pub fn hover(&self) -> Option<WindowButton> {
        self.hover
    }

    /// The button being held down.
    pub fn pressed(&self) -> Option<WindowButton> {
        self.pressed
    }

    /// The pointer moved (`None`: it left the window).
    pub fn moved(&mut self, chrome: &Chrome, pos: Option<(f32, f32)>) {
        self.hover = match pos.map(|(x, y)| chrome.zone_at(x, y)) {
            Some(Zone::Button(b)) => Some(b),
            _ => None,
        };
    }

    /// The left button went down at `pos`, `now_ms` milliseconds after some
    /// fixed moment.
    pub fn press(
        &mut self,
        chrome: &Chrome,
        pos: (f32, f32),
        now_ms: u64,
    ) -> Option<WindowCommand> {
        match chrome.zone_at(pos.0, pos.1) {
            Zone::Button(b) => {
                self.pressed = Some(b);
                self.hover = Some(b);
                None
            }
            Zone::Resize(edge) => Some(WindowCommand::Resize(edge)),
            Zone::TitleBar => {
                let double = self.last_bar_press.is_some_and(|(at, from)| {
                    now_ms.saturating_sub(at) <= DOUBLE_CLICK_MS
                        && (pos.0 - from.0).abs() <= DOUBLE_CLICK_SLOP
                        && (pos.1 - from.1).abs() <= DOUBLE_CLICK_SLOP
                });
                if double {
                    self.last_bar_press = None;
                    Some(WindowCommand::ToggleMaximize)
                } else {
                    self.last_bar_press = Some((now_ms, pos));
                    Some(WindowCommand::Drag)
                }
            }
            // The navigation row, the rail and the content are the interface's own.
            Zone::Nav | Zone::Rail | Zone::Strip | Zone::Content => None,
        }
    }

    /// The left button went up at `pos`. A button is used only when the
    /// pointer is still on the button it went down on.
    pub fn release(&mut self, chrome: &Chrome, pos: Option<(f32, f32)>) -> Option<WindowCommand> {
        let pressed = self.pressed.take()?;
        let over = pos.map(|(x, y)| chrome.zone_at(x, y));
        (over == Some(Zone::Button(pressed))).then_some(match pressed {
            WindowButton::Minimize => WindowCommand::Minimize,
            WindowButton::Maximize => WindowCommand::ToggleMaximize,
            WindowButton::Close => WindowCommand::Close,
        })
    }

    /// The right button went down at `pos`.
    pub fn press_right(&mut self, chrome: &Chrome, pos: (f32, f32)) -> Option<WindowCommand> {
        matches!(chrome.zone_at(pos.0, pos.1), Zone::TitleBar).then_some(WindowCommand::Menu)
    }

    /// The pointer left the window or the window lost focus: nothing is held.
    pub fn cancel(&mut self) {
        self.hover = None;
        self.pressed = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chrome() -> Chrome {
        Chrome {
            size: (1280, 720),
            scale: 1.0,
            maximized: false,
            fullscreen: false,
            rail_out: 1.0,
        }
    }

    const CLOSE: (f32, f32) = (1250.0, 17.0);
    const MAX: (f32, f32) = (1210.0, 17.0);
    const BAR: (f32, f32) = (600.0, 17.0);

    #[test]
    fn a_button_is_used_when_released_over_the_one_pressed() {
        let c = chrome();
        let mut input = ChromeInput::default();
        assert_eq!(input.press(&c, CLOSE, 0), None);
        assert_eq!(input.pressed(), Some(WindowButton::Close));
        assert_eq!(input.release(&c, Some(CLOSE)), Some(WindowCommand::Close));
        assert_eq!(input.pressed(), None);
        input.press(&c, MAX, 0);
        assert_eq!(
            input.release(&c, Some(MAX)),
            Some(WindowCommand::ToggleMaximize)
        );
        input.press(&c, (1160.0, 17.0), 0);
        assert_eq!(
            input.release(&c, Some((1160.0, 17.0))),
            Some(WindowCommand::Minimize)
        );
    }

    #[test]
    fn letting_go_somewhere_else_cancels_the_click() {
        let c = chrome();
        let mut input = ChromeInput::default();
        input.press(&c, CLOSE, 0);
        assert_eq!(input.release(&c, Some(MAX)), None);
        input.press(&c, CLOSE, 0);
        assert_eq!(input.release(&c, Some(BAR)), None);
        input.press(&c, CLOSE, 0);
        assert_eq!(input.release(&c, None), None, "outside the window");
        assert_eq!(input.release(&c, Some(CLOSE)), None, "nothing was pressed");
    }

    #[test]
    fn hover_follows_the_pointer_over_the_buttons_only() {
        let c = chrome();
        let mut input = ChromeInput::default();
        input.moved(&c, Some(CLOSE));
        assert_eq!(input.hover(), Some(WindowButton::Close));
        input.moved(&c, Some(BAR));
        assert_eq!(input.hover(), None);
        input.moved(&c, Some(MAX));
        input.moved(&c, None);
        assert_eq!(input.hover(), None);
    }

    #[test]
    fn pressing_the_bar_drags_and_two_quick_presses_maximize() {
        let c = chrome();
        let mut input = ChromeInput::default();
        assert_eq!(input.press(&c, BAR, 1000), Some(WindowCommand::Drag));
        assert_eq!(
            input.press(&c, (602.0, 18.0), 1250),
            Some(WindowCommand::ToggleMaximize)
        );
        // The double click is used up: the next press starts a drag again.
        assert_eq!(input.press(&c, BAR, 1300), Some(WindowCommand::Drag));
    }

    #[test]
    fn slow_or_distant_presses_are_not_a_double_click() {
        let c = chrome();
        let mut input = ChromeInput::default();
        input.press(&c, BAR, 1000);
        assert_eq!(
            input.press(&c, BAR, 1500),
            Some(WindowCommand::Drag),
            "too slow"
        );
        input.press(&c, BAR, 5000);
        assert_eq!(
            input.press(&c, (700.0, 17.0), 5100),
            Some(WindowCommand::Drag),
            "too far"
        );
    }

    #[test]
    fn the_edges_start_a_resize_and_the_content_does_nothing() {
        let c = chrome();
        let mut input = ChromeInput::default();
        assert_eq!(
            input.press(&c, (640.0, 2.0), 0),
            Some(WindowCommand::Resize(Edge::Top))
        );
        assert_eq!(
            input.press(&c, (2.0, 717.0), 0),
            Some(WindowCommand::Resize(Edge::BottomLeft))
        );
        assert_eq!(input.press(&c, (640.0, 300.0), 0), None);
        // The navigation row under the bar belongs to the interface.
        assert_eq!(input.press(&c, (640.0, 60.0), 0), None);
    }

    #[test]
    fn a_maximized_window_cannot_be_resized_but_can_be_dragged_and_restored() {
        let c = Chrome {
            maximized: true,
            ..chrome()
        };
        let mut input = ChromeInput::default();
        assert_eq!(input.press(&c, (640.0, 1.0), 0), Some(WindowCommand::Drag));
    }

    #[test]
    fn the_right_button_opens_the_system_menu_on_the_bar_only() {
        let c = chrome();
        let mut input = ChromeInput::default();
        assert_eq!(input.press_right(&c, BAR), Some(WindowCommand::Menu));
        assert_eq!(input.press_right(&c, CLOSE), None);
        assert_eq!(input.press_right(&c, (640.0, 300.0)), None);
    }

    #[test]
    fn cancel_forgets_what_was_held() {
        let c = chrome();
        let mut input = ChromeInput::default();
        input.press(&c, CLOSE, 0);
        input.cancel();
        assert_eq!((input.hover(), input.pressed()), (None, None));
        assert_eq!(input.release(&c, Some(CLOSE)), None);
    }

    #[test]
    fn full_screen_gives_the_content_every_press() {
        let c = Chrome {
            fullscreen: true,
            ..chrome()
        };
        let mut input = ChromeInput::default();
        assert_eq!(input.press(&c, BAR, 0), None);
        assert_eq!(input.press(&c, CLOSE, 0), None);
        assert_eq!(input.press_right(&c, BAR), None);
    }
}
