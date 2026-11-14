//! The shape of the mouse cursor over the layer editor.

use dockview_domain::layer_edit::Handle;
use winit::window::CursorIcon;

/// The cursor for the handle the pointer is on; while dragging a body, a closed
/// hand.
pub fn cursor_for(handle: Handle, dragging: bool) -> CursorIcon {
    match handle {
        Handle::Move if dragging => CursorIcon::Grabbing,
        Handle::Move => CursorIcon::Move,
        Handle::TopLeft | Handle::BottomRight => CursorIcon::NwseResize,
        Handle::TopRight | Handle::BottomLeft => CursorIcon::NeswResize,
        Handle::Top | Handle::Bottom => CursorIcon::NsResize,
        Handle::Left | Handle::Right => CursorIcon::EwResize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_handle_gets_the_arrow_that_points_along_its_edge() {
        assert_eq!(cursor_for(Handle::TopLeft, false), CursorIcon::NwseResize);
        assert_eq!(
            cursor_for(Handle::BottomRight, true),
            CursorIcon::NwseResize
        );
        assert_eq!(cursor_for(Handle::TopRight, false), CursorIcon::NeswResize);
        assert_eq!(
            cursor_for(Handle::BottomLeft, false),
            CursorIcon::NeswResize
        );
        assert_eq!(cursor_for(Handle::Top, false), CursorIcon::NsResize);
        assert_eq!(cursor_for(Handle::Left, false), CursorIcon::EwResize);
    }

    #[test]
    fn a_body_is_a_hand_that_closes_while_dragging() {
        assert_eq!(cursor_for(Handle::Move, false), CursorIcon::Move);
        assert_eq!(cursor_for(Handle::Move, true), CursorIcon::Grabbing);
    }
}
