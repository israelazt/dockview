//! The arithmetic of a vertical list that scrolls: how far it can go, what a
//! turn of the wheel does and where the scroll bar's thumb is. Pure, so it is
//! tested without a window.

/// Logical pixels a notch of the wheel moves the list.
pub const NOTCH_PX: f32 = 56.0;
/// The shortest the scroll bar's thumb is drawn.
pub const MIN_THUMB: f32 = 24.0;

/// The farthest the list can scroll: how much of the content does not fit.
pub fn max_scroll(content_h: f32, viewport_h: f32) -> f32 {
    (content_h - viewport_h).max(0.0)
}

/// Keeps `scroll` inside what the content allows.
pub fn clamp_scroll(scroll: f32, content_h: f32, viewport_h: f32) -> f32 {
    scroll.clamp(0.0, max_scroll(content_h, viewport_h))
}

/// `scroll` after `notches` of the wheel (positive is up, towards the top of
/// the list), kept inside the content.
pub fn scroll_by(scroll: f32, notches: f32, content_h: f32, viewport_h: f32) -> f32 {
    clamp_scroll(scroll - notches * NOTCH_PX, content_h, viewport_h)
}

/// The scroll bar's thumb as `(offset, length)` inside a track as tall as the
/// viewport; `None` when everything fits and no bar is needed.
pub fn thumb(content_h: f32, viewport_h: f32, scroll: f32) -> Option<(f32, f32)> {
    let hidden = max_scroll(content_h, viewport_h);
    if hidden <= 0.0 || content_h <= 0.0 {
        return None;
    }
    let length = (viewport_h * viewport_h / content_h)
        .max(MIN_THUMB)
        .min(viewport_h);
    let travel = viewport_h - length;
    let offset = travel * (scroll / hidden).clamp(0.0, 1.0);
    Some((offset, length))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_that_fits_cannot_scroll_and_taller_content_scrolls_by_the_difference() {
        assert_eq!(max_scroll(300.0, 400.0), 0.0);
        assert_eq!(max_scroll(400.0, 400.0), 0.0);
        assert_eq!(max_scroll(900.0, 400.0), 500.0);
    }

    #[test]
    fn the_wheel_scrolls_down_for_negative_notches_and_stops_at_both_ends() {
        let (content, view) = (900.0, 400.0);
        assert_eq!(scroll_by(0.0, -1.0, content, view), NOTCH_PX);
        assert_eq!(scroll_by(NOTCH_PX, 1.0, content, view), 0.0);
        assert_eq!(scroll_by(0.0, 3.0, content, view), 0.0, "not above the top");
        assert_eq!(
            scroll_by(490.0, -5.0, content, view),
            500.0,
            "not past the end"
        );
    }

    #[test]
    fn nothing_scrolls_when_everything_fits() {
        assert_eq!(scroll_by(0.0, -4.0, 200.0, 400.0), 0.0);
        assert_eq!(clamp_scroll(80.0, 200.0, 400.0), 0.0);
    }

    #[test]
    fn a_shrinking_list_pulls_the_scroll_back_in() {
        // A section folded while scrolled to the bottom.
        assert_eq!(clamp_scroll(500.0, 700.0, 400.0), 300.0);
    }

    #[test]
    fn the_thumb_is_in_proportion_and_travels_the_whole_track() {
        assert_eq!(thumb(300.0, 400.0, 0.0), None, "no bar when it fits");
        let (offset, length) = thumb(800.0, 400.0, 0.0).unwrap();
        assert_eq!((offset, length), (0.0, 200.0));
        let (offset, length) = thumb(800.0, 400.0, 400.0).unwrap();
        assert_eq!(
            offset + length,
            400.0,
            "at the end the thumb touches the bottom"
        );
        let (offset, _) = thumb(800.0, 400.0, 200.0).unwrap();
        assert_eq!(offset, 100.0, "half way is half way");
    }

    #[test]
    fn a_very_long_list_still_gets_a_thumb_you_can_see() {
        let (_, length) = thumb(100_000.0, 400.0, 0.0).unwrap();
        assert_eq!(length, MIN_THUMB);
    }
}
