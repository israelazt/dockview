//! The Studio panel in the window: how the window is shared between the video
//! and the panel, in the two units the pieces speak. The sharing itself is
//! `dockview_domain::layout::studio_layout` (physical pixels, for the
//! renderer); the interface works in logical pixels, so this converts.

use dockview_domain::layout::{studio_layout, Rect as PhysicalRect};
use dockview_ui::Rect;

/// The room the video and the panel have in this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StudioPlace {
    /// Where the video is fitted, in physical pixels (for the renderer).
    pub video: PhysicalRect,
    /// The room the panel has now, in logical pixels; `None` when it is closed.
    pub panel: Option<Rect>,
    /// What the panel measures when fully open, in logical pixels.
    pub full_width: f32,
    /// The width left for the video, in logical pixels: what the controls over
    /// it anchor to.
    pub video_width: f32,
}

/// Shares a `window` (physical pixels) at interface `scale`, with the panel
/// `openness` open (0 to 1).
pub fn place(window: (u32, u32), scale: f32, openness: f32) -> StudioPlace {
    let scale = scale.max(0.1);
    let layout = studio_layout(window, scale, openness);
    let full = studio_layout(window, scale, 1.0);
    let logical = |r: PhysicalRect| Rect::new(r.x / scale, r.y / scale, r.w / scale, r.h / scale);
    StudioPlace {
        video: layout.video,
        panel: layout.panel.map(logical),
        full_width: full.panel.map_or(0.0, |p| p.w / scale),
        video_width: layout.video.w / scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_the_video_has_the_whole_window_and_there_is_no_panel() {
        let p = place((1920, 1080), 1.0, 0.0);
        assert_eq!(p.panel, None);
        assert_eq!((p.video.x, p.video.w, p.video.h), (0.0, 1920.0, 1080.0));
        assert_eq!(p.video_width, 1920.0);
    }

    #[test]
    fn open_the_panel_takes_its_width_and_the_video_the_rest() {
        let p = place((1920, 1080), 1.0, 1.0);
        let panel = p.panel.expect("open");
        assert_eq!((panel.x, panel.w, panel.h), (1600.0, 320.0, 1080.0));
        assert_eq!(p.video.w, 1600.0);
        assert_eq!(p.video_width, 1600.0);
        assert_eq!(p.full_width, 320.0);
    }

    #[test]
    fn at_a_higher_scale_the_panel_is_the_same_in_logical_pixels() {
        let p = place((2880, 1620), 1.5, 1.0);
        let panel = p.panel.expect("open");
        assert_eq!(panel.w, 320.0, "320 logical pixels whatever the scale");
        assert_eq!(p.full_width, 320.0);
        assert_eq!(
            p.video.w,
            2880.0 - 480.0,
            "the renderer gets physical pixels"
        );
        assert_eq!(p.video_width, (2880.0 - 480.0) / 1.5);
        assert_eq!(panel.x + panel.w, 2880.0 / 1.5);
    }

    #[test]
    fn while_it_opens_the_panel_grows_and_the_full_width_stays() {
        let half = place((1920, 1080), 1.0, 0.5);
        let panel = half.panel.expect("opening");
        assert_eq!(panel.w, 160.0);
        assert_eq!(half.full_width, 320.0);
        assert_eq!(half.video.w, 1760.0);
    }

    #[test]
    fn a_small_window_never_gives_the_panel_more_than_its_share() {
        let p = place((600, 400), 1.0, 1.0);
        let panel = p.panel.expect("open");
        assert_eq!(panel.w, 270.0, "45 % of 600");
        assert_eq!(p.full_width, 270.0);
        assert_eq!(p.video.w + panel.w, 600.0);
    }

    #[test]
    fn a_weird_scale_does_not_divide_by_zero() {
        let p = place((1280, 720), 0.0, 1.0);
        assert!(p.video_width.is_finite() && p.full_width.is_finite());
    }
}
