//! Sizes, timings, icons and text styles of the controls.

use super::super::text::Weight;
use super::super::TextStyle;

/// Seconds of pointer inactivity before the controls fade out.
pub const IDLE_HIDE_SECS: f32 = 3.0;
/// Fade duration, in seconds.
pub const FADE_SECS: f32 = 0.2;
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

pub(super) const PILL_HEIGHT: f32 = 52.0;
pub(super) const BOTTOM_MARGIN: f32 = 24.0;
pub(super) const PAD_X: f32 = 10.0;
/// Space between two buttons of the bar.
pub(super) const GAP: f32 = 8.0;
/// Space on each side of a divider.
pub(super) const DIVIDER_PAD: f32 = 12.0;
/// Height of the wide buttons (save, record, screenshot) and size of the
/// round ones (microphone, readouts, full screen).
pub(super) const ITEM_H: f32 = 36.0;
pub(super) const ICON_SIZE: f32 = 17.0;
pub(super) const SLIDER_WIDTH: f32 = 84.0;
/// Size of the round buttons of the clip length selector, and its width.
pub(super) const STEP_BUTTON: f32 = 32.0;
pub(super) const LENGTH_POPOVER_W: f32 = 172.0;

// Lucide code points.
pub(super) const ICON_VOLUME_MUTE: char = '\u{e1ac}';
pub(super) const ICON_VOLUME_LOW: char = '\u{e1aa}';
pub(super) const ICON_VOLUME_HIGH: char = '\u{e1ab}';
pub(super) const ICON_FULLSCREEN: char = '\u{e112}';
pub(super) const ICON_EXIT_FULLSCREEN: char = '\u{e11a}';
pub(super) const ICON_SAVE: char = '\u{e14d}';
pub(super) const ICON_CAMERA: char = '\u{e064}';
pub(super) const ICON_EYE: char = '\u{e0ba}';
pub(super) const ICON_EYE_OFF: char = '\u{e0bb}';
pub(super) const ICON_MIC: char = '\u{e118}';
pub(super) const ICON_MIC_OFF: char = '\u{e119}';
pub(super) const ICON_MINUS: char = '\u{e11c}';
pub(super) const ICON_PLUS: char = '\u{e13d}';

pub(super) const BODY: TextStyle = TextStyle::new(13.0, Weight::Regular);
/// The text of the wide buttons.
pub(super) const BUTTON_TEXT: TextStyle = TextStyle::new(13.0, Weight::SemiBold);
/// The hotkey chip inside a wide button.
pub(super) const CHIP_TEXT: TextStyle = TextStyle::new(10.5, Weight::Medium);
