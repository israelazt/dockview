//! The layers of the scene in use, as rows of the Scenes section: choose one
//! for the editor, show or hide it, move it in the drawing order, remove it.

use dockview_domain::i18n::{tr, Key};
use dockview_domain::theme::Theme;
use dockview_domain::view::{Action, Model, SceneAction};

use super::scenes::ROW_TEXT;
use super::widgets::ROW_H;
use super::StudioPanel;
use crate::fit::fit_text;
use crate::text::Weight;
use crate::{Rect, TextStyle, Ui};

/// Size of the small icon buttons inside a layer's row.
const ICON_BUTTON: f32 = 26.0;
pub(super) const TAG: TextStyle = TextStyle::new(11.5, Weight::Medium);
const ICON_EYE: char = '\u{e0ba}';
const ICON_EYE_OFF: char = '\u{e0bb}';
const ICON_CLOSE: char = '\u{e1b2}';
const ICON_UP: char = '\u{e070}';
const ICON_DOWN: char = '\u{e06d}';

impl StudioPanel {
    /// A layer of the scene in use: an eye that shows or hides it, its name
    /// (choosing it for the editor when clicked, with a tag on the base) and,
    /// but for the base, buttons that move it up or down in the drawing order
    /// and remove it.
    pub(super) fn layer_row(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        index: usize,
        (x, y, w): (f32, f32, f32),
    ) {
        let t = &m.theme;
        let layer = &m.scenes.layers[index];
        let (visible, name) = (layer.visible, layer.name.as_str());
        let layers = m.scenes.layers.len();
        let chosen = m.scenes.selected == Some(index);
        let line_h = ui.measure("Ag", ROW_TEXT).1;
        let button_y = y + (ROW_H - ICON_BUTTON) / 2.0;

        // The row itself chooses the layer; the buttons are on top of it.
        let row = Rect::new(x, y, w, ROW_H);
        let pick = self.item(row, Action::Scene(SceneAction::SelectLayer(Some(index))));
        let hover = self.hover_amount(pick);
        if chosen {
            ui.rect(row, 9.0, t.accent.with_alpha(0.16));
        }
        ui.rect(row, 9.0, t.hover.with_alpha(t.hover.a * hover));

        let eye = Rect::new(x, button_y, ICON_BUTTON, ICON_BUTTON);
        let hit = self.item(eye, Action::Scene(SceneAction::ToggleLayer(index)));
        let hover = self.hover_amount(hit);
        ui.rect(eye, 8.0, t.hover.with_alpha(t.hover.a * hover));
        let (cx, cy) = eye.center();
        let icon = if visible { ICON_EYE } else { ICON_EYE_OFF };
        ui.icon(
            icon,
            cx,
            cy,
            15.0,
            if visible { t.text } else { t.text_dim },
        );

        let mut right = x + w;
        if index > 0 {
            let remove = Rect::new(right - ICON_BUTTON, button_y, ICON_BUTTON, ICON_BUTTON);
            self.icon_item(
                ui,
                t,
                remove,
                ICON_CLOSE,
                14.0,
                Action::Scene(SceneAction::RemoveLayer(index)),
            );
            right -= ICON_BUTTON + 2.0;
            // Down is towards the base; the top layer cannot go higher and the
            // first one over the base cannot go lower.
            if index + 1 < layers {
                let up = Rect::new(right - ICON_BUTTON, button_y, ICON_BUTTON, ICON_BUTTON);
                self.icon_item(
                    ui,
                    t,
                    up,
                    ICON_UP,
                    14.0,
                    Action::Scene(SceneAction::MoveLayer(index, 1)),
                );
            }
            right -= ICON_BUTTON;
            if index > 1 {
                let down = Rect::new(right - ICON_BUTTON, button_y, ICON_BUTTON, ICON_BUTTON);
                self.icon_item(
                    ui,
                    t,
                    down,
                    ICON_DOWN,
                    14.0,
                    Action::Scene(SceneAction::MoveLayer(index, -1)),
                );
            }
            right -= ICON_BUTTON + 4.0;
        } else {
            let tag = tr(m.lang, Key::SceneBaseTag);
            let tag_w = ui.measure(tag, TAG).0;
            ui.text(
                tag,
                right - tag_w - 6.0,
                y + (ROW_H - line_h) / 2.0 + 1.0,
                TAG,
                t.text_dim,
            );
            right -= tag_w + 14.0;
        }
        let text_x = x + ICON_BUTTON + 8.0;
        let label = fit_text(ui, name, ROW_TEXT, (right - text_x).max(0.0));
        let color = if visible { t.text } else { t.text_dim };
        ui.text(&label, text_x, y + (ROW_H - line_h) / 2.0, ROW_TEXT, color);
    }

    /// A small square button with an icon that asks for `action`.
    fn icon_item(
        &mut self,
        ui: &mut Ui,
        t: &Theme,
        rect: Rect,
        icon: char,
        size: f32,
        action: Action,
    ) {
        let hit = self.item(rect, action);
        let hover = self.hover_amount(hit);
        ui.rect(rect, 8.0, t.hover.with_alpha(t.hover.a * hover));
        let (cx, cy) = rect.center();
        ui.icon(icon, cx, cy, size, t.text_dim);
    }
}
