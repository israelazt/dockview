//! The shape of the mouse cursor over the layer editor.

use dockview_domain::layer_edit::Handle;
use dockview_domain::window_chrome::{ChromeCursor, Edge};
use winit::window::{CursorIcon, ResizeDirection};

/// The cursor for a zone of the window's frame.
pub fn chrome_cursor(cursor: ChromeCursor) -> CursorIcon {
    match cursor {
        ChromeCursor::Default => CursorIcon::Default,
        ChromeCursor::ResizeVertical => CursorIcon::NsResize,
        ChromeCursor::ResizeHorizontal => CursorIcon::EwResize,
        ChromeCursor::ResizeNwSe => CursorIcon::NwseResize,
        ChromeCursor::ResizeNeSw => CursorIcon::NeswResize,
    }
}

/// The direction of a resize drag that starts at `edge`.
pub fn resize_direction(edge: Edge) -> ResizeDirection {
    match edge {
        Edge::Top => ResizeDirection::North,
        Edge::Bottom => ResizeDirection::South,
        Edge::Left => ResizeDirection::West,
        Edge::Right => ResizeDirection::East,
        Edge::TopLeft => ResizeDirection::NorthWest,
        Edge::TopRight => ResizeDirection::NorthEast,
        Edge::BottomLeft => ResizeDirection::SouthWest,
        Edge::BottomRight => ResizeDirection::SouthEast,
    }
}

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
    fn the_frame_cursors_and_resize_directions_match_their_edges() {
        assert_eq!(
            chrome_cursor(ChromeCursor::ResizeVertical),
            CursorIcon::NsResize
        );
        assert_eq!(
            chrome_cursor(ChromeCursor::ResizeNwSe),
            CursorIcon::NwseResize
        );
        assert_eq!(chrome_cursor(ChromeCursor::Default), CursorIcon::Default);
        assert_eq!(resize_direction(Edge::Top), ResizeDirection::North);
        assert_eq!(
            resize_direction(Edge::BottomRight),
            ResizeDirection::SouthEast
        );
        assert_eq!(resize_direction(Edge::Left), ResizeDirection::West);
    }

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
