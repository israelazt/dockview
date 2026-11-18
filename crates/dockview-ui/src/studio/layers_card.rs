//! The card of the layers of the scene in use (the left column): one row per
//! layer with its eye, the kind of source and its name; under them a line that
//! counts them with the buttons that move or remove the chosen one; and the
//! lists of what can be added. With no scene in use the card lists the sources
//! instead (see `sources.rs`).

use dockview_domain::i18n::{tr, Key};
use dockview_domain::view::{Action, LayerKind, Model, SceneAction};

use super::scene_layers::{ICON_BUTTON, ICON_CLOSE, ICON_DOWN, ICON_EYE, ICON_EYE_OFF, ICON_UP};
use super::scenes::{add_height, ROW_TEXT};
use super::widgets::GAP;
use super::{intersect, PanelHit, StudioPanel, NOTE, PAD};
use crate::fit::fit_text;
use crate::{Rect, Ui};

/// Height of one layer row.
pub(super) const LAYER_ROW_H: f32 = 48.0;
/// Height of the line under the rows (the count and the buttons).
const FOOTER_H: f32 = 40.0;
const PAD_TOP: f32 = 4.0;
const PAD_BOTTOM: f32 = 14.0;
// Lucide code points.
const ICON_CAMERA: char = '\u{e064}';
const ICON_SCREEN: char = '\u{e11d}';
const ICON_WINDOW: char = '\u{e426}';
const ICON_GRIP: char = '\u{e0eb}';
const ICON_LOCK: char = '\u{e10b}';
const ICON_UNLOCK: char = '\u{e10c}';

/// Width of the grip at the left of a row.
const GRIP_W: f32 = 18.0;

/// The layer a drag at height `y` lands on: rows are listed top layer first
/// from `top`, `rows` of them; the base (0) never moves and nothing goes under
/// it. A drag that cannot land anywhere keeps the layer where it is.
pub(super) fn reorder_target(y: f32, top: f32, rows: usize, layer: usize) -> usize {
    if rows < 3 || layer == 0 || layer >= rows {
        return layer;
    }
    let shown = (((y - top) / LAYER_ROW_H).floor().max(0.0) as usize).min(rows - 1);
    (rows - 1 - shown).clamp(1, rows - 1)
}

/// The steps that take `layer` to `target`, one position at a time (each step
/// names where the layer is by then).
pub(super) fn reorder_actions(layer: usize, target: usize) -> Vec<Action> {
    let dir: i32 = if target > layer { 1 } else { -1 };
    (0..layer.abs_diff(target))
        .map(|k| {
            let at = (layer as i64 + i64::from(dir) * k as i64) as usize;
            Action::Scene(SceneAction::MoveLayer(at, dir))
        })
        .collect()
}

/// How tall the card is for the scene in use.
pub(super) fn body_height(m: &Model) -> f32 {
    let s = &m.scenes;
    let mut height = PAD_TOP + s.layers.len() as f32 * LAYER_ROW_H + FOOTER_H;
    if s.can_add {
        height += GAP + add_height(m);
    }
    height + PAD_BOTTOM
}

fn kind_icon(kind: LayerKind) -> char {
    match kind {
        LayerKind::Capture => ICON_CAMERA,
        LayerKind::Monitor => ICON_SCREEN,
        LayerKind::Window => ICON_WINDOW,
    }
}

fn kind_label(kind: LayerKind) -> Key {
    match kind {
        LayerKind::Capture => Key::SourceCapture,
        LayerKind::Monitor => Key::SourceScreen,
        LayerKind::Window => Key::SourceApp,
    }
}

/// What the buttons of the footer can do for the chosen layer: move up, move
/// down, remove. The base (the first) never moves or goes; the last one cannot
/// go higher and the one over the base cannot go lower.
pub(super) fn footer_options(selected: Option<usize>, layers: usize) -> (bool, bool, bool) {
    match selected.filter(|i| *i < layers) {
        None => (false, false, false),
        // The main picture stays at the bottom, but it can be removed.
        Some(0) => (false, false, true),
        Some(i) => (i + 1 < layers, i > 1, true),
    }
}

