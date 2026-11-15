//! The Scenes section: pick a scene, make or delete one, and edit the layers
//! of the one in use (show or hide, remove, add a source). The same choices
//! and actions as the scenes popover; with the room the panel has, adding a
//! source needs no view of its own: the sources are listed right there.

use dockview_domain::i18n::{tr, Key, Lang};
use dockview_domain::theme::{Color, Theme};
use dockview_domain::view::{Action, Model, SceneAction};

use super::scene_list::{self, SCENE_ROW_H};
use super::widgets::{list_height, GAP, LABEL_H, ROW_H};
use super::{intersect, PanelHit, StudioPanel, PAD};
use crate::fit::fit_text;
use crate::text::Weight;
use crate::{Rect, TextStyle, Ui};

/// Scenes listed before the rest are cut off (the plain source is one more).
pub(super) const MAX_SCENES: usize = 8;
/// Most cards, screens or windows listed to add.
const MAX_ADD: usize = 5;
const PAD_TOP: f32 = 4.0;
const PAD_BOTTOM: f32 = 14.0;

pub(super) const ROW_TEXT: TextStyle = TextStyle::new(13.0, Weight::Regular);

pub(super) const ICON_PLUS: char = '\u{e13d}';
const ICON_TRASH: char = '\u{e18d}';
const ICON_PENCIL: char = '\u{e1f9}';
const ICON_MOVE: char = '\u{e121}';

/// How tall the body is: the list, the buttons and, with a scene in use, its
/// layers and the sources that can be added.
#[cfg(test)]
pub fn body_height(m: &Model) -> f32 {
    body_height_in(m, false)
}

/// The same; in a card of its own (`cards`) the button that makes a scene is
/// in the header and takes no row.
pub fn body_height_in(m: &Model, cards: bool) -> f32 {
    let s = &m.scenes;
    let mut height = PAD_TOP + scene_list::row_count(m) as f32 * SCENE_ROW_H;
    if !cards {
        height += GAP + ROW_H;
    }
    if s.active.is_some() {
        height += GAP;
        height += ROW_H * 3.0; // edit the layers, rename and delete this scene
        height += GAP + LABEL_H + s.layers.len() as f32 * ROW_H;
        if s.can_add {
            let group = |n: usize| LABEL_H + list_height(n, MAX_ADD);
            height += GAP
                + LABEL_H
                + group(m.video_devices.len())
                + group(m.sources.monitors.len())
                + group(m.sources.windows.len());
        }
    }
    height + PAD_BOTTOM
}

impl StudioPanel {
    /// Draws the body of the Scenes section with its top-left corner at the
    /// corner of `body`.
    pub(super) fn draw_scenes(&mut self, ui: &mut Ui, m: &Model, body: Rect) {
        let t = &m.theme;
        let lang = m.lang;
        let (x, w) = (body.x + PAD, body.w - PAD * 2.0);
        let mut y = body.y + PAD_TOP;
        let s = &m.scenes;

        y += self.scene_rows(ui, m, (x, y, w));
        if !self.scope.cards() {
            y += GAP;
            y += self.action_row(
                ui,
                t,
                ICON_PLUS,
                tr(lang, Key::SceneNew),
                Action::Scene(SceneAction::New),
                (x, y, w),
            );
        }
        let Some(active) = s.active else {
            self.confirm_delete = None;
            return;
        };
        if self.confirm_delete != Some(active) {
            self.confirm_delete = None;
        }
        if self.editing.as_ref().is_some_and(|(i, _)| *i != active) {
            self.editing = None;
        }
        y += GAP;
        y += self.edit_row(ui, t, lang, s.editing, (x, y, w));
        let name = s.names.get(active).map_or("", String::as_str);
        y += self.rename_row(ui, t, lang, (active, name), (x, y, w));
        y += self.delete_row(ui, t, lang, active, (x, y, w));

        y += GAP;
        y += self.caption(ui, t, tr(lang, Key::SceneLayers), x, y);
        for i in 0..s.layers.len() {
            self.layer_row(ui, m, i, (x, y, w));
            y += ROW_H;
        }
        if !s.can_add {
            return;
        }

        y += GAP;
        y += self.caption(ui, t, tr(lang, Key::SceneAddSource), x, y);
        y += self.caption(ui, t, tr(lang, Key::SourceCapture), x, y);
        let add_card = |i| Action::Scene(SceneAction::AddCapture(i));
        y += self.list(
            ui,
            t,
            lang,
            &m.video_devices,
            None,
            MAX_ADD,
            &add_card,
            x,
            y,
            w,
        );
        y += self.caption(ui, t, tr(lang, Key::SourceScreen), x, y);
        let add_screen = |i| Action::Scene(SceneAction::AddMonitor(i));
        y += self.list(
            ui,
            t,
            lang,
            &m.sources.monitors,
            None,
            MAX_ADD,
            &add_screen,
            x,
            y,
            w,
        );
        y += self.caption(ui, t, tr(lang, Key::SourceApp), x, y);
        let add_window = |i| Action::Scene(SceneAction::AddWindow(i));
        self.list(
            ui,
            t,
            lang,
            &m.sources.windows,
            None,
            MAX_ADD,
            &add_window,
            x,
            y,
            w,
        );
    }

