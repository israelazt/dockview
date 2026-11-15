//! What the pointer does to the panel: presses, drags, clicks and the wheel.

use dockview_domain::view::Action;

use super::sections::StudioSection;
use super::{edit, scroll, PanelHit, StudioPanel};
use crate::input::Pointer;
use crate::Rect;

impl StudioPanel {
    pub(super) fn interact(
        &mut self,
        pointer: &Pointer,
        visible: Rect,
        content_h: f32,
        list_h: f32,
    ) {
        if pointer.just_pressed {
            self.pressed = self.hovered;
            if self.hovered != Some(PanelHit::EditField) {
                self.editing = None;
            }
        }
        // A slider follows the pointer for as long as it is held.
        if let (Some(PanelHit::Slider(i)), Some((x, _))) = (self.pressed, pointer.pos) {
            if pointer.down || pointer.just_pressed {
                self.drag_slider(i, x);
            }
        }
        // A layer held by its grip follows the pointer; where it lands is
        // decided when it is let go.
        if let (Some(PanelHit::Grip(layer)), Some((_, y)), Some((top, rows))) =
            (self.pressed, pointer.pos, self.layer_rows)
        {
            if pointer.down {
                self.reorder = Some((
                    layer,
                    super::layers_card::reorder_target(y, top, rows, layer),
                ));
            }
        }
        if !pointer.down {
            if let Some((layer, target)) = self.reorder.take() {
                self.actions
                    .extend(super::layers_card::reorder_actions(layer, target));
            }
        }
        if pointer.just_released {
            if let (Some(pressed), Some(hovered)) = (self.pressed, self.hovered) {
                if pressed == hovered {
                    self.activate(pressed);
                }
            }
            self.pressed = None;
        }
        let over_panel = pointer.pos.is_some_and(|(x, y)| visible.contains(x, y));
        if pointer.wheel != 0.0 && over_panel {
            self.scroll_target =
                scroll::scroll_by(self.scroll_target, pointer.wheel, content_h, list_h);
        }
    }

    /// Asks for the value of slider `i` that the pointer at `x` points at.
    fn drag_slider(&mut self, i: usize, x: f32) {
        let Some(slider) = self.sliders.get(i).copied() else {
            return;
        };
        let value = slider.value_at(x);
        if (value - slider.value).abs() > f32::EPSILON {
            self.actions.push((slider.make)(value));
            // Until the application answers, the slider shows where it was
            // dragged to (so a fast drag is not asked for twice).
            if let Some(shown) = self.sliders.get_mut(i) {
                shown.value = value;
            }
        }
    }

    fn activate(&mut self, hit: PanelHit) {
        match hit {
            PanelHit::Close => self.actions.push(Action::ToggleStudio),
            PanelHit::Slider(_) | PanelHit::Grip(_) => {}
            PanelHit::AskDelete => self.confirm_delete = self.ask_delete_of,
            PanelHit::CancelDelete => self.confirm_delete = None,
            PanelHit::Rename => {
                if let Some((index, name)) = self.rename_of.clone() {
                    self.editing = Some((index, edit::TextEdit::new(&name)));
                }
            }
            PanelHit::EditField => {}
            PanelHit::Item(i) => {
                if let Some(action) = self.items.get(i) {
                    // Whatever is answered, the question is over.
                    self.confirm_delete = None;
                    self.actions.push(action.clone());
                }
            }
            PanelHit::Header(section) => {
                if let Some(i) = StudioSection::ALL.iter().position(|s| *s == section) {
                    self.open[i] = !self.open[i];
                }
            }
        }
    }
}
