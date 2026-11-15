//! The Studio panel in the window: how the window is shared between the video
//! and the panel, in the two units the pieces speak. The sharing itself is
//! `dockview_domain::layout::studio_layout` (physical pixels, for the
//! renderer); the interface works in logical pixels, so this converts.

use dockview_domain::layout::{studio_layout, Rect as PhysicalRect};
use dockview_ui::Rect;

/// Space around the video and the panel while the Studio is open, in logical
/// pixels: both become cards with air around them (it grows with the opening).
pub const CARD_MARGIN: f32 = 12.0;

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
    /// The video's rectangle in logical pixels (where its frame is drawn and
    /// where what floats over it is anchored).
    pub video_logical: Rect,
}

/// Shares a `window` (physical pixels) at interface `scale`, with the panel
/// `openness` open (0 to 1).
pub fn place(window: (u32, u32), scale: f32, openness: f32) -> StudioPlace {
    let scale = scale.max(0.1);
    let layout = studio_layout(window, scale, openness);
    let full = studio_layout(window, scale, 1.0);
    let logical = |r: PhysicalRect| Rect::new(r.x / scale, r.y / scale, r.w / scale, r.h / scale);
    // With the panel out, both it and the video are cards with a margin: the
    // video has it at left, top and bottom and half of it to the panel; the
    // panel has it at right, top and bottom and the other half to the video.
    let (video, panel) = match layout.panel {
        None => (layout.video, None),
        Some(p) => {
            let m = (CARD_MARGIN * scale * openness.clamp(0.0, 1.0)).round();
            let v = layout.video;
            (
                PhysicalRect {
                    x: v.x + m,
                    y: v.y + m,
                    w: (v.w - m * 1.5).max(1.0),
                    h: (v.h - m * 2.0).max(1.0),
                },
                Some(PhysicalRect {
                    x: p.x + m * 0.5,
                    y: p.y + m,
                    w: (p.w - m * 1.5).max(1.0),
                    h: (p.h - m * 2.0).max(1.0),
                }),
            )
        }
    };
    StudioPlace {
        video,
        panel: panel.map(logical),
        full_width: full.panel.map_or(0.0, |p| p.w / scale),
        video_width: video.w / scale,
        video_logical: logical(video),
    }
}

/// The air around the two cards of the Studio, and a fine frame around the
/// video. `content` is the size of the area they share, in logical pixels, and
/// `openness` how far the panel is out (0 draws nothing).
pub fn draw_card_frame(
    ui: &mut dockview_ui::Ui,
    theme: &dockview_domain::theme::Theme,
    place: &StudioPlace,
    content: (f32, f32),
    openness: f32,
) {
    let Some(panel) = place.panel else {
        return;
    };
    if openness <= 0.0 {
        return;
    }
    let (w, h) = content;
    let v = place.video_logical;
    ui.set_opacity(openness.clamp(0.0, 1.0));
    let air = theme.backdrop;
    // Above, below, left of the video; between the cards; right of the panel.
    for r in [
        Rect::new(0.0, 0.0, w, v.y),
        Rect::new(0.0, v.y + v.h, w, (h - v.y - v.h).max(0.0)),
        Rect::new(0.0, v.y, v.x, v.h),
        Rect::new(v.x + v.w, v.y, (panel.x - v.x - v.w).max(0.0), v.h),
        Rect::new(
            panel.x + panel.w,
            v.y,
            (w - panel.x - panel.w).max(0.0),
            v.h,
        ),
    ] {
        if r.w > 0.0 && r.h > 0.0 {
            ui.rect(r, 0.0, air);
        }
    }
    ui.outline(v, 1.0, theme.border);
    ui.shadow(
        panel,
        dockview_domain::design::RADIUS_CARD,
        18.0,
        theme.shadow,
    );
    ui.set_opacity(1.0);
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
        assert_eq!(p.video_logical, Rect::new(0.0, 0.0, 1920.0, 1080.0));
    }

    #[test]
    fn open_the_panel_and_the_video_are_cards_with_a_margin_around_them() {
        let p = place((1920, 1080), 1.0, 1.0);
        let panel = p.panel.expect("open");
        let m = CARD_MARGIN;
        // The video: margin at left, top and bottom; half of it towards the panel.
        assert_eq!((p.video.x, p.video.y, p.video.h), (m, m, 1080.0 - 2.0 * m));
        assert_eq!(p.video.w, 1600.0 - 1.5 * m);
        // The panel: the other half towards the video, the margin at right, top and bottom.
        assert_eq!(
            (panel.x, panel.y, panel.h),
            (1600.0 + m / 2.0, m, 1080.0 - 2.0 * m)
        );
        assert_eq!(panel.w, 320.0 - 1.5 * m);
        assert_eq!(panel.x + panel.w, 1920.0 - m, "the margin at the right");
        // A gap of one margin between the two cards.
        assert_eq!(panel.x - (p.video.x + p.video.w), m);
        assert_eq!(p.video_width, p.video.w);
        assert_eq!(p.full_width, 320.0);
    }

    #[test]
    fn at_a_higher_scale_the_panel_is_the_same_in_logical_pixels_and_so_is_the_margin() {
        let p = place((2880, 1620), 1.5, 1.0);
        let panel = p.panel.expect("open");
        assert_eq!(p.full_width, 320.0);
        assert_eq!(panel.w, 320.0 - 1.5 * CARD_MARGIN);
        assert_eq!(
            p.video.x,
            CARD_MARGIN * 1.5,
            "physical pixels for the renderer"
        );
        assert_eq!(p.video_logical.x, CARD_MARGIN);
        assert_eq!(panel.x + panel.w, 2880.0 / 1.5 - CARD_MARGIN);
    }

    #[test]
    fn while_it_opens_the_margin_grows_with_the_panel() {
        let closed = place((1920, 1080), 1.0, 0.0);
        let half = place((1920, 1080), 1.0, 0.5);
        let open = place((1920, 1080), 1.0, 1.0);
        assert_eq!(closed.video.x, 0.0);
        assert!(half.video.x > 0.0 && half.video.x < open.video.x);
        assert_eq!(half.full_width, 320.0);
    }

    #[test]
    fn a_small_window_never_gives_the_panel_more_than_its_share() {
        let p = place((600, 400), 1.0, 1.0);
        let panel = p.panel.expect("open");
        assert_eq!(p.full_width, 270.0, "45 % of 600");
        assert_eq!(panel.w, 270.0 - 1.5 * CARD_MARGIN);
        assert!(p.video.w + panel.w < 600.0);
    }

    #[test]
    fn a_weird_scale_does_not_divide_by_zero() {
        let p = place((1280, 720), 0.0, 1.0);
        assert!(p.video_width.is_finite() && p.full_width.is_finite());
    }
}
