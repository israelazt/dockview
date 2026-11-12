//! The recording badge: a red dot and the elapsed time at the top-right of the
//! window. It stays on screen while recording, even when the controls hide.
//! It is drawn on the window only, never into the recorded video.

use super::text::Weight;
use super::{Rect, TextStyle, Ui};
use crate::theme::Theme;

const MARGIN: f32 = 18.0;
const HEIGHT: f32 = 34.0;
const PAD_X: f32 = 14.0;
const DOT: f32 = 10.0;
const STYLE: TextStyle = TextStyle::new(13.0, Weight::SemiBold);

/// `MM:SS`, or `H:MM:SS` from one hour on.
pub fn clock_label(seconds: f32) -> String {
    let total = seconds.max(0.0) as u64;
    let (hours, minutes, secs) = (total / 3600, (total / 60) % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

/// Opacity of the dot: it breathes once per second of footage while recording,
/// and stays dim while the recording is still starting.
fn dot_opacity(seconds: f32, starting: bool, animations: bool) -> f32 {
    if starting {
        0.45
    } else if animations {
        0.65 + 0.35 * (0.5 + 0.5 * (seconds.fract() * std::f32::consts::TAU).cos())
    } else {
        1.0
    }
}

/// Draws the badge. `seconds` is the footage written so far.
pub fn draw(ui: &mut Ui, theme: &Theme, seconds: f32, starting: bool, animations: bool) {
    let label = clock_label(seconds);
    // Reserve room for the widest time of its length, so it does not jitter.
    let widest = label.replace(|c: char| c.is_ascii_digit(), "0");
    let text_w = ui.measure(&widest, STYLE.tabular()).0;
    let width = PAD_X * 2.0 + DOT + 8.0 + text_w;
    let (screen_w, _) = ui.logical_size();
    let rect = Rect::new(
        (screen_w - MARGIN - width).round(),
        MARGIN,
        width.round(),
        HEIGHT,
    );
    let cy = rect.y + HEIGHT / 2.0;

    ui.shadow(rect, HEIGHT / 2.0, 16.0, theme.shadow);
    ui.panel(rect, HEIGHT / 2.0, theme.surface, theme.border, 1.0);
    let alpha = dot_opacity(seconds, starting, animations);
    ui.rect(
        Rect::new(rect.x + PAD_X, cy - DOT / 2.0, DOT, DOT),
        DOT / 2.0,
        theme.error.with_alpha(alpha),
    );
    let line_h = ui.measure("Ag", STYLE).1;
    ui.text(
        &label,
        rect.x + PAD_X + DOT + 8.0,
        cy - line_h / 2.0,
        STYLE.tabular(),
        theme.text,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_reads_minutes_then_hours() {
        assert_eq!(clock_label(0.0), "00:00");
        assert_eq!(clock_label(9.9), "00:09");
        assert_eq!(clock_label(65.0), "01:05");
        assert_eq!(clock_label(3599.0), "59:59");
        assert_eq!(clock_label(3600.0), "1:00:00");
        assert_eq!(clock_label(7384.0), "2:03:04");
        assert_eq!(clock_label(-5.0), "00:00");
    }

    #[test]
    fn the_dot_breathes_while_recording_and_is_dim_while_starting() {
        assert!(dot_opacity(0.0, true, true) < 0.5);
        let (a, b) = (dot_opacity(0.0, false, true), dot_opacity(0.5, false, true));
        assert!(a > b, "brightest at the start of each second");
        assert!((0.0..=1.0).contains(&a) && (0.0..=1.0).contains(&b));
        assert_eq!(
            dot_opacity(0.3, false, false),
            1.0,
            "steady without animations"
        );
    }

    #[test]
    fn the_badge_draws_inside_the_window() {
        let mut ui = Ui::new();
        ui.begin((1280, 720), 1.0);
        draw(&mut ui, &Theme::dark(), 125.0, false, true);
        assert!(!ui.vertices().is_empty());
    }
}