impl StudioPanel {
    /// Draws the card's body with its top-left corner at the corner of `body`.
    pub(super) fn draw_layers_card(&mut self, ui: &mut Ui, m: &Model, body: Rect) {
        let t = &m.theme;
        let (x, w) = (body.x + PAD, body.w - PAD * 2.0);
        let mut y = body.y + PAD_TOP;
        let s = &m.scenes;
        let (name_h, note_h) = (ui.measure("Ag", ROW_TEXT).1, ui.measure("Ag", NOTE).1);
        let count = s.layers.len();
        self.layer_rows = Some((y, count));
        let rows_top = y;
        // The top layer is listed first and the base last, so "up" is up.
        for (i, layer) in s.layers.iter().enumerate().rev() {
            let row = Rect::new(x, y + 2.0, w, LAYER_ROW_H - 4.0);
            let cy = y + LAYER_ROW_H / 2.0;
            let pick = self.item(row, Action::Scene(SceneAction::SelectLayer(Some(i))));
            let hover = self.hover_amount(pick);
            if s.selected == Some(i) {
                ui.rect(row, 10.0, t.accent.with_alpha(0.16));
            }
            ui.rect(row, 10.0, t.hover.with_alpha(t.hover.a * hover));
            if i > 0 {
                // The grip: held and dragged, it moves the layer in the order.
                let grip = Rect::new(x, row.y, GRIP_W, row.h);
                if let Some(shown) = intersect(grip, self.clip) {
                    self.register(PanelHit::Grip(i), shown);
                }
                let lit = self.reorder.is_some_and(|(held, _)| held == i);
                let tone = if lit || self.hovered == Some(PanelHit::Grip(i)) {
                    t.text
                } else {
                    t.text_dim.with_alpha(0.6)
                };
                let (gx, gy) = grip.center();
                ui.icon(ICON_GRIP, gx + 1.0, gy, 14.0, tone);
            }
            let buttons_x = x + GRIP_W;
            // The eye shows or hides the layer; it is on top of the row.
            let eye = Rect::new(buttons_x, cy - ICON_BUTTON / 2.0, ICON_BUTTON, ICON_BUTTON);
            let hit = self.item(eye, Action::Scene(SceneAction::ToggleLayer(i)));
            let eye_hover = self.hover_amount(hit);
            ui.rect(eye, 8.0, t.hover.with_alpha(t.hover.a * eye_hover));
            let (ex, ey) = eye.center();
            let (icon, tone) = if layer.visible {
                (ICON_EYE, t.text)
            } else {
                (ICON_EYE_OFF, t.text_dim)
            };
            ui.icon(icon, ex, ey, 15.0, tone);
            // The lock keeps the editor over the picture away from the layer.
            let lock = Rect::new(eye.x + ICON_BUTTON, eye.y, ICON_BUTTON, ICON_BUTTON);
            let hit = self.item(lock, Action::Scene(SceneAction::ToggleLock(i)));
            let lock_hover = self.hover_amount(hit);
            ui.rect(lock, 8.0, t.hover.with_alpha(t.hover.a * lock_hover));
            let (lx, ly) = lock.center();
            let (icon, tone) = if layer.locked {
                (ICON_LOCK, t.accent)
            } else {
                (ICON_UNLOCK, t.text_dim.with_alpha(0.55))
            };
            ui.icon(icon, lx, ly, 14.0, tone);
            let kind_x = lock.x + ICON_BUTTON + 12.0;
            ui.icon(kind_icon(layer.kind), kind_x, cy, 15.0, t.text_dim);
            let text_x = kind_x + 14.0;
            let room = (x + w - 6.0 - text_x).max(0.0);
            let mut note = tr(m.lang, kind_label(layer.kind)).to_owned();
            if i == 0 {
                note = format!("{note} · {}", tr(m.lang, Key::SceneBaseTag));
            }
            let top = cy - (name_h + note_h + 1.0) / 2.0;
            let name = fit_text(ui, &layer.name, ROW_TEXT, room);
            let color = if layer.visible { t.text } else { t.text_dim };
            ui.text(&name, text_x, top, ROW_TEXT, color);
            let note = fit_text(ui, &note, NOTE, room);
            ui.text(&note, text_x, top + name_h + 1.0, NOTE, t.text_dim);
            y += LAYER_ROW_H;
        }
        // Where the layer being dragged would land.
        if let Some((held, target)) = self.reorder.filter(|(h, tg)| h != tg) {
            let shown = count - 1 - target;
            let edge = if target > held { 0.0 } else { LAYER_ROW_H };
            let line = Rect::new(
                x,
                rows_top + shown as f32 * LAYER_ROW_H + edge - 1.0,
                w,
                2.0,
            );
            ui.rect(line, 1.0, t.accent);
        }
        self.layers_footer(ui, m, (x, y, w));
        y += FOOTER_H;
        if s.can_add {
            y += GAP;
            self.draw_add(ui, m, (x, y, w));
        }
    }

