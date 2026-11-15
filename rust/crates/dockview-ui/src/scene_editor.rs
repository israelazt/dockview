//! The free layer editor: outlines and handles drawn over the picture, and the
//! pointer that picks, moves and resizes layers. The geometry is in
//! `dockview_domain::layer_edit`; this turns the pointer into [`Action`]s and
//! draws what it is doing.

use dockview_domain::layer_edit::{
    drag_to, handle_at, pick_layer, to_units, Drag, Handle, Modifiers, Picture,
};
use dockview_domain::scene::NormRect;
use dockview_domain::view::{Action, Model, SceneAction};

use crate::input::Pointer;
use crate::text::Weight;
use crate::{Rect, TextStyle, Ui};

/// The size the scenes are drawn at: the editor's picture has this proportion.
pub const PICTURE_SIZE: (u32, u32) = (1920, 1080);
/// Side of a handle, in logical pixels.
const HANDLE: f32 = 9.0;
const LABEL: TextStyle = TextStyle::new(11.5, Weight::Medium);

/// A layer held by the pointer.
#[derive(Clone, Copy)]
struct Held {
    layer: usize,
    drag: Drag,
    /// Where the pointer went down, on screen.
    press_at: (f32, f32),
    /// The pointer has gone further than a click wobbles; until then nothing
    /// is moved (a press at rest must not nudge a layer towards a snap line).
    moved: bool,
}

/// How far the pointer goes before a press becomes a drag, in pixels.
const DRAG_SLOP: f32 = 3.0;

#[derive(Default)]
pub struct SceneEditor {
    /// The layer being dragged, how, and whether the pointer has really moved.
    drag: Option<Held>,
    /// Where the layer being dragged is now, before the application answers.
    live: Option<(usize, NormRect)>,
    /// Frames since the drag ended with the drawn rectangle still not the model's.
    settle: u8,
    /// What the pointer is over, for the cursor.
    hover: Option<Handle>,
    actions: Vec<Action>,
}

