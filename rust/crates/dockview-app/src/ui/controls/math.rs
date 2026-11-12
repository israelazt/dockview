//! Small pure helpers for fades, easing and colour.

use crate::theme::Color;

use super::consts::FADE_SECS;

/// Moves `visibility` towards shown/hidden at the fade speed.
pub(super) fn step_visibility(visibility: f32, show: bool, dt: f32) -> f32 {
    let step = dt / FADE_SECS;
    if show {
        (visibility + step).min(1.0)
    } else {
        (visibility - step).max(0.0)
    }
}

/// Blends two colours; `amount` 0.0 gives `a`, 1.0 gives `b`.
pub(super) fn lerp_color(a: Color, b: Color, amount: f32) -> Color {
    let k = amount.clamp(0.0, 1.0);
    let mix = |x: f32, y: f32| x + (y - x) * k;
    Color {
        r: mix(a.r, b.r),
        g: mix(a.g, b.g),
        b: mix(a.b, b.b),
        a: mix(a.a, b.a),
    }
}

/// Exponential smoothing of `current` towards `target`; snaps when close.
pub(super) fn smooth(current: f32, target: f32, dt: f32, rate: f32) -> f32 {
    let next = current + (target - current) * (1.0 - (-rate * dt).exp());
    if (target - next).abs() < 0.002 {
        target
    } else {
        next
    }
}

/// Moves `current` towards `target` by at most `step`.
pub(super) fn move_toward(current: f32, target: f32, step: f32) -> f32 {
    if current < target {
        (current + step).min(target)
    } else {
        (current - step).max(target)
    }
}

/// Fast start, gentle landing.
pub(super) fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// Rounds a 0–1 volume to whole percent so values stay stable and exact.
pub(super) fn round_percent(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}
