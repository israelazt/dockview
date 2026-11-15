//! The measures the design repeats everywhere, in logical pixels: how round
//! things are and how small the window may get. The colours are in `theme`.

/// Cards and panels.
pub const RADIUS_CARD: f32 = 16.0;
/// Buttons, rows and icons that light up.
pub const RADIUS_CONTROL: f32 = 10.0;
/// Chips, labels and small bars.
pub const RADIUS_CHIP: f32 = 8.0;

/// The smallest the window can be made: below it the frame (title bar, row,
/// rail, strip) and the Studio panel no longer leave a useful picture.
pub const MIN_WINDOW: (f32, f32) = (960.0, 540.0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_radii_get_smaller_from_cards_to_chips() {
        const { assert!(RADIUS_CARD > RADIUS_CONTROL && RADIUS_CONTROL > RADIUS_CHIP) };
    }

    #[test]
    fn the_minimum_window_fits_the_frame_and_the_panel_with_room_for_a_picture() {
        use crate::layout::PANEL_WIDTH;
        use crate::window_chrome::{NAV_HEIGHT, RAIL_WIDTH, STRIP_HEIGHT, TITLE_BAR_HEIGHT};
        let (w, h) = MIN_WINDOW;
        let picture_w = w - RAIL_WIDTH - PANEL_WIDTH;
        let picture_h = h - TITLE_BAR_HEIGHT - NAV_HEIGHT - STRIP_HEIGHT;
        assert!(picture_w >= 480.0, "{picture_w}");
        assert!(picture_h >= 360.0, "{picture_h}");
    }
}
