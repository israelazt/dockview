//! How the window is shared between the video and the Studio side panel. Plain
//! arithmetic in physical pixels, so it is tested without a window.

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Width of the panel in logical pixels.
pub const PANEL_WIDTH: f32 = 320.0;
/// The panel never takes more than this share of the window, so a small
/// window still shows a useful video.
pub const PANEL_MAX_SHARE: f32 = 0.45;

/// Where the video area and the panel go.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StudioLayout {
    /// The area the video is fitted into (the whole window when closed).
    pub video: Rect,
    /// The panel, on the right; `None` when it takes no room.
    pub panel: Option<Rect>,
}

/// Shares a `window` (physical pixels) at interface `scale`. `openness` goes
/// from 0 (closed) to 1 (fully open) so the opening can be animated.
pub fn studio_layout(window: (u32, u32), scale: f32, openness: f32) -> StudioLayout {
    let (ww, wh) = (window.0 as f32, window.1 as f32);
    let full = (PANEL_WIDTH * scale.max(0.1)).min(ww * PANEL_MAX_SHARE);
    let width = (full * openness.clamp(0.0, 1.0)).round();
    if width < 1.0 {
        return StudioLayout {
            video: Rect {
                x: 0.0,
                y: 0.0,
                w: ww,
                h: wh,
            },
            panel: None,
        };
    }
    StudioLayout {
        video: Rect {
            x: 0.0,
            y: 0.0,
            w: ww - width,
            h: wh,
        },
        panel: Some(Rect {
            x: ww - width,
            y: 0.0,
            w: width,
            h: wh,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_gives_the_whole_window_to_the_video() {
        let l = studio_layout((1920, 1080), 1.0, 0.0);
        assert_eq!(
            l.video,
            Rect {
                x: 0.0,
                y: 0.0,
                w: 1920.0,
                h: 1080.0
            }
        );
        assert_eq!(l.panel, None);
    }

    #[test]
    fn open_takes_the_panel_width_from_the_right() {
        let l = studio_layout((1920, 1080), 1.0, 1.0);
        assert_eq!(l.video.w, 1600.0);
        assert_eq!(
            l.panel,
            Some(Rect {
                x: 1600.0,
                y: 0.0,
                w: 320.0,
                h: 1080.0
            })
        );
    }

    #[test]
    fn the_panel_grows_with_the_interface_scale() {
        let l = studio_layout((1920, 1080), 1.5, 1.0);
        assert_eq!(l.panel.unwrap().w, 480.0);
        assert_eq!(l.video.w, 1440.0);
    }

    #[test]
    fn a_small_window_keeps_most_of_its_room_for_the_video() {
        let l = studio_layout((640, 360), 1.0, 1.0);
        assert_eq!(l.panel.unwrap().w, 288.0);
        assert_eq!(l.video.w, 352.0);
    }

    #[test]
    fn half_open_is_half_the_width() {
        let l = studio_layout((1920, 1080), 1.0, 0.5);
        assert_eq!(l.panel.unwrap().w, 160.0);
        assert_eq!(l.video.w + l.panel.unwrap().w, 1920.0);
    }

    #[test]
    fn an_empty_window_does_not_break() {
        let l = studio_layout((0, 0), 1.0, 1.0);
        assert_eq!(l.panel, None);
        assert_eq!(l.video.w, 0.0);
    }

    #[test]
    fn the_panel_contains_its_own_pixels_only() {
        let panel = studio_layout((1920, 1080), 1.0, 1.0).panel.unwrap();
        assert!(panel.contains(1600.0, 0.0));
        assert!(!panel.contains(1599.0, 10.0));
        assert!(!panel.contains(1920.0, 10.0));
    }
}
