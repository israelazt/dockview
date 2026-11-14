//! Sizes, timings, icons and text styles of the controls.

use super::super::text::Weight;
use super::super::TextStyle;

/// Seconds of pointer inactivity before the controls fade out.
pub const IDLE_HIDE_SECS: f32 = 3.0;
/// Fade duration, in seconds.
pub const FADE_SECS: f32 = 0.2;
/// Audio sync change per click, in milliseconds.
pub const SYNC_STEP_MS: i32 = 10;
/// Volume change per scroll notch.
pub const WHEEL_VOLUME_STEP: f32 = 0.05;
/// Smoothing rate for hover fades (higher is faster; ~95% in 3/rate seconds).
pub(super) const HOVER_RATE: f32 = 22.0;
/// Smoothing rate for the pressed state.
pub(super) const PRESS_RATE: f32 = 34.0;
/// Period of the "connecting" pulse, in seconds.
pub(super) const PULSE_SECS: f32 = 1.2;
/// How long the pointer rests on a control before its tooltip appears.
pub(super) const TOOLTIP_DELAY_SECS: f32 = 0.5;
/// Fade-in time of a tooltip.
pub(super) const TOOLTIP_FADE_SECS: f32 = 0.12;
pub(super) const TOOLTIP_MAX_WIDTH: f32 = 280.0;

pub(super) const PILL_HEIGHT: f32 = 56.0;
pub(super) const BOTTOM_MARGIN: f32 = 28.0;
pub(super) const PAD_X: f32 = 22.0;
pub(super) const GAP: f32 = 18.0;
pub(super) const BUTTON: f32 = 32.0;
pub(super) const ICON_SIZE: f32 = 17.0;
pub(super) const SLIDER_WIDTH: f32 = 96.0;

// Lucide code points.
pub(super) const ICON_VOLUME_MUTE: char = '\u{e1ac}';
pub(super) const ICON_VOLUME_LOW: char = '\u{e1aa}';
pub(super) const ICON_VOLUME_HIGH: char = '\u{e1ab}';
pub(super) const ICON_MINUS: char = '\u{e11c}';
pub(super) const ICON_PLUS: char = '\u{e13d}';
pub(super) const ICON_SETTINGS: char = '\u{e29a}';
pub(super) const ICON_FULLSCREEN: char = '\u{e112}';
pub(super) const ICON_EXIT_FULLSCREEN: char = '\u{e11a}';
pub(super) const ICON_REPLAY: char = '\u{e1f5}';
pub(super) const ICON_SAVE: char = '\u{e14d}';
pub(super) const ICON_SOURCE: char = '\u{e11d}';
pub(super) const ICON_SCENES: char = '\u{e52a}';
/// The Studio button: a window with a side panel, shown opening or closing.
pub(super) const ICON_STUDIO_OPEN: char = '\u{e434}';
pub(super) const ICON_STUDIO_CLOSE: char = '\u{e432}';
pub(super) const ICON_MIC: char = '\u{e118}';

pub(super) const LABEL: TextStyle = TextStyle::new(13.0, Weight::Medium);
pub(super) const BODY: TextStyle = TextStyle::new(13.0, Weight::Regular);
