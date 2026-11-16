//! A transient message at the top of the window ("Clip saved").

use super::text::Weight;
use super::{Rect, TextStyle, Ui};
use dockview_domain::theme::Theme;

/// How long a message stays, in seconds.
pub const LIFETIME_SECS: f32 = 4.0;
/// Fade in and fade out time, in seconds.
const FADE_SECS: f32 = 0.25;
/// Distance from the top of the window, in logical pixels.
const TOP_MARGIN: f32 = 22.0;
const HEIGHT: f32 = 44.0;
const PAD_X: f32 = 20.0;
/// Vertical travel while appearing.
const SLIDE: f32 = 8.0;
const STYLE: TextStyle = TextStyle::new(13.0, Weight::Medium);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
struct Toast {
    text: String,
    kind: ToastKind,
    /// Seconds since it appeared.
    age: f32,
}

/// Holds the current message, if any. A new message replaces the old one.
#[derive(Debug, Default)]
pub struct ToastHost {
    current: Option<Toast>,
}

/// Opacity of a message of the given age: fades in, holds, fades out.
fn opacity_at(age: f32, animations: bool) -> f32 {
    if !animations {
        return if age < LIFETIME_SECS { 1.0 } else { 0.0 };
    }
    let fade_in = (age / FADE_SECS).clamp(0.0, 1.0);
    let fade_out = ((LIFETIME_SECS - age) / FADE_SECS).clamp(0.0, 1.0);
    fade_in.min(fade_out)
}

impl ToastHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.current = Some(Toast {
            text: text.into(),
            kind,
            age: 0.0,
        });
    }

    pub fn is_visible(&self) -> bool {
        self.current.is_some()
    }

    /// Advances time and draws the message while it lives.
    pub fn frame(&mut self, ui: &mut Ui, theme: &Theme, dt: f32, animations: bool) {
        let Some(toast) = self.current.as_mut() else {
            return;
        };
        toast.age += dt;
        if toast.age >= LIFETIME_SECS {
            self.current = None;
            return;
        }
        let opacity = opacity_at(toast.age, animations);
        let rise = if animations {
            (1.0 - (toast.age / FADE_SECS).clamp(0.0, 1.0)) * -SLIDE
        } else {
            0.0
        };

        let dot = match toast.kind {
            ToastKind::Info => theme.accent,
            ToastKind::Success => theme.live,
            ToastKind::Error => theme.error,
        };
        let text_w = ui.measure(&toast.text, STYLE).0;
        let width = PAD_X * 2.0 + 16.0 + text_w;
        let (screen_w, _) = ui.logical_size();
        let rect = Rect::new(
            ((screen_w - width) / 2.0).round(),
            TOP_MARGIN,
            width.round(),
            HEIGHT,
        );

        ui.set_opacity(opacity);
        ui.set_offset(0.0, rise);
        ui.shadow(rect, HEIGHT / 2.0, 20.0, theme.shadow);
        ui.panel(rect, HEIGHT / 2.0, theme.surface, theme.border, 1.0);
        let cy = rect.y + HEIGHT / 2.0;
        ui.rect(Rect::new(rect.x + PAD_X, cy - 4.0, 8.0, 8.0), 4.0, dot);
        let line_h = ui.measure("Ag", STYLE).1;
        ui.text(
            &toast.text,
            rect.x + PAD_X + 16.0,
            cy - line_h / 2.0,
            STYLE,
            theme.text,
        );
        ui.set_offset(0.0, 0.0);
        ui.set_opacity(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui() -> Ui {
        let mut ui = Ui::new();
        ui.begin((1280, 720), 1.0);
        ui
    }

    #[test]
    fn nothing_is_drawn_without_a_message() {
        let mut host = ToastHost::new();
        let mut ui = ui();
        host.frame(&mut ui, &Theme::dark(), 0.016, true);
        assert!(ui.vertices().is_empty());
        assert!(!host.is_visible());
    }

    #[test]
    fn a_message_is_drawn_then_expires_after_its_lifetime() {
        let mut host = ToastHost::new();
        host.show("Clip saved", ToastKind::Success);
        let mut ui = ui();
        host.frame(&mut ui, &Theme::dark(), 0.5, true);
        assert!(!ui.vertices().is_empty() && host.is_visible());

        for _ in 0..40 {
            host.frame(&mut ui, &Theme::dark(), 0.1, true);
        }
        assert!(!host.is_visible(), "gone after {LIFETIME_SECS} s");
    }

    #[test]
    fn a_new_message_replaces_the_old_one_and_restarts_the_clock() {
        let mut host = ToastHost::new();
        host.show("first", ToastKind::Info);
        let mut ui = ui();
        host.frame(&mut ui, &Theme::dark(), 3.5, true);
        host.show("second", ToastKind::Error);
        host.frame(&mut ui, &Theme::dark(), 1.0, true);
        assert!(host.is_visible(), "the clock restarted");
        assert_eq!(host.current.as_ref().unwrap().text, "second");
    }

    #[test]
    fn opacity_ramps_in_holds_and_ramps_out() {
        assert_eq!(opacity_at(0.0, true), 0.0);
        assert!(opacity_at(0.1, true) > 0.0 && opacity_at(0.1, true) < 1.0);
        assert_eq!(opacity_at(2.0, true), 1.0);
        assert!(opacity_at(LIFETIME_SECS - 0.1, true) < 1.0);
        assert_eq!(opacity_at(LIFETIME_SECS, true), 0.0);
    }

    #[test]
    fn without_animations_it_is_fully_opaque_until_it_ends() {
        assert_eq!(opacity_at(0.0, false), 1.0);
        assert_eq!(opacity_at(3.9, false), 1.0);
        assert_eq!(opacity_at(4.0, false), 0.0);
    }

    #[test]
    fn the_message_is_centred_and_inside_the_window() {
        let mut host = ToastHost::new();
        host.show("Clip saved · 30 s", ToastKind::Success);
        let mut ui = ui();
        host.frame(&mut ui, &Theme::light(), 1.0, false);
        let xs: Vec<f32> = ui.vertices().iter().map(|v| v.pos[0]).collect();
        let (min, max) = (
            xs.iter().cloned().fold(f32::MAX, f32::min),
            xs.iter().cloned().fold(f32::MIN, f32::max),
        );
        assert!(min >= 0.0 && max <= 1280.0);
        assert!(((min + max) / 2.0 - 640.0).abs() < 4.0);
    }
}
