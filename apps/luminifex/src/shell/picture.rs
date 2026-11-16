//! Where the picture is drawn in the window.

use dockview_domain::layout::Rect as PhysicalRect;

use super::studio::StudioPlace;

/// The area the picture fits in, in physical pixels, or `None` when it fits
/// the whole window. `offset` is what the frame takes at the left and at the
/// top (the rail, the title bar, the row of tabs and the strip); `framed` is
/// whether there is a frame at all; `hidden` is for a mode with a view of its
/// own (Settings), which covers the picture: a pixel is all the picture gets.
pub fn video_area(
    place: &StudioPlace,
    offset: (f32, f32),
    framed: bool,
    hidden: bool,
) -> Option<PhysicalRect> {
    let area = PhysicalRect {
        x: place.video.x + offset.0,
        y: place.video.y + offset.1,
        ..place.video
    };
    if hidden {
        return Some(PhysicalRect {
            w: 1.0,
            h: 1.0,
            ..area
        });
    }
    framed.then_some(area)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::studio::place;

    #[test]
    fn the_picture_fits_the_room_the_frame_leaves() {
        let p = place((1280, 600), 1.0, 0.0);
        let a = video_area(&p, (50.0, 100.0), true, false).expect("framed");
        assert_eq!((a.x, a.y), (p.video.x + 50.0, p.video.y + 100.0));
        assert_eq!((a.w, a.h), (p.video.w, p.video.h));
    }

    #[test]
    fn without_a_frame_the_picture_fits_the_whole_window() {
        let p = place((1280, 720), 1.0, 0.0);
        assert_eq!(video_area(&p, (0.0, 0.0), false, false), None);
    }

    #[test]
    fn a_mode_with_a_view_of_its_own_leaves_the_picture_a_single_pixel() {
        let p = place((1280, 600), 1.0, 0.0);
        let a = video_area(&p, (0.0, 80.0), true, true).expect("hidden is still an area");
        assert_eq!((a.w, a.h), (1.0, 1.0));
        let whole = video_area(&p, (0.0, 0.0), false, true).expect("even with no frame");
        assert_eq!((whole.w, whole.h), (1.0, 1.0));
    }
}