    /// The switch of the layer editor over the picture: lit while it is on.
    fn edit_row(
        &mut self,
        ui: &mut Ui,
        t: &Theme,
        lang: Lang,
        on: bool,
        (x, y, w): (f32, f32, f32),
    ) -> f32 {
        let row = Rect::new(x, y, w, ROW_H);
        let hit = self.item(row, Action::Scene(SceneAction::ToggleEdit));
        let hover = self.hover_amount(hit);
        if on {
            ui.rect(row, 9.0, t.accent.with_alpha(0.16));
        }
        ui.rect(row, 9.0, t.hover.with_alpha(t.hover.a * hover));
        let color = if on { t.accent } else { t.text_dim };
        ui.icon(ICON_MOVE, x + 18.0, y + ROW_H / 2.0, 15.0, color);
        let line_h = ui.measure("Ag", ROW_TEXT).1;
        let text = fit_text(ui, tr(lang, Key::SceneEditLayers), ROW_TEXT, w - 48.0);
        ui.text(
            &text,
            x + 38.0,
            y + (ROW_H - line_h) / 2.0,
            ROW_TEXT,
            t.text,
        );
        ROW_H
    }

    /// The row that renames the scene in use: a button, which becomes a text
    /// field while the name is typed. Returns its height.
    fn rename_row(
        &mut self,
        ui: &mut Ui,
        t: &Theme,
        lang: Lang,
        (active, name): (usize, &str),
        (x, y, w): (f32, f32, f32),
    ) -> f32 {
        let row = Rect::new(x, y, w, ROW_H);
        let line_h = ui.measure("Ag", ROW_TEXT).1;
        let typing = self
            .editing
            .as_ref()
            .filter(|(i, _)| *i == active)
            .map(|(_, e)| (e.text().to_owned(), e.before_caret().to_owned()));
        let Some((typed, before_caret)) = typing else {
            self.rename_of = Some((active, name.to_owned()));
            if let Some(shown) = intersect(row, self.clip) {
                self.register(PanelHit::Rename, shown);
            }
            let hover = self.hover_amount(PanelHit::Rename);
            ui.rect(row, 9.0, t.hover.with_alpha(t.hover.a * hover));
            ui.icon(ICON_PENCIL, x + 18.0, y + ROW_H / 2.0, 15.0, t.text_dim);
            let text = fit_text(ui, tr(lang, Key::SceneRename), ROW_TEXT, w - 48.0);
            ui.text(
                &text,
                x + 38.0,
                y + (ROW_H - line_h) / 2.0,
                ROW_TEXT,
                t.text,
            );
            return ROW_H;
        };
        // The field: what was typed so far and the caret after it.
        let field = Rect::new(x, y + 1.0, w, ROW_H - 2.0);
        if let Some(shown) = intersect(field, self.clip) {
            self.register(PanelHit::EditField, shown);
        }
        ui.panel(field, 9.0, t.surface_solid, t.accent, 1.0);
        let text_x = x + 10.0;
        self.edit_room = w - 20.0 - 2.0;
        ui.text(&typed, text_x, y + (ROW_H - line_h) / 2.0, ROW_TEXT, t.text);
        let caret_x = text_x + ui.measure(&before_caret, ROW_TEXT).0;
        ui.rect(
            Rect::new(caret_x, y + 8.0, 1.5, ROW_H - 16.0),
            0.75,
            t.accent,
        );
        ROW_H
    }

