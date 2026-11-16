//! The scenes as rows: a dot that says which one is on air, its name, a line
//! under it with what it holds and, at the right, the key that switches to it.

use dockview_domain::i18n::{tr, Key};
use dockview_domain::view::{Action, Model, SceneAction};

use super::scene_layers::TAG;
use super::scenes::{MAX_SCENES, ROW_TEXT};
use super::{StudioPanel, NOTE};
use crate::fit::fit_text;
use crate::{Rect, Ui};

/// Height of one scene row.
pub(super) const SCENE_ROW_H: f32 = 52.0;

/// How many rows the list shows: the plain source and each scene, up to the cap.
pub(super) fn row_count(m: &Model) -> usize {
    (1 + m.scenes.names.len()).min(MAX_SCENES + 1)
}

/// The name of row `i` (the plain source first, then each scene).
fn name(m: &Model, i: usize) -> String {
    match i.checked_sub(1) {
        None => tr(m.lang, Key::ScenePlain).to_owned(),
        Some(s) => m.scenes.names.get(s).cloned().unwrap_or_default(),
    }
}

/// The line under the name: how many layers the scene has.
pub(super) fn subtitle(m: &Model, i: usize) -> String {
    let Some(scene) = i.checked_sub(1) else {
        return tr(m.lang, Key::ScenePlainNote).to_owned();
    };
    let n = m.scenes.layer_counts.get(scene).copied().unwrap_or(0);
    let word = if n == 1 {
        Key::SceneLayerOne
    } else {
        Key::SceneLayerMany
    };
    format!("{n} {}", tr(m.lang, word))
}

/// The global hotkey of row `i`: Ctrl+Alt+F12 for the plain source and
/// Ctrl+Alt+F1 to F9 for the first nine scenes.
pub(super) fn hotkey(i: usize) -> Option<String> {
    match i {
        0 => Some("F12".to_owned()),
        1..=9 => Some(format!("F{i}")),
        _ => None,
    }
}

impl StudioPanel {
    /// Draws the rows at `(x, y)`, `w` wide. Returns their height.
    pub(super) fn scene_rows(&mut self, ui: &mut Ui, m: &Model, (x, y, w): (f32, f32, f32)) -> f32 {
        let t = &m.theme;
        let active = m.scenes.active.map_or(0, |i| i + 1);
        let (name_h, note_h) = (ui.measure("Ag", ROW_TEXT).1, ui.measure("Ag", NOTE).1);
        for i in 0..row_count(m) {
            let top = y + i as f32 * SCENE_ROW_H;
            let row = Rect::new(x, top + 2.0, w, SCENE_ROW_H - 4.0);
            let hit = self.item(row, Action::Scene(SceneAction::Activate(i.checked_sub(1))));
            let hover = self.hover_amount(hit);
            let on = i == active;
            if on {
                ui.rect(row, 10.0, t.accent.with_alpha(0.16));
            }
            ui.rect(row, 10.0, t.hover.with_alpha(t.hover.a * hover));
            let cy = top + SCENE_ROW_H / 2.0;
            let dot = if on {
                t.brand
            } else {
                t.text_dim.with_alpha(0.35)
            };
            ui.rect(Rect::new(x + 11.0, cy - 3.5, 7.0, 7.0), 3.5, dot);
            // The key, in a chip at the right, takes its room first.
            let mut right = x + w - 8.0;
            if let Some(key) = hotkey(i) {
                let chip_w = ui.measure(&key, TAG).0 + 12.0;
                let chip = Rect::new(right - chip_w, cy - 9.0, chip_w, 18.0);
                ui.rect(chip, 6.0, t.hover.with_alpha(t.hover.a * 1.5));
                let key_h = ui.measure(&key, TAG).1;
                ui.text(&key, chip.x + 6.0, cy - key_h / 2.0, TAG, t.text_dim);
                right -= chip_w + 8.0;
            }
            let text_x = x + 28.0;
            let label = fit_text(ui, &name(m, i), ROW_TEXT, (right - text_x).max(0.0));
            let color = if on { t.text } else { t.text_dim };
            ui.text(
                &label,
                text_x,
                cy - (name_h + note_h + 1.0) / 2.0,
                ROW_TEXT,
                color,
            );
            let note = fit_text(ui, &subtitle(m, i), NOTE, (right - text_x).max(0.0));
            ui.text(
                &note,
                text_x,
                cy - (name_h + note_h + 1.0) / 2.0 + name_h + 1.0,
                NOTE,
                t.text_dim,
            );
        }
        row_count(m) as f32 * SCENE_ROW_H
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_model::model;

    #[test]
    fn the_plain_source_comes_first_with_its_own_key() {
        let mut m = model();
        m.scenes.names = vec!["Juego".into()];
        m.scenes.layer_counts = vec![3];
        assert_eq!(name(&m, 0), tr(m.lang, Key::ScenePlain));
        assert_eq!(name(&m, 1), "Juego");
        assert_eq!(hotkey(0).as_deref(), Some("F12"));
        assert_eq!(hotkey(1).as_deref(), Some("F1"));
        assert_eq!(hotkey(9).as_deref(), Some("F9"));
        assert_eq!(hotkey(10), None);
    }

    #[test]
    fn the_line_under_the_name_counts_the_layers() {
        let mut m = model();
        m.scenes.names = vec!["A".into(), "B".into(), "C".into()];
        m.scenes.layer_counts = vec![1, 3];
        assert_eq!(subtitle(&m, 0), tr(m.lang, Key::ScenePlainNote));
        assert_eq!(
            subtitle(&m, 1),
            format!("1 {}", tr(m.lang, Key::SceneLayerOne))
        );
        assert_eq!(
            subtitle(&m, 2),
            format!("3 {}", tr(m.lang, Key::SceneLayerMany))
        );
        assert!(
            subtitle(&m, 3).starts_with("0 "),
            "a count that is missing is none"
        );
    }

    #[test]
    fn the_list_is_capped() {
        let mut m = model();
        m.scenes.names = (0..20).map(|i| i.to_string()).collect();
        assert_eq!(row_count(&m), MAX_SCENES + 1);
    }
}
