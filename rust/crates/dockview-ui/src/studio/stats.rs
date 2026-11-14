//! The Statistics section: what the app and the capture device are doing, in
//! rows, and the plain-language diagnosis under them. It shows the same rows
//! and sentences as the floating statistics panel (it uses the same code to
//! build them).

use dockview_domain::i18n::{tr, Key};
use dockview_domain::stats::Stats;
use dockview_domain::view::Model;

use super::widgets::{GAP, LABEL_H};
use super::{StudioPanel, NOTE, PAD};
use crate::fit::fit_text;
use crate::stats::{finding_text, sections, tone_color, wrap_text};
use crate::text::Weight;
use crate::{Rect, TextStyle, Ui};

const PAD_TOP: f32 = 4.0;
const PAD_BOTTOM: f32 = 14.0;
/// Height of a row of label and value.
const ROW: f32 = 22.0;
/// Height of a line of the diagnosis, and the space between two findings.
const FINDING_LINE: f32 = 18.0;
const FINDING_GAP: f32 = 6.0;
/// Width the dot of a finding takes, with its space.
const DOT: f32 = 18.0;

const VALUE: TextStyle = TextStyle::new(13.0, Weight::Regular).tabular();

/// The sentences of the diagnosis wrapped to the room there is, one list of
/// lines per finding.
fn diagnosis_lines(
    ui: &mut Ui,
    stats: &Stats,
    lang: dockview_domain::i18n::Lang,
    width: f32,
) -> Vec<Vec<String>> {
    stats
        .findings
        .iter()
        .map(|f| wrap_text(ui, finding_text(*f, lang), NOTE, width - DOT))
        .collect()
}

/// How tall the body is for a panel `width` wide: the rows, then the
/// diagnosis as long as its sentences are once wrapped.
pub fn body_height(ui: &mut Ui, m: &Model, stats: &Stats, width: f32) -> f32 {
    let mut height = PAD_TOP;
    for section in sections(stats, m.lang) {
        height += LABEL_H + section.rows.len() as f32 * ROW + GAP;
    }
    height += LABEL_H;
    for lines in diagnosis_lines(ui, stats, m.lang, width - PAD * 2.0) {
        height += lines.len() as f32 * FINDING_LINE + FINDING_GAP;
    }
    height + PAD_BOTTOM - FINDING_GAP
}

impl StudioPanel {
    /// Draws the body of the Statistics section with its top-left corner at
    /// the corner of `body`.
    pub(super) fn draw_stats(&mut self, ui: &mut Ui, m: &Model, stats: &Stats, body: Rect) {
        let t = &m.theme;
        let (x, w) = (body.x + PAD, body.w - PAD * 2.0);
        let mut y = body.y + PAD_TOP;
        let line_h = ui.measure("Ag", VALUE).1;

        for section in sections(stats, m.lang) {
            y += self.caption(ui, t, section.title, x, y);
            for (label, value) in &section.rows {
                let value_w = ui.measure(value, VALUE).0;
                // A long value (the device mode) shortens its label, not the
                // other way round, so the two never overlap.
                let room = (w - 10.0 - value_w - 12.0).max(0.0);
                let label = fit_text(ui, label, VALUE, room);
                let text_y = y + (ROW - line_h) / 2.0;
                ui.text(&label, x + 10.0, text_y, VALUE, t.text_dim);
                ui.text(value, x + w - value_w, text_y, VALUE, t.text);
                y += ROW;
            }
            y += GAP;
        }

        y += self.caption(ui, t, tr(m.lang, Key::Diagnosis), x, y);
        for (finding, lines) in stats
            .findings
            .iter()
            .zip(diagnosis_lines(ui, stats, m.lang, w))
        {
            ui.rect(
                Rect::new(x + 10.0, y + 5.0, 8.0, 8.0),
                4.0,
                tone_color(*finding, t),
            );
            for (i, line) in lines.iter().enumerate() {
                ui.text(
                    line,
                    x + 10.0 + DOT,
                    y + i as f32 * FINDING_LINE,
                    NOTE,
                    t.text,
                );
            }
            y += lines.len() as f32 * FINDING_LINE + FINDING_GAP;
        }
    }
}
