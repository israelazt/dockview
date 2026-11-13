//! Small pure helpers for the controls: fades and rounding (the smoothing and
//! easing are in `crate::motion`).

use super::consts::FADE_SECS;
pub(super) use crate::motion::{ease_out_cubic, lerp_color, move_toward, smooth};

/// Moves `visibility` towards shown/hidden at the fade speed.
pub(super) fn step_visibility(visibility: f32, show: bool, dt: f32) -> f32 {
    let step = dt / FADE_SECS;
    if show {
        (visibility + step).min(1.0)
    } else {
        (visibility - step).max(0.0)
    }
}

/// Rounds a 0–1 volume to whole percent so values stay stable and exact.
pub(super) fn round_percent(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}
