//! What a click, a press or a drag on the controls does.

use super::*;

impl Controls {
    pub(super) fn interact(&mut self, model: &Model, pointer: &Pointer) {
        if pointer.just_pressed {
            self.pressed = self.hovered;
            // A press anywhere but on the length chip or its selector closes it.
            if !matches!(
                self.hovered,
                Some(Hit::ReplayLength | Hit::LengthLess | Hit::LengthMore | Hit::LengthPopover)
            ) {
                self.length_open = false;
            }
            if self.hovered == Some(Hit::Volume) {
                self.drag_volume(model, pointer);
            }
        } else if pointer.down && self.pressed == Some(Hit::Volume) {
            self.drag_volume(model, pointer);
        }

        if pointer.just_released {
            if let (Some(pressed), Some(hovered)) = (self.pressed, self.hovered) {
                if pressed == hovered {
                    self.activate(pressed);
                }
            }
            self.pressed = None;
        }

        if pointer.wheel != 0.0 && self.hovered.is_some() {
            let volume = (model.volume + pointer.wheel * WHEEL_VOLUME_STEP).clamp(0.0, 1.0);
            self.actions.push(Action::SetVolume(round_percent(volume)));
        }
    }

    pub(super) fn drag_volume(&mut self, model: &Model, pointer: &Pointer) {
        let Some((x, _)) = pointer.pos else {
            return;
        };
        let value =
            round_percent(((x - self.volume_track.x) / self.volume_track.w).clamp(0.0, 1.0));
        if (value - model.volume).abs() > f32::EPSILON {
            self.actions.push(Action::SetVolume(value));
        }
    }

    pub(super) fn activate(&mut self, hit: Hit) {
        let action = match hit {
            Hit::ReplaySave => Action::SaveReplay,
            Hit::Record => Action::ToggleRecord,
            Hit::Capture => Action::Capture,
            Hit::Mic => Action::ToggleMic,
            Hit::Hud => Action::ToggleHud,
            Hit::Fullscreen => Action::ToggleFullscreen,
            Hit::ReplayLength => {
                self.length_open = !self.length_open;
                return;
            }
            Hit::LengthLess => Action::ReplayLength(-1),
            Hit::LengthMore => Action::ReplayLength(1),
            Hit::Pill | Hit::Volume | Hit::LengthPopover => return,
        };
        self.actions.push(action);
    }
}
