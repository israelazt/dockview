//! The frame of the window, which is ours (no system title bar): how tall the
//! title bar is, where its three buttons are, and what the pointer is over at
//! any point (the content, the bar, a button or a resize edge). Plain
//! arithmetic in physical pixels, so it is tested without a window.

use crate::layout::Rect;

/// Height of the title bar in logical pixels.
pub const TITLE_BAR_HEIGHT: f32 = 34.0;
/// Height of the navigation row under the title bar, in logical pixels.
pub const NAV_HEIGHT: f32 = 44.0;
/// Width of the icon rail on the left, in logical pixels.
pub const RAIL_WIDTH: f32 = 52.0;
/// Width of each of the three window buttons in logical pixels.
pub const BUTTON_WIDTH: f32 = 46.0;
/// Thickness of the strip along each edge that resizes the window, in logical
/// pixels.
pub const RESIZE_BAND: f32 = 6.0;
/// How far along an edge a corner reaches, in logical pixels (corners are
/// easier to grab than the thin band).
pub const CORNER_REACH: f32 = 14.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowButton {
    Minimize,
    /// Maximizes, or restores a window that already is.
    Maximize,
    Close,
}

impl WindowButton {
    /// Left to right, as drawn.
    pub const ALL: [WindowButton; 3] = [Self::Minimize, Self::Maximize, Self::Close];
}

/// The edge or corner a resize drag starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// What is under a point of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// The rest of the window: the application itself.
    Content,
    /// The free part of the title bar: dragging it moves the window.
    TitleBar,
    Button(WindowButton),
    Resize(Edge),
    /// The navigation row under the bar: the interface answers there.
    Nav,
    /// The icon rail on the left, under the row.
    Rail,
}

/// The shape the mouse cursor takes over a zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeCursor {
    /// The ordinary arrow (or whatever the content wants).
    Default,
    /// Up and down arrows (top and bottom edges).
    ResizeVertical,
    /// Left and right arrows (left and right edges).
    ResizeHorizontal,
    /// Along the line from top left to bottom right.
    ResizeNwSe,
    /// Along the line from top right to bottom left.
    ResizeNeSw,
}

impl Zone {
    /// The cursor to show over this zone.
    pub fn cursor(self) -> ChromeCursor {
        match self {
            Zone::Resize(Edge::Top | Edge::Bottom) => ChromeCursor::ResizeVertical,
            Zone::Resize(Edge::Left | Edge::Right) => ChromeCursor::ResizeHorizontal,
            Zone::Resize(Edge::TopLeft | Edge::BottomRight) => ChromeCursor::ResizeNwSe,
            Zone::Resize(Edge::TopRight | Edge::BottomLeft) => ChromeCursor::ResizeNeSw,
            _ => ChromeCursor::Default,
        }
    }
}

/// The window as it is now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chrome {
    /// Physical pixels.
    pub size: (u32, u32),
    /// Interface scale (physical pixels per logical pixel).
    pub scale: f32,
    pub maximized: bool,
    /// Full screen shows only the content: no bar and no resize edges.
    pub fullscreen: bool,
    /// The Studio panel is open: only then is there an icon rail (the viewer
    /// gives all its width to the picture).
    pub panel_open: bool,
}

impl Chrome {
    fn px(&self, logical: f32) -> f32 {
        (logical * self.scale.max(0.1)).round()
    }

    /// Height of the title bar in physical pixels (0 in full screen).
    pub fn bar_height(&self) -> f32 {
        if self.fullscreen {
            0.0
        } else {
            self.px(TITLE_BAR_HEIGHT)
        }
    }

    /// The title bar, or `None` in full screen.
    pub fn bar(&self) -> Option<Rect> {
        (!self.fullscreen).then(|| Rect {
            x: 0.0,
            y: 0.0,
            w: self.size.0 as f32,
            h: self.bar_height(),
        })
    }

    /// Height of the navigation row in physical pixels (0 in full screen).
    pub fn nav_height(&self) -> f32 {
        if self.fullscreen {
            0.0
        } else {
            self.px(NAV_HEIGHT)
        }
    }

    /// The navigation row, just under the bar, or `None` in full screen.
    pub fn nav(&self) -> Option<Rect> {
        (!self.fullscreen).then(|| Rect {
            x: 0.0,
            y: self.bar_height(),
            w: self.size.0 as f32,
            h: self.nav_height(),
        })
    }

