//! What the screenshot overlay shows on top of the dimmed screen: the dimming
//! with the chosen part left clear, its border and size, the free-form path
//! and the bar of mode buttons. It is drawn with the same interface engine and
//! the same look as the rest of Dockview (rounded panels, Lucide icons, the
//! sliding indicator), and moves the same way: the bar slides in, the buttons
//! light up as the pointer reaches them, and the highlighted window glides to
//! the next one.
//!
//! The logic (what a click means, where the buttons are) is in
//! `dockview_domain::overlay_input`; this only draws it.

use dockview_domain::desktop::PxRect;
use dockview_domain::i18n::{tr, Lang};
use dockview_domain::select::{Mode, Toolbar, View};
use dockview_domain::theme::{Color, Theme};

use crate::motion::{ease_out_cubic, lerp_color, move_toward, smooth};
use crate::text::Weight;
use crate::{Rect, TextStyle, Ui};

/// How long the dimming and the bar take to come in, in seconds.
const APPEAR_SECS: f32 = 0.22;
/// How much the screen is darkened (0 to 1).
const DIM_ALPHA: f32 = 0.55;
const HOVER_RATE: f32 = 22.0;
const PRESS_RATE: f32 = 34.0;
const GLIDE_RATE: f32 = 18.0;
const WINDOW_RATE: f32 = 26.0;

const LABEL: TextStyle = TextStyle::new(13.0, Weight::Medium);
const CHIP: TextStyle = TextStyle::new(12.0, Weight::Medium);

// Lucide code points.
const ICON_RECT: char = '\u{e0ab}';
const ICON_WINDOW: char = '\u{e426}';
const ICON_FULL: char = '\u{e534}';
const ICON_FREE: char = '\u{e1ce}';
const ICON_CLOSE: char = '\u{e1b2}';
/// The size of the icons of the pill of the app.
const ICON_SIZE: f32 = 17.0;

fn icon_of(mode: Mode) -> char {
    match mode {
        Mode::Rectangle => ICON_RECT,
        Mode::Window => ICON_WINDOW,
        Mode::Full => ICON_FULL,
        Mode::Freeform => ICON_FREE,
    }
}

fn index_of(mode: Mode) -> usize {
    Mode::ALL.iter().position(|m| *m == mode).unwrap_or(0)
}

/// What is shown this frame.
pub struct SnipView<'a> {
    pub view: &'a View,
    pub mode: Mode,
    /// The corner of the overlay window on the virtual screen.
    pub origin: (i32, i32),
    pub toolbar: &'a Toolbar,
    pub hovered: Option<Mode>,
    pub pressed: Option<Mode>,
    pub close_hovered: bool,
    pub close_pressed: bool,
    pub lang: Lang,
}

/// The state of the animations.
pub struct SnipChrome {
    /// How far the overlay has come in, 0 to 1, linear in time.
    appear: f32,
    /// Where the sliding indicator is, as a button position (fractions glide).
    indicator: Option<f32>,
    /// The four mode buttons, then the close button.
    hover: [f32; 5],
    press: [f32; 5],
    /// The window highlight (logical x, y, w, h) and how visible it is.
    window: Option<[f32; 4]>,
    window_alpha: f32,
    theme: Theme,
}

impl Default for SnipChrome {
    fn default() -> Self {
        Self::new()
    }
}

impl SnipChrome {
    pub fn new() -> Self {
        Self {
            appear: 0.0,
            indicator: None,
            hover: [0.0; 5],
            press: [0.0; 5],
            window: None,
            window_alpha: 0.0,
            theme: Theme::dark(),
        }
    }

    /// Whether anything is still moving (the window can then wait for events).
    pub fn is_animating(&self) -> bool {
        self.appear < 1.0
            || self
                .hover
                .iter()
                .chain(&self.press)
                .any(|v| *v > 0.002 && *v < 0.998)
            || self.window_alpha > 0.002 && self.window_alpha < 0.998
    }

    fn step(value: f32, target: f32, dt: f32, rate: f32, animations: bool) -> f32 {
        if animations {
            smooth(value, target, dt, rate)
        } else {
            target
        }
    }

