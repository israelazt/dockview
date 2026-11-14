//! The small widgets the sections of the Studio panel are made of. Each one
//! registers what can be clicked with the [`Action`] it asks for, so the panel
//! does what the popovers do and no logic is written twice.

use dockview_domain::i18n::{tr, Key, Lang};
use dockview_domain::theme::Theme;
use dockview_domain::view::Action;

use super::{intersect, PanelHit, StudioPanel};
use crate::fit::fit_text;
use crate::motion::smooth;
use crate::text::Weight;
use crate::{Rect, TextStyle, Ui};

/// Height of the small title above a group of controls.
pub const LABEL_H: f32 = 20.0;
/// Height of a row of controls.
pub const ROW_H: f32 = 32.0;
/// Space between one group of controls and the next.
pub const GAP: f32 = 14.0;
/// Smoothing rate of the sliding indicator of a segmented control.
const SLIDE_RATE: f32 = 20.0;

const CAPTION: TextStyle = TextStyle::new(12.0, Weight::Medium);
const CHOICE: TextStyle = TextStyle::new(12.5, Weight::Medium);
const ROW_TEXT: TextStyle = TextStyle::new(13.0, Weight::Regular);
const ICON_CHECK: char = '\u{e06c}';

impl StudioPanel {
    /// Registers `rect` as something that runs `action` when clicked. Only the
    /// part of it on show (inside the clip of the section being drawn) can be
    /// clicked. Returns what to ask the hover amount of.
    pub(super) fn item(&mut self, rect: Rect, action: Action) -> PanelHit {
        let hit = PanelHit::Item(self.next_items.len());
        self.next_items.push(action);
        if let Some(shown) = intersect(rect, self.clip) {
            self.register(hit, shown);
        }
        hit
    }

    /// A small dim title above a group of controls; returns its height.
    pub(super) fn caption(&self, ui: &mut Ui, t: &Theme, text: &str, x: f32, y: f32) -> f32 {
        ui.text(text, x, y, CAPTION, t.text_dim);
        LABEL_H
    }

    /// A row of choices of which one is selected (for example light, dark,
    /// auto). `key` names the control so its indicator can glide from one
    /// choice to the next; `actions[i]` is what choosing `labels[i]` asks for.
    /// Returns its height.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn segmented(
        &mut self,
        ui: &mut Ui,
        t: &Theme,
        key: &'static str,
        labels: &[&str],
        selected: usize,
        actions: Vec<Action>,
        x: f32,
        y: f32,
        w: f32,
    ) -> f32 {
        let track = Rect::new(x, y, w, ROW_H);
        ui.rect(track, 11.0, t.segment_track);
        let segment_w = (w - 6.0) / labels.len().max(1) as f32;
        let line_h = ui.measure("Ag", CHOICE).1;

        // The indicator starts at the current choice and glides to a new one.
        let target = selected as f32;
        let current = self.slide.get(key).copied().unwrap_or(target);
        let position = if self.animations {
            smooth(current, target, self.dt, SLIDE_RATE)
        } else {
            target
        };
        self.slide.insert(key, position);
        let indicator = Rect::new(
            x + 3.0 + segment_w * position,
            y + 3.0,
            segment_w,
            ROW_H - 6.0,
        );
        ui.rect(indicator, 8.0, t.segment_active);

        for ((i, label), action) in labels.iter().enumerate().zip(actions) {
            let segment = Rect::new(
                x + 3.0 + segment_w * i as f32,
                y + 3.0,
                segment_w,
                ROW_H - 6.0,
            );
            let hit = self.item(segment, action);
            let hover = self.hover_amount(hit);
            if i != selected && hover > 0.002 {
                ui.rect(
                    segment,
                    8.0,
                    t.hover.with_alpha((t.hover.a * hover).min(1.0)),
                );
            }
            let color = if i == selected { t.text } else { t.text_dim };
            let text = fit_text(ui, label, CHOICE, segment_w - 8.0);
            let text_w = ui.measure(&text, CHOICE).0;
            ui.text(
                &text,
                segment.x + (segment_w - text_w) / 2.0,
                y + (ROW_H - line_h) / 2.0,
                CHOICE,
                color,
            );
        }
        ROW_H
    }

    /// A list of names of which one can be chosen (the cards, the screens...),
    /// showing at most `limit` of them; `choose(i)` is what picking the `i`-th
    /// asks for. The chosen one is marked with a check. An empty list says so.
    /// Returns its height (see [`list_height`]).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn list(
        &mut self,
        ui: &mut Ui,
        t: &Theme,
        lang: Lang,
        names: &[String],
        selected: Option<usize>,
        limit: usize,
        choose: &dyn Fn(usize) -> Action,
        x: f32,
        y: f32,
        w: f32,
    ) -> f32 {
        let line_h = ui.measure("Ag", ROW_TEXT).1;
        if names.is_empty() {
            ui.text(
                tr(lang, Key::NoDevice),
                x + 10.0,
                y + (ROW_H - line_h) / 2.0,
                ROW_TEXT,
                t.text_dim,
            );
            return ROW_H;
        }
        let mut top = y;
        for (i, name) in names.iter().take(limit).enumerate() {
            let row = Rect::new(x, top, w, ROW_H);
            let hit = self.item(row, choose(i));
            let hover = self.hover_amount(hit);
            ui.rect(row, 9.0, t.hover.with_alpha(t.hover.a * hover));
            let label = fit_text(ui, name, ROW_TEXT, w - 20.0 - 28.0);
            let chosen = selected == Some(i);
            ui.text(
                &label,
                x + 10.0,
                top + (ROW_H - line_h) / 2.0,
                ROW_TEXT,
                if chosen { t.text } else { t.text_dim },
            );
            if chosen {
                ui.icon(ICON_CHECK, x + w - 18.0, top + ROW_H / 2.0, 15.0, t.accent);
            }
            top += ROW_H;
        }
        top - y
    }
}

/// How tall a [`StudioPanel::list`] of `count` names is when it shows at most
/// `limit`: an empty one still takes a row, for its note.
pub fn list_height(count: usize, limit: usize) -> f32 {
    count.clamp(1, limit) as f32 * ROW_H
}