    /// Width of the icon rail in physical pixels (0 in full screen).
    pub fn rail_width(&self) -> f32 {
        if self.fullscreen || !self.panel_open {
            0.0
        } else {
            self.px(RAIL_WIDTH)
        }
    }

    /// The icon rail, on the left under the row, or `None` in full screen.
    pub fn rail(&self) -> Option<Rect> {
        let top = self.bar_height() + self.nav_height();
        (!self.fullscreen && self.panel_open).then(|| Rect {
            x: 0.0,
            y: top,
            w: self.rail_width(),
            h: (self.size.1 as f32 - top).max(0.0),
        })
    }

    /// Everything right of the rail and below the bar and the row: where the
    /// application draws.
    pub fn content(&self) -> Rect {
        let top = self.bar_height() + self.nav_height();
        let left = self.rail_width();
        Rect {
            x: left,
            y: top,
            w: (self.size.0 as f32 - left).max(0.0),
            h: (self.size.1 as f32 - top).max(0.0),
        }
    }

    /// Where `button` is, or `None` in full screen.
    pub fn button(&self, button: WindowButton) -> Option<Rect> {
        if self.fullscreen {
            return None;
        }
        let width = self.px(BUTTON_WIDTH);
        let slot = match button {
            WindowButton::Minimize => 2.0,
            WindowButton::Maximize => 1.0,
            WindowButton::Close => 0.0,
        };
        Some(Rect {
            x: self.size.0 as f32 - width * (slot + 1.0),
            y: 0.0,
            w: width,
            h: self.bar_height(),
        })
    }

    /// Whether the window can be resized from its edges.
    pub fn resizable(&self) -> bool {
        !self.maximized && !self.fullscreen
    }

    /// What is under the physical point `(x, y)`. Resize edges win over the
    /// bar and its buttons, so the top edge of the window can be grabbed.
    pub fn zone_at(&self, x: f32, y: f32) -> Zone {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        if x < 0.0 || y < 0.0 || x >= w || y >= h {
            return Zone::Content;
        }
        if self.resizable() {
            if let Some(edge) = self.edge_at(x, y) {
                return Zone::Resize(edge);
            }
        }
        if y < self.bar_height() {
            for button in WindowButton::ALL {
                if self.button(button).is_some_and(|r| r.contains(x, y)) {
                    return Zone::Button(button);
                }
            }
            return Zone::TitleBar;
        }
        if y < self.bar_height() + self.nav_height() {
            return Zone::Nav;
        }
        if x < self.rail_width() {
            return Zone::Rail;
        }
        Zone::Content
    }

