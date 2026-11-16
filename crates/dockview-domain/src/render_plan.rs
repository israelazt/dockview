//! What each render preset means: how the picture is scaled to the window and
//! how many pictures a second are shown. Pure rules; the renderer and the
//! window apply them.

use std::time::Duration;

use crate::settings::RenderPreset;

/// How the video is scaled to fit the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleFilter {
    /// Each texel is repeated: the cheapest, and the sharpest edges.
    Nearest,
    /// Bilinear: smooth, and cheap.
    Linear,
    /// Catmull-Rom: the sharpest smooth scaling, using more of the GPU.
    Bicubic,
}

/// What a preset asks of the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderPlan {
    pub filter: ScaleFilter,
    /// The most pictures a second shown; `None` shows every one that arrives.
    /// This only limits what is drawn: recordings and the replay buffer still
    /// get every frame.
    pub max_video_fps: Option<u32>,
}

/// The plan for a preset.
pub fn render_plan(preset: RenderPreset) -> RenderPlan {
    match preset {
        RenderPreset::Performance => RenderPlan {
            filter: ScaleFilter::Nearest,
            max_video_fps: Some(30),
        },
        RenderPreset::Balanced => RenderPlan {
            filter: ScaleFilter::Linear,
            max_video_fps: None,
        },
        RenderPreset::Quality => RenderPlan {
            filter: ScaleFilter::Bicubic,
            max_video_fps: None,
        },
    }
}

/// The share of the interval that must have passed to show another picture.
/// Below a full interval so a source close to the cap (a 30 fps camera under a
/// 30 fps cap, with its jitter) is never thinned out, yet above a half so a
/// source at twice the cap is cut exactly in half.
const DUE_SHARE: f64 = 0.75;

/// Whether a picture that just arrived should be shown, given the time since
/// the last one was shown.
pub fn video_frame_due(since_last_shown: Duration, max_fps: Option<u32>) -> bool {
    match max_fps {
        None | Some(0) => true,
        Some(fps) => since_last_shown.as_secs_f64() >= DUE_SHARE / f64::from(fps),
    }
}

/// The four weights of Catmull-Rom interpolation for the point `t` (0 to 1)
/// between the two middle samples. The video shader computes the same
/// polynomials per axis; this is the reference they are checked against.
pub fn catmull_rom_weights(t: f32) -> [f32; 4] {
    let (t2, t3) = (t * t, t * t * t);
    [
        -0.5 * t3 + t2 - 0.5 * t,
        1.5 * t3 - 2.5 * t2 + 1.0,
        -1.5 * t3 + 2.0 * t2 + 0.5 * t,
        0.5 * t3 - 0.5 * t2,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catmull_rom_passes_through_the_samples_and_its_weights_add_up_to_one() {
        assert_eq!(catmull_rom_weights(0.0), [0.0, 1.0, 0.0, 0.0]);
        let end = catmull_rom_weights(1.0);
        for (got, want) in end.iter().zip([0.0, 0.0, 1.0, 0.0]) {
            assert!((got - want).abs() < 1e-6, "{end:?}");
        }
        for step in 0..=20 {
            let sum: f32 = catmull_rom_weights(step as f32 / 20.0).iter().sum();
            assert!((sum - 1.0).abs() < 1e-5, "t={step}/20 sums to {sum}");
        }
    }

    #[test]
    fn catmull_rom_at_the_middle_is_the_classic_nine_sixteenths() {
        let w = catmull_rom_weights(0.5);
        assert!((w[1] - 0.5625).abs() < 1e-6 && (w[2] - 0.5625).abs() < 1e-6);
        assert!((w[0] + 0.0625).abs() < 1e-6 && (w[3] + 0.0625).abs() < 1e-6);
    }

    #[test]
    fn each_preset_has_its_own_filter_and_only_performance_caps_the_rate() {
        let p = render_plan(RenderPreset::Performance);
        assert_eq!(
            (p.filter, p.max_video_fps),
            (ScaleFilter::Nearest, Some(30))
        );
        let b = render_plan(RenderPreset::Balanced);
        assert_eq!((b.filter, b.max_video_fps), (ScaleFilter::Linear, None));
        let q = render_plan(RenderPreset::Quality);
        assert_eq!((q.filter, q.max_video_fps), (ScaleFilter::Bicubic, None));
    }

    #[test]
    fn without_a_cap_every_picture_is_shown() {
        assert!(video_frame_due(Duration::ZERO, None));
        assert!(video_frame_due(Duration::ZERO, Some(0)));
    }

    #[test]
    fn a_60_fps_source_at_a_30_fps_cap_shows_every_other_picture() {
        let frame = Duration::from_secs_f64(1.0 / 60.0);
        let mut since_last = Duration::ZERO;
        let mut shown = 0;
        for _ in 0..60 {
            since_last += frame;
            if video_frame_due(since_last, Some(30)) {
                shown += 1;
                since_last = Duration::ZERO;
            }
        }
        assert_eq!(shown, 30);
    }

    #[test]
    fn a_30_fps_camera_with_jitter_is_not_thinned_out_by_a_30_fps_cap() {
        // Frames arrive 28 to 39 ms apart.
        for ms in [28, 31, 33, 36, 39] {
            assert!(
                video_frame_due(Duration::from_millis(ms), Some(30)),
                "{ms} ms"
            );
        }
    }

    #[test]
    fn a_picture_is_not_held_back_when_the_source_is_slower_than_the_cap() {
        // A 24 fps source at a 30 fps cap: every picture is on time.
        assert!(video_frame_due(
            Duration::from_secs_f64(1.0 / 24.0),
            Some(30)
        ));
    }

    #[test]
    fn a_picture_too_soon_after_the_last_is_skipped() {
        assert!(!video_frame_due(Duration::from_millis(10), Some(30)));
        assert!(video_frame_due(Duration::from_millis(32), Some(30)));
    }
}