    /// The count of layers at the left and, at the right, the buttons that act
    /// on the chosen layer (dim when they cannot).
    fn layers_footer(&mut self, ui: &mut Ui, m: &Model, (x, y, w): (f32, f32, f32)) {
        let t = &m.theme;
        let s = &m.scenes;
        let n = s.layers.len();
        let word = if n == 1 {
            Key::SceneLayerOne
        } else {
            Key::SceneLayerMany
        };
        let line_h = ui.measure("Ag", NOTE).1;
        let count = format!("{n} {}", tr(m.lang, word));
        ui.text(
            &count,
            x + 10.0,
            y + (FOOTER_H - line_h) / 2.0,
            NOTE,
            t.text_dim,
        );
        let (up, down, remove) = footer_options(s.selected, n);
        let Some(sel) = s.selected.filter(|i| *i < n) else {
            return;
        };
        let buttons = [
            (ICON_UP, up, Action::Scene(SceneAction::MoveLayer(sel, 1))),
            (
                ICON_DOWN,
                down,
                Action::Scene(SceneAction::MoveLayer(sel, -1)),
            ),
            (
                ICON_CLOSE,
                remove,
                Action::Scene(SceneAction::RemoveLayer(sel)),
            ),
        ];
        let mut right = x + w;
        for (icon, enabled, action) in buttons.into_iter().rev() {
            let rect = Rect::new(
                right - ICON_BUTTON,
                y + (FOOTER_H - ICON_BUTTON) / 2.0,
                ICON_BUTTON,
                ICON_BUTTON,
            );
            if enabled {
                self.icon_item(ui, t, rect, icon, 14.0, action);
            } else {
                let (cx, cy) = rect.center();
                ui.icon(icon, cx, cy, 14.0, t.text_dim.with_alpha(0.3));
            }
            right -= ICON_BUTTON + 2.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_chosen_nothing_to_do() {
        assert_eq!(footer_options(None, 3), (false, false, false));
        assert_eq!(footer_options(Some(7), 3), (false, false, false));
    }

    #[test]
    fn the_base_stays_where_it_is_but_can_be_removed() {
        assert_eq!(footer_options(Some(0), 3), (false, false, true));
    }

    #[test]
    fn the_layer_over_the_base_can_only_go_up_and_the_top_one_only_down() {
        assert_eq!(footer_options(Some(1), 3), (true, false, true));
        assert_eq!(footer_options(Some(2), 3), (false, true, true));
        assert_eq!(footer_options(Some(1), 2), (false, false, true));
    }

    #[test]
    fn every_kind_has_its_own_icon_and_words() {
        let kinds = [LayerKind::Capture, LayerKind::Monitor, LayerKind::Window];
        for (i, a) in kinds.iter().enumerate() {
            for b in &kinds[i + 1..] {
                assert_ne!(kind_icon(*a), kind_icon(*b));
                assert_ne!(kind_label(*a), kind_label(*b));
            }
        }
    }
}
