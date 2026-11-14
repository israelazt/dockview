//! Drawing the Studio panel: its background, its header, the sections and the
//! scroll bar.

use dockview_domain::i18n::{tr, Key};
use dockview_domain::stats::Stats;
use dockview_domain::theme::Theme;
use dockview_domain::view::Model;

use super::sections::{SectionRow, StudioSection, HEADER_H};
use super::{
    intersect, PanelHit, StudioPanel, BUTTON, ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT, ICON_CLOSE,
    ICON_SIZE, PAD, SECTION, TITLE, TOP_H,
};
use crate::motion::lerp_color;
use crate::{Rect, Ui};

impl StudioPanel {
    // -- Drawing -----------------------------------------------------------

    /// The background and the line along its left edge.
    pub(super) fn draw_frame(&self, ui: &mut Ui, t: &Theme, panel: Rect) {
        ui.rect(panel, 0.0, t.surface_solid);
        ui.rect(Rect::new(panel.x, panel.y, 1.0, panel.h), 0.0, t.border);
    }

    /// The title and the close button, over a line.
    pub(super) fn draw_top(&mut self, ui: &mut Ui, m: &Model, panel: Rect) {
        let t = &m.theme;
        let line_h = ui.measure("Ag", TITLE).1;
        ui.text(
            tr(m.lang, Key::StudioTitle),
            panel.x + PAD,
            panel.y + (TOP_H - line_h) / 2.0,
            TITLE,
            t.text,
        );
        let button = Rect::new(
            panel.x + panel.w - PAD - BUTTON,
            panel.y + (TOP_H - BUTTON) / 2.0,
            BUTTON,
            BUTTON,
        );
        self.register(PanelHit::Close, button);
        let hover = self.hover_amount(PanelHit::Close);
        ui.rect(button, BUTTON / 2.0, t.hover.with_alpha(t.hover.a * hover));
        let (cx, cy) = button.center();
        ui.icon(
            ICON_CLOSE,
            cx,
            cy,
            ICON_SIZE,
            lerp_color(t.text_dim, t.text, hover),
        );
        ui.rect(
            Rect::new(panel.x, panel.y + TOP_H - 1.0, panel.w, 1.0),
            0.0,
            t.border,
        );
    }

    /// The sections, cut off at the list's edges and moved by the scroll.
    pub(super) fn draw_sections(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        stats: &Stats,
        rows: &[SectionRow],
        list: Rect,
    ) {
        let t = &m.theme;
        let fade = self.openness();
        for row in rows {
            let top = list.y + row.top - self.scroll;
            if top + HEADER_H + row.body < list.y || top > list.y + list.h {
                continue; // out of sight
            }
            ui.set_clip(Some(list));
            let header = Rect::new(list.x, top, list.w, HEADER_H);
            // Only the part of the header on show can be pointed at.
            if let Some(shown) = intersect(header, list) {
                self.register(PanelHit::Header(row.section), shown);
            }
            let hover = self.hover_amount(PanelHit::Header(row.section));
            let wash = Rect::new(
                header.x + 8.0,
                header.y + 3.0,
                header.w - 16.0,
                HEADER_H - 6.0,
            );
            ui.rect(wash, 8.0, t.hover.with_alpha(t.hover.a * hover));
            let open = self.is_open(row.section);
            let chevron = if open {
                ICON_CHEVRON_DOWN
            } else {
                ICON_CHEVRON_RIGHT
            };
            let cy = header.y + HEADER_H / 2.0;
            ui.icon(chevron, header.x + PAD + 6.0, cy, 15.0, t.text_dim);
            let line_h = ui.measure("Ag", SECTION).1;
            ui.text(
                tr(m.lang, row.section.title()),
                header.x + PAD + 26.0,
                cy - line_h / 2.0,
                SECTION,
                t.text,
            );
            if row.body > 0.0 {
                // The body is laid out at its full height and cut to the part
                // that is open, so it unfolds like a curtain.
                let body = Rect::new(list.x, header.y + HEADER_H, list.w, row.full);
                let open_part = Rect::new(body.x, body.y, body.w, row.body);
                if let Some(shown) = intersect(open_part, list) {
                    ui.set_clip(Some(shown));
                    self.clip = shown;
                    let amount = if row.full > 0.0 {
                        row.body / row.full
                    } else {
                        1.0
                    };
                    ui.set_opacity(fade * amount);
                    self.draw_body(ui, m, stats, row.section, body);
                    ui.set_opacity(fade);
                }
                ui.set_clip(Some(list));
            }
            let end = header.y + HEADER_H + row.body;
            ui.rect(Rect::new(list.x, end - 1.0, list.w, 1.0), 0.0, t.border);
        }
    }

    pub(super) fn is_open(&self, section: StudioSection) -> bool {
        StudioSection::ALL
            .iter()
            .position(|s| *s == section)
            .is_some_and(|i| self.open[i])
    }

    /// A thin bar at the right edge while the list is longer than the room.
    pub(super) fn draw_scroll_bar(&self, ui: &mut Ui, t: &Theme, list: Rect, content_h: f32) {
        ui.set_clip(None);
        let Some((offset, length)) = super::scroll::thumb(content_h, list.h, self.scroll) else {
            return;
        };
        let bar = Rect::new(list.x + list.w - 6.0, list.y + offset, 3.0, length);
        ui.rect(bar, 1.5, t.text_dim.with_alpha(0.45));
    }
}