    /// The row that deletes the scene in use. A click only asks; the row then
    /// holds the question with a button for each answer, and only "delete"
    /// asks the application to do it. Returns its height.
    fn delete_row(
        &mut self,
        ui: &mut Ui,
        t: &Theme,
        lang: Lang,
        active: usize,
        (x, y, w): (f32, f32, f32),
    ) -> f32 {
        if self.confirm_delete != Some(active) {
            self.ask_delete_of = Some(active);
            let row = Rect::new(x, y, w, ROW_H);
            if let Some(shown) = intersect(row, self.clip) {
                self.register(PanelHit::AskDelete, shown);
            }
            let hover = self.hover_amount(PanelHit::AskDelete);
            ui.rect(row, 9.0, t.hover.with_alpha(t.hover.a * hover));
            ui.icon(ICON_TRASH, x + 18.0, y + ROW_H / 2.0, 15.0, t.text_dim);
            let line_h = ui.measure("Ag", ROW_TEXT).1;
            let text = fit_text(ui, tr(lang, Key::TipSceneDelete), ROW_TEXT, w - 48.0);
            ui.text(
                &text,
                x + 38.0,
                y + (ROW_H - line_h) / 2.0,
                ROW_TEXT,
                t.text,
            );
            return ROW_H;
        }
        let line_h = ui.measure("Ag", ROW_TEXT).1;
        let ask = fit_text(ui, tr(lang, Key::SceneDeleteAsk), ROW_TEXT, w * 0.34);
        ui.text(&ask, x + 10.0, y + (ROW_H - line_h) / 2.0, ROW_TEXT, t.text);
        // The buttons, side by side at the right.
        let button_w = (w - 10.0) * 0.33;
        let yes = Rect::new(x + w - button_w, y, button_w, ROW_H);
        let no = Rect::new(yes.x - 6.0 - button_w, y, button_w, ROW_H);
        let hit = self.item(yes, Action::Scene(SceneAction::Delete(active)));
        let hover = self.hover_amount(hit);
        ui.rect(yes, 9.0, t.error.with_alpha(0.16 + 0.14 * hover));
        self.button_label(ui, tr(lang, Key::SceneDeleteYes), yes, t.error);
        if let Some(shown) = intersect(no, self.clip) {
            self.register(PanelHit::CancelDelete, shown);
        }
        let hover = self.hover_amount(PanelHit::CancelDelete);
        ui.rect(no, 9.0, t.hover.with_alpha(t.hover.a * hover));
        self.button_label(ui, tr(lang, Key::SceneDeleteNo), no, t.text);
        ROW_H
    }

    /// `label` centred in `rect`.
    fn button_label(&self, ui: &mut Ui, label: &str, rect: Rect, color: Color) {
        let text = fit_text(ui, label, ROW_TEXT, rect.w - 8.0);
        let (text_w, line_h) = ui.measure(&text, ROW_TEXT);
        ui.text(
            &text,
            rect.x + (rect.w - text_w) / 2.0,
            rect.y + (rect.h - line_h) / 2.0,
            ROW_TEXT,
            color,
        );
    }

    /// A row that is a button: an icon and a label. Returns its height.
    fn action_row(
        &mut self,
        ui: &mut Ui,
        t: &Theme,
        icon: char,
        label: &str,
        action: Action,
        (x, y, w): (f32, f32, f32),
    ) -> f32 {
        let row = Rect::new(x, y, w, ROW_H);
        let hit = self.item(row, action);
        let hover = self.hover_amount(hit);
        ui.rect(row, 9.0, t.hover.with_alpha(t.hover.a * hover));
        ui.icon(icon, x + 18.0, y + ROW_H / 2.0, 15.0, t.text_dim);
        let line_h = ui.measure("Ag", ROW_TEXT).1;
        let text = fit_text(ui, label, ROW_TEXT, w - 38.0 - 10.0);
        ui.text(
            &text,
            x + 38.0,
            y + (ROW_H - line_h) / 2.0,
            ROW_TEXT,
            t.text,
        );
        ROW_H
    }
}
