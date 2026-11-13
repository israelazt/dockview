//! Which part of the window the pointer belongs to: the video (where the
//! floating controls are) or the Studio panel. A pointer over the panel is not
//! over the video, so it must not wake the pill, scroll the volume or close a
//! popover, and the other way round.

use super::super::input::Pointer;
use super::super::Rect;

/// The pointer as the controls over the video see it, and as the Studio panel
/// sees it. The side the pointer is not on gets no position, no wheel and no
/// new press, but keeps the held button and the release, so a drag that began
/// on one side still ends properly after leaving it. `panel` is the part of
/// the panel on screen (`None` when it is closed).
pub fn route_pointer(pointer: &Pointer, panel: Option<Rect>) -> (Pointer, Pointer) {
    let over_panel = match (pointer.pos, panel) {
        (Some((x, y)), Some(area)) => area.contains(x, y),
        _ => false,
    };
    let aside = Pointer {
        pos: None,
        wheel: 0.0,
        just_pressed: false,
        ..*pointer
    };
    if over_panel {
        (aside, *pointer)
    } else {
        (*pointer, aside)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> Option<Rect> {
        Some(Rect::new(960.0, 0.0, 320.0, 720.0))
    }

    fn at(x: f32, y: f32) -> Pointer {
        Pointer {
            pos: Some((x, y)),
            down: true,
            just_pressed: true,
            just_released: false,
            wheel: 2.0,
        }
    }

    #[test]
    fn a_pointer_over_the_panel_belongs_to_the_panel_alone() {
        let p = at(1000.0, 300.0);
        let (video, side) = route_pointer(&p, panel());
        assert_eq!(side, p);
        assert_eq!(video.pos, None);
        assert_eq!(video.wheel, 0.0, "the wheel must not change the volume");
        assert!(!video.just_pressed, "a click must not reach the video");
        assert!(video.down, "the held button is kept");
    }

    #[test]
    fn a_pointer_over_the_video_belongs_to_the_video_alone() {
        let p = at(400.0, 300.0);
        let (video, side) = route_pointer(&p, panel());
        assert_eq!(video, p);
        assert_eq!(side.pos, None);
        assert_eq!(side.wheel, 0.0, "the wheel must not scroll the panel");
        assert!(!side.just_pressed);
    }

    #[test]
    fn the_release_reaches_both_sides_so_a_drag_can_end_anywhere() {
        let mut p = at(1000.0, 300.0);
        p.just_pressed = false;
        p.down = false;
        p.just_released = true;
        let (video, side) = route_pointer(&p, panel());
        assert!(video.just_released && side.just_released);
    }

    #[test]
    fn with_no_panel_everything_goes_to_the_video() {
        let p = at(1000.0, 300.0);
        let (video, side) = route_pointer(&p, None);
        assert_eq!(video, p);
        assert_eq!(side.pos, None);
    }

    #[test]
    fn a_pointer_outside_the_window_is_nowhere() {
        let p = Pointer::default();
        let (video, side) = route_pointer(&p, panel());
        assert_eq!((video.pos, side.pos), (None, None));
    }

    #[test]
    fn the_panel_edge_is_inside_the_panel_and_the_pixel_before_is_the_video() {
        let (video, side) = route_pointer(&at(960.0, 10.0), panel());
        assert_eq!((video.pos, side.pos.is_some()), (None, true));
        let (video, side) = route_pointer(&at(959.0, 10.0), panel());
        assert_eq!((video.pos.is_some(), side.pos), (true, None));
    }
}