    fn edge_at(&self, x: f32, y: f32) -> Option<Edge> {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        let (band, reach) = (self.px(RESIZE_BAND), self.px(CORNER_REACH));
        let left = x < band;
        let right = x >= w - band;
        let top = y < band;
        let bottom = y >= h - band;
        let near_left = x < reach;
        let near_right = x >= w - reach;
        let near_top = y < reach;
        let near_bottom = y >= h - reach;
        match (
            (left && near_top) || (top && near_left),
            (right && near_top) || (top && near_right),
            (left && near_bottom) || (bottom && near_left),
            (right && near_bottom) || (bottom && near_right),
        ) {
            (true, ..) => Some(Edge::TopLeft),
            (_, true, ..) => Some(Edge::TopRight),
            (_, _, true, _) => Some(Edge::BottomLeft),
            (.., true) => Some(Edge::BottomRight),
            _ if top => Some(Edge::Top),
            _ if bottom => Some(Edge::Bottom),
            _ if left => Some(Edge::Left),
            _ if right => Some(Edge::Right),
            _ => None,
        }
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
            panel_open: true,
        }
    }

    #[test]
    fn the_bar_is_34_logical_pixels_and_scales() {
        assert_eq!(chrome().bar_height(), 34.0);
        let hi = Chrome {
            scale: 2.0,
            ..chrome()
        };
        assert_eq!(hi.bar_height(), 68.0);
        assert_eq!(hi.bar().unwrap().h, 68.0);
        assert_eq!(hi.nav().unwrap().y, 68.0);
        assert_eq!(hi.content().y, 68.0 + 2.0 * NAV_HEIGHT);
        assert_eq!(hi.content().h, 720.0 - 68.0 - 2.0 * NAV_HEIGHT);
    }

    #[test]
    fn the_buttons_sit_at_the_right_end_in_order_and_do_not_overlap() {
        let c = chrome();
        let min = c.button(WindowButton::Minimize).unwrap();
        let max = c.button(WindowButton::Maximize).unwrap();
        let close = c.button(WindowButton::Close).unwrap();
        assert_eq!(close.x + close.w, 1280.0);
        assert_eq!(max.x + max.w, close.x);
        assert_eq!(min.x + min.w, max.x);
        assert_eq!((close.w, close.h), (46.0, 34.0));
    }

    #[test]
    fn each_part_of_the_bar_answers_for_itself() {
        let c = chrome();
        assert_eq!(c.zone_at(600.0, 17.0), Zone::TitleBar);
        assert_eq!(c.zone_at(20.0, 17.0), Zone::TitleBar);
        assert_eq!(c.zone_at(1250.0, 17.0), Zone::Button(WindowButton::Close));
        assert_eq!(
            c.zone_at(1210.0, 17.0),
            Zone::Button(WindowButton::Maximize)
        );
        assert_eq!(
            c.zone_at(1160.0, 17.0),
            Zone::Button(WindowButton::Minimize)
        );
        assert_eq!(c.zone_at(1100.0, 17.0), Zone::TitleBar);
        assert_eq!(c.zone_at(600.0, 300.0), Zone::Content);
    }

    #[test]
    fn the_navigation_row_sits_under_the_bar_and_answers_for_itself() {
        let c = chrome();
        assert_eq!(c.nav().unwrap().y, 34.0);
        assert_eq!(c.nav().unwrap().h, NAV_HEIGHT);
        assert_eq!(c.zone_at(600.0, 40.0), Zone::Nav);
        assert_eq!(c.zone_at(600.0, 77.0), Zone::Nav);
        assert_eq!(c.zone_at(600.0, 78.0), Zone::Content);
        assert_eq!(c.content().y, 34.0 + NAV_HEIGHT);
        assert_eq!(c.content().h, 720.0 - 34.0 - NAV_HEIGHT);
    }

    #[test]
    fn without_the_panel_there_is_no_rail_and_the_content_has_the_width() {
        let c = Chrome {
            panel_open: false,
            ..chrome()
        };
        assert!(c.rail().is_none());
        assert_eq!(c.rail_width(), 0.0);
        assert_eq!(c.zone_at(20.0, 300.0), Zone::Content);
        assert_eq!(c.content().x, 0.0);
        assert_eq!(c.content().w, 1280.0);
        // The bar and the row do not depend on it.
        assert_eq!(c.zone_at(600.0, 40.0), Zone::Nav);
    }

    #[test]
    fn the_rail_is_on_the_left_under_the_row_and_the_content_is_what_is_left() {
        let c = chrome();
        let rail = c.rail().unwrap();
        assert_eq!(
            (rail.x, rail.y, rail.w),
            (0.0, 34.0 + NAV_HEIGHT, RAIL_WIDTH)
        );
        assert_eq!(rail.h, 720.0 - 34.0 - NAV_HEIGHT);
        assert_eq!(c.zone_at(20.0, 300.0), Zone::Rail);
        assert_eq!(c.zone_at(RAIL_WIDTH - 1.0, 300.0), Zone::Rail);
        assert_eq!(c.zone_at(RAIL_WIDTH, 300.0), Zone::Content);
        assert_eq!(c.content().x, RAIL_WIDTH);
        assert_eq!(c.content().w, 1280.0 - RAIL_WIDTH);
        // The row runs over the rail, and the left edge still resizes.
        assert_eq!(c.zone_at(20.0, 60.0), Zone::Nav);
        assert_eq!(c.zone_at(2.0, 300.0), Zone::Resize(Edge::Left));
        let hi = Chrome { scale: 2.0, ..c };
        assert_eq!(hi.content().x, RAIL_WIDTH * 2.0);
        // The side edges still resize from the row.
        assert_eq!(c.zone_at(2.0, 60.0), Zone::Resize(Edge::Left));
    }

    #[test]
    fn the_edges_and_corners_resize_and_win_over_the_bar() {
        let c = chrome();
        assert_eq!(c.zone_at(640.0, 2.0), Zone::Resize(Edge::Top));
        assert_eq!(c.zone_at(640.0, 717.0), Zone::Resize(Edge::Bottom));
        assert_eq!(c.zone_at(2.0, 300.0), Zone::Resize(Edge::Left));
        assert_eq!(c.zone_at(1277.0, 300.0), Zone::Resize(Edge::Right));
        assert_eq!(c.zone_at(2.0, 2.0), Zone::Resize(Edge::TopLeft));
        assert_eq!(c.zone_at(1277.0, 2.0), Zone::Resize(Edge::TopRight));
        assert_eq!(c.zone_at(2.0, 717.0), Zone::Resize(Edge::BottomLeft));
        assert_eq!(c.zone_at(1277.0, 717.0), Zone::Resize(Edge::BottomRight));
        // The corner reaches further along the edge than the band is thick.
        assert_eq!(c.zone_at(10.0, 2.0), Zone::Resize(Edge::TopLeft));
        assert_eq!(c.zone_at(2.0, 10.0), Zone::Resize(Edge::TopLeft));
        assert_eq!(c.zone_at(20.0, 2.0), Zone::Resize(Edge::Top));
        // Just inside the band, the bar answers again.
        assert_eq!(c.zone_at(640.0, 7.0), Zone::TitleBar);
        assert_eq!(c.zone_at(1250.0, 7.0), Zone::Button(WindowButton::Close));
    }

    #[test]
    fn the_band_scales_with_the_interface() {
        let hi = Chrome {
            scale: 2.0,
            ..chrome()
        };
        assert_eq!(hi.zone_at(640.0, 10.0), Zone::Resize(Edge::Top));
        assert_eq!(hi.zone_at(640.0, 13.0), Zone::TitleBar);
    }

    #[test]
    fn a_maximized_window_has_no_resize_edges() {
        let c = Chrome {
            maximized: true,
            ..chrome()
        };
        assert!(!c.resizable());
        assert_eq!(c.zone_at(640.0, 1.0), Zone::TitleBar);
        assert_eq!(c.zone_at(1.0, 300.0), Zone::Rail);
        assert_eq!(c.zone_at(1279.0, 1.0), Zone::Button(WindowButton::Close));
    }

    #[test]
    fn full_screen_is_all_content() {
        let c = Chrome {
            fullscreen: true,
            ..chrome()
        };
        assert!(c.bar().is_none());
        assert!(c.button(WindowButton::Close).is_none());
        assert_eq!(c.zone_at(640.0, 1.0), Zone::Content);
        assert_eq!(c.zone_at(1279.0, 1.0), Zone::Content);
        assert_eq!(c.zone_at(600.0, 40.0), Zone::Content);
        assert!(c.nav().is_none() && c.rail().is_none());
        assert_eq!(c.zone_at(20.0, 300.0), Zone::Content);
        assert_eq!(c.content().x, 0.0);
        assert_eq!(c.content().y, 0.0);
        assert_eq!(c.content().h, 720.0);
    }

    #[test]
    fn each_edge_has_the_cursor_that_points_along_it() {
        assert_eq!(
            Zone::Resize(Edge::Top).cursor(),
            ChromeCursor::ResizeVertical
        );
        assert_eq!(
            Zone::Resize(Edge::Bottom).cursor(),
            ChromeCursor::ResizeVertical
        );
        assert_eq!(
            Zone::Resize(Edge::Left).cursor(),
            ChromeCursor::ResizeHorizontal
        );
        assert_eq!(
            Zone::Resize(Edge::TopLeft).cursor(),
            ChromeCursor::ResizeNwSe
        );
        assert_eq!(
            Zone::Resize(Edge::BottomRight).cursor(),
            ChromeCursor::ResizeNwSe
        );
        assert_eq!(
            Zone::Resize(Edge::TopRight).cursor(),
            ChromeCursor::ResizeNeSw
        );
        assert_eq!(
            Zone::Resize(Edge::BottomLeft).cursor(),
            ChromeCursor::ResizeNeSw
        );
        assert_eq!(Zone::TitleBar.cursor(), ChromeCursor::Default);
        assert_eq!(
            Zone::Button(WindowButton::Close).cursor(),
            ChromeCursor::Default
        );
    }

    #[test]
    fn a_point_outside_the_window_is_content() {
        let c = chrome();
        assert_eq!(c.zone_at(-1.0, 5.0), Zone::Content);
        assert_eq!(c.zone_at(1280.0, 5.0), Zone::Content);
        assert_eq!(c.zone_at(5.0, 720.0), Zone::Content);
    }

    #[test]
    fn a_tiny_window_still_answers_without_panicking() {
        let c = Chrome {
            size: (60, 40),
            ..chrome()
        };
        let _ = c.zone_at(30.0, 20.0);
        assert!(c.content().h >= 0.0);
    }
}
