//! The Scenes section: pick a scene, make or delete one, and edit the layers
//! of the one in use (show or hide, remove, add a source). The same choices
//! and actions as the scenes popover; with the room the panel has, adding a
//! source needs no view of its own: the sources are listed right there.

use dockview_domain::i18n::{tr, Key};
use dockview_domain::theme::Theme;
use dockview_domain::view::{Action, Model, SceneAction};

use super::widgets::{list_height, GAP, LABEL_H, ROW_H};
use super::{StudioPanel, PAD};
use crate::fit::fit_text;
use crate::text::Weight;
use crate::{Rect, TextStyle, Ui};

/// Scenes listed before the rest are cut off (the plain source is one more).
const MAX_SCENES: usize = 8;
/// Most cards, screens or windows listed to add.
const MAX_ADD: usize = 5;
const PAD_TOP: f32 = 4.0;
const PAD_BOTTOM: f32 = 14.0;
/// Size of the small icon buttons inside a layer's row.
const ICON_BUTTON: f32 = 26.0;

const ROW_TEXT: TextStyle = TextStyle::new(13.0, Weight::Regular);
const TAG: TextStyle = TextStyle::new(11.5, Weight::Medium);

const ICON_PLUS: char = '\u{e13d}';
const ICON_EYE: char = '\u{e0ba}';
const ICON_EYE_OFF: char = '\u{e0bb}';
const ICON_TRASH: char = '\u{e18d}';
const ICON_CLOSE: char = '\u{e1b2}';

/// The scenes as a list: the plain source first, then each scene.
fn scene_names(m: &Model) -> Vec<String> {
    let mut names = vec![tr(m.lang, Key::ScenePlain).to_owned()];
    names.extend(m.scenes.names.iter().cloned());
    names
}

/// What choosing row `i` of that list asks for.
fn activate(i: usize) -> Action {
    Action::Scene(SceneAction::Activate(i.checked_sub(1)))
}

/// How tall the body is: the list, the buttons and, with a scene in use, its
/// layers and the sources that can be added.
pub fn body_height(m: &Model) -> f32 {
    let s = &m.scenes;
    let mut height = PAD_TOP + list_height(1 + s.names.len(), MAX_SCENES + 1) + GAP + ROW_H;
    if s.active.is_some() {
        height += ROW_H; // delete this scene
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

        let selected = Some(s.active.map_or(0, |i| i + 1));
        y += self.list(
            ui,
            t,
            lang,
            &scene_names(m),
            selected,
            MAX_SCENES + 1,
            &activate,
            x,
            y,
            w,
        ) + GAP;
        y += self.action_row(
            ui,
            t,
            ICON_PLUS,
            tr(lang, Key::SceneNew),
            Action::Scene(SceneAction::New),
            (x, y, w),
        );
        let Some(active) = s.active else {
            return;
        };
        y += self.action_row(
            ui,
            t,
            ICON_TRASH,
            tr(lang, Key::TipSceneDelete),
            Action::Scene(SceneAction::Delete(active)),
            (x, y, w),
        );

        y += GAP;
        y += self.caption(ui, t, tr(lang, Key::SceneLayers), x, y);
        for (i, layer) in s.layers.iter().enumerate() {
            self.layer_row(ui, m, i, layer.visible, &layer.name, (x, y, w));
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

    /// A layer of the scene in use: an eye that shows or hides it, its name
    /// (with a tag on the base) and, but for the base, a button that removes it.
    fn layer_row(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        index: usize,
        visible: bool,
        name: &str,
        (x, y, w): (f32, f32, f32),
    ) {
        let t = &m.theme;
        let line_h = ui.measure("Ag", ROW_TEXT).1;
        let button_y = y + (ROW_H - ICON_BUTTON) / 2.0;

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
            let hit = self.item(remove, Action::Scene(SceneAction::RemoveLayer(index)));
            let hover = self.hover_amount(hit);
            ui.rect(remove, 8.0, t.hover.with_alpha(t.hover.a * hover));
            let (cx, cy) = remove.center();
            ui.icon(ICON_CLOSE, cx, cy, 14.0, t.text_dim);
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
}