    /// Draws one frame. `ui` was begun at the overlay's size and the scale of
    /// the monitor the bar is on. With `animations` off everything is instant.
    pub fn frame(&mut self, ui: &mut Ui, v: &SnipView, dt: f32, animations: bool) {
        self.appear = if animations {
            move_toward(self.appear, 1.0, dt / APPEAR_SECS)
        } else {
            1.0
        };
        let eased = ease_out_cubic(self.appear);
        let scale = ui.scale().max(0.1);
        let logical = |r: PxRect| {
            Rect::new(
                (r.x - v.origin.0) as f32 / scale,
                (r.y - v.origin.1) as f32 / scale,
                r.w as f32 / scale,
                r.h as f32 / scale,
            )
        };

        // The part left clear: the rectangle being dragged, or the window
        // under the pointer (which glides from one window to the next).
        let target = v.view.hover.map(logical);
        if let Some(t) = target {
            let to = [t.x, t.y, t.w, t.h];
            self.window = Some(match self.window {
                Some(from) => {
                    std::array::from_fn(|i| Self::step(from[i], to[i], dt, WINDOW_RATE, animations))
                }
                None => to,
            });
        }
        self.window_alpha = Self::step(
            self.window_alpha,
            f32::from(target.is_some()),
            dt,
            HOVER_RATE,
            animations,
        );
        let clear = match (v.view.rect, self.window) {
            (Some(r), _) => Some(logical(r)),
            (None, Some(w)) if target.is_some() => Some(Rect::new(w[0], w[1], w[2], w[3])),
            _ => None,
        };

        self.draw_dim(ui, clear, DIM_ALPHA * eased);
        if let Some(r) = clear {
            self.draw_selection(ui, r, v, scale);
        }
        self.draw_path(ui, v, scale);
        self.draw_toolbar(ui, v, dt, eased, animations, scale);
    }

    /// The darkening: everything but `clear`, in the four strips around it.
    fn draw_dim(&self, ui: &mut Ui, clear: Option<Rect>, alpha: f32) {
        let color = Color::hex(0x000000, alpha);
        let (w, h) = ui.logical_size();
        let Some(c) = clear else {
            ui.rect(Rect::new(0.0, 0.0, w, h), 0.0, color);
            return;
        };
        let (left, right) = (c.x.clamp(0.0, w), (c.x + c.w).clamp(0.0, w));
        let (top, bottom) = (c.y.clamp(0.0, h), (c.y + c.h).clamp(0.0, h));
        ui.rect(Rect::new(0.0, 0.0, w, top), 0.0, color);
        ui.rect(Rect::new(0.0, bottom, w, h - bottom), 0.0, color);
        ui.rect(Rect::new(0.0, top, left, bottom - top), 0.0, color);
        ui.rect(Rect::new(right, top, w - right, bottom - top), 0.0, color);
    }

    /// The border of the clear part and the chip with its size.
    fn draw_selection(&self, ui: &mut Ui, r: Rect, v: &SnipView, scale: f32) {
        let t = &self.theme;
        ui.outline(r, 2.0, t.accent);
        // The size is what the picture will be, in real pixels.
        let text = format!("{} × {}", (r.w * scale).round(), (r.h * scale).round());
        let (tw, th) = ui.measure(&text, CHIP);
        let chip_w = tw + 18.0;
        let chip_h = th + 10.0;
        let y = if r.y > chip_h + 12.0 {
            r.y - chip_h - 8.0
        } else {
            r.y + r.h + 8.0
        };
        let chip = Rect::new(r.x, y, chip_w, chip_h);
        ui.shadow(chip, 8.0, 10.0, t.shadow);
        ui.panel(chip, 8.0, t.surface, t.border, 1.0);
        ui.text(&text, chip.x + 9.0, chip.y + 5.0, CHIP, t.text);
        let _ = v;
    }

    /// The free-form path, as a line of dots.
    fn draw_path(&self, ui: &mut Ui, v: &SnipView, scale: f32) {
        let color = self.theme.accent;
        for (x, y) in &v.view.path {
            let (lx, ly) = (
                (x - v.origin.0) as f32 / scale,
                (y - v.origin.1) as f32 / scale,
            );
            ui.rect(Rect::new(lx - 1.5, ly - 1.5, 3.0, 3.0), 1.5, color);
        }
    }