impl SceneEditor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// What the pointer is over (a handle or a body), for the cursor shape.
    pub fn hover(&self) -> Option<Handle> {
        self.hover
    }

    /// Whether a drag is in progress.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Runs one frame. `area` is the room the picture has (x, y, width, height,
    /// in logical pixels); `pointer` is what the editor may use (what the
    /// controls and the panel did not take).
    pub fn frame(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        area: (f32, f32, f32, f32),
        pointer: &Pointer,
        mods: Modifiers,
    ) {
        let s = &m.scenes;
        if !s.editing || s.layers.is_empty() {
            self.drag = None;
            self.live = None;
            self.hover = None;
            return;
        }
        let picture = Picture::fitted(area, PICTURE_SIZE);
        self.interact(m, &picture, pointer, mods);
        self.draw(ui, m, &picture);
    }

    /// The rectangle of layer `i`: where it is being dragged to, else where the
    /// model has it.
    fn rect_of(&self, m: &Model, i: usize) -> NormRect {
        match self.live {
            Some((live, rect)) if live == i => rect,
            _ => m.scenes.layers[i].rect,
        }
    }

    fn interact(&mut self, m: &Model, picture: &Picture, pointer: &Pointer, mods: Modifiers) {
        let layers = &m.scenes.layers;
        let selected = m.scenes.selected.filter(|i| *i < layers.len());
        let at = pointer.pos;

        self.hover = match (at, selected, self.drag) {
            (_, _, Some(held)) => Some(held.drag.handle),
            (Some((x, y)), Some(i), None) => handle_at(picture, self.rect_of(m, i), x, y),
            _ => None,
        };

        if pointer.just_pressed {
            if let Some((x, y)) = at.filter(|(x, y)| picture.contains(*x, *y)) {
                self.press(m, picture, selected, (x, y));
            }
        }
        if let (Some(mut held), Some((x, y)), true) = (self.drag, at, pointer.down) {
            let (dx, dy) = (x - held.press_at.0, y - held.press_at.1);
            held.moved |= dx.hypot(dy) > DRAG_SLOP;
            self.drag = Some(held);
            if held.moved {
                let others: Vec<NormRect> = (0..layers.len())
                    .filter(|i| *i != held.layer && layers[*i].visible)
                    .map(|i| self.rect_of(m, i))
                    .collect();
                let rect = drag_to(&held.drag, picture, (x, y), &others, mods);
                if rect != self.rect_of(m, held.layer) {
                    self.live = Some((held.layer, rect));
                    self.actions.push(Action::Scene(SceneAction::SetLayerRect(
                        held.layer,
                        to_units(rect),
                    )));
                }
            }
        }
        if pointer.just_released || !pointer.down {
            self.drag = None;
        }
        // After the drag, the drawn rectangle goes back to the model's once the
        // application has answered (or after a few frames if it never does).
        if self.drag.is_none() {
            if let Some((i, r)) = self.live {
                self.settle += 1;
                let close = |a: f32, b: f32| (a - b).abs() < 2e-4;
                let caught_up = layers.get(i).is_none_or(|l| {
                    close(l.rect.x, r.x)
                        && close(l.rect.y, r.y)
                        && close(l.rect.w, r.w)
                        && close(l.rect.h, r.h)
                });
                if caught_up || self.settle > 3 {
                    self.live = None;
                    self.settle = 0;
                }
            }
        } else {
            self.settle = 0;
        }
    }

    /// A press on the picture: grab a handle of the chosen layer, or choose
    /// the layer under the pointer (and start moving it), or choose none.
    fn press(&mut self, m: &Model, picture: &Picture, selected: Option<usize>, (x, y): (f32, f32)) {
        let locked = |i: usize| m.scenes.layers.get(i).is_some_and(|l| l.locked);
        if let Some(i) = selected.filter(|i| !locked(*i)) {
            let rect = self.rect_of(m, i);
            if let Some(handle) = handle_at(picture, rect, x, y) {
                self.hold(i, Drag::begin(handle, rect, picture, x, y), (x, y));
                return;
            }
        }
        let rects: Vec<(NormRect, bool)> = m
            .scenes
            .layers
            .iter()
            .map(|l| (l.rect, l.visible && !l.locked))
            .collect();
        match pick_layer(picture, &rects, x, y) {
            Some(i) => {
                self.actions
                    .push(Action::Scene(SceneAction::SelectLayer(Some(i))));
                self.hold(
                    i,
                    Drag::begin(Handle::Move, rects[i].0, picture, x, y),
                    (x, y),
                );
            }
            None => self
                .actions
                .push(Action::Scene(SceneAction::SelectLayer(None))),
        }
    }

    fn hold(&mut self, layer: usize, drag: Drag, press_at: (f32, f32)) {
        self.actions
            .push(Action::Scene(SceneAction::BeginEdit(layer)));
        self.drag = Some(Held {
            layer,
            drag,
            press_at,
            moved: false,
        });
    }

    fn draw(&self, ui: &mut Ui, m: &Model, picture: &Picture) {
        let t = &m.theme;
        let layers = &m.scenes.layers;
        let selected = m.scenes.selected.filter(|i| *i < layers.len());
        for (i, layer) in layers.iter().enumerate() {
            if !layer.visible || Some(i) == selected {
                continue;
            }
            let (x, y, w, h) = picture.to_screen(self.rect_of(m, i));
            ui.outline(Rect::new(x, y, w, h), 1.0, t.accent.with_alpha(0.55));
        }
        let Some(i) = selected else {
            return;
        };
        let (x, y, w, h) = picture.to_screen(self.rect_of(m, i));
        let rect = Rect::new(x, y, w, h);
        // A light tint and a frame: the layer underneath stays visible.
        ui.rect(rect, 0.0, t.accent.with_alpha(0.10));
        ui.outline(rect, 2.0, t.accent);
        // The name, on a chip over the top left corner.
        let name = &layers[i].name;
        let (text_w, text_h) = ui.measure(name, LABEL);
        let chip = Rect::new(
            x,
            (y - text_h - 8.0).max(picture.y),
            text_w + 14.0,
            text_h + 6.0,
        );
        ui.rect(chip, 5.0, t.accent);
        ui.text(
            name,
            chip.x + 7.0,
            chip.y + 3.0,
            LABEL,
            dockview_domain::theme::Color::hex(0xFFFFFF, 1.0),
        );
        for handle in Handle::ALL {
            let (ax, ay) = handle.anchor();
            let (cx, cy) = (x + ax * w, y + ay * h);
            let spot = Rect::new(cx - HANDLE / 2.0, cy - HANDLE / 2.0, HANDLE, HANDLE);
            ui.panel(spot, 2.5, t.text, t.accent, 1.5);
        }
    }
}

#[cfg(test)]
mod tests;