    /// The bar of mode buttons, made like the pill of the app: a capsule with
    /// the same shadow and border, 32 px buttons with a rounded wash on hover,
    /// icons 17 px that dip under the finger, a thin divider and a close button.
    fn draw_toolbar(
        &mut self,
        ui: &mut Ui,
        v: &SnipView,
        dt: f32,
        eased: f32,
        animations: bool,
        scale: f32,
    ) {
        let t = self.theme;
        let logical = |r: PxRect| {
            Rect::new(
                (r.x - v.origin.0) as f32 / scale,
                (r.y - v.origin.1) as f32 / scale,
                r.w as f32 / scale,
                r.h as f32 / scale,
            )
        };
        // The bar slides down a little as it fades in, like the Studio panel.
        ui.set_opacity(eased);
        ui.set_offset(0.0, (1.0 - eased) * -14.0);
        let bar = logical(v.toolbar.back);
        ui.shadow(bar, bar.h / 2.0, 22.0, t.shadow);
        ui.panel(bar, bar.h / 2.0, t.surface, t.border, 1.0);

        // The indicator glides to the current mode's button.
        let current = index_of(v.mode) as f32;
        let at = Self::step(
            self.indicator.unwrap_or(current),
            current,
            dt,
            GLIDE_RATE,
            animations,
        );
        self.indicator = Some(at);
        if let (Some(first), Some(last)) = (
            v.toolbar.buttons.get(at.floor() as usize),
            v.toolbar.buttons.get(at.ceil() as usize),
        ) {
            let (a, b) = (logical(first.1), logical(last.1));
            let k = at - at.floor();
            let lit = Rect::new(a.x + (b.x - a.x) * k, a.y, a.w, a.h);
            ui.rect(lit, lit.h / 2.0, t.segment_active);
        }

        for (i, (mode, rect)) in v.toolbar.buttons.iter().enumerate() {
            let r = logical(*rect);
            let (hover, press) = (v.hovered == Some(*mode), v.pressed == Some(*mode));
            let (h, p) = self.animate(i, hover, press, dt, animations);
            let wash = t.hover.a * (h * 0.6 + 0.9 * p);
            if wash > 0.002 && *mode != v.mode {
                ui.rect(r, r.h / 2.0, t.hover.with_alpha(wash.min(1.0)));
            }
            let chosen = (1.0 - (index_of(*mode) as f32 - at).abs()).clamp(0.0, 1.0);
            let color = lerp_color(t.text_dim, t.text, chosen.max(h));
            let words = tr(v.lang, mode.label());
            let (text_w, text_h) = ui.measure(words, LABEL);
            let icon = ICON_SIZE * (1.0 - 0.1 * p);
            let total = icon + 8.0 + text_w;
            let x = r.x + (r.w - total) / 2.0;
            let cy = r.y + r.h / 2.0;
            ui.icon(icon_of(*mode), x + icon / 2.0, cy, icon, color);
            ui.text(words, x + icon + 8.0, cy - text_h / 2.0, LABEL, color);
        }

        // The thin divider, then the close button (an icon in a round wash).
        let close = logical(v.toolbar.close);
        let cy = close.y + close.h / 2.0;
        let divider_x = (v.toolbar.divider_x - v.origin.0) as f32 / scale;
        ui.rect(Rect::new(divider_x, cy - 10.0, 1.0, 20.0), 0.5, t.track);
        let (h, p) = self.animate(4, v.close_hovered, v.close_pressed, dt, animations);
        let wash = t.hover.a * (h + 0.9 * p);
        if wash > 0.002 {
            ui.rect(close, close.h / 2.0, t.hover.with_alpha(wash.min(1.0)));
        }
        let (ccx, _) = close.center();
        ui.icon(ICON_CLOSE, ccx, cy, ICON_SIZE * (1.0 - 0.1 * p), t.text);
        ui.set_offset(0.0, 0.0);
        ui.set_opacity(1.0);
    }

    /// Moves the hover and press amounts of button `i`; returns them.
    fn animate(
        &mut self,
        i: usize,
        hover: bool,
        press: bool,
        dt: f32,
        animations: bool,
    ) -> (f32, f32) {
        self.hover[i] = Self::step(self.hover[i], f32::from(hover), dt, HOVER_RATE, animations);
        self.press[i] = Self::step(self.press[i], f32::from(press), dt, PRESS_RATE, animations);
        (self.hover[i], self.press[i])
    }
}

#[cfg(test)]
mod tests;
