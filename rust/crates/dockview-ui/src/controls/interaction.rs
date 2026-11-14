//! What a click, a press or a drag on the controls does.

use super::*;

impl Controls {
    pub(super) fn interact(&mut self, model: &Model, pointer: &Pointer) {
        if pointer.just_pressed {
            self.pressed = self.hovered;
            if self.hovered == Some(Hit::Volume) {
                self.drag_volume(model, pointer);
            }
        } else if pointer.down && self.pressed == Some(Hit::Volume) {
            self.drag_volume(model, pointer);
        }

        if pointer.just_released {
            if let (Some(pressed), Some(hovered)) = (self.pressed, self.hovered) {
                if pressed == hovered {
                    self.activate(pressed, model);
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

    pub(super) fn activate(&mut self, hit: Hit, model: &Model) {
        let sync =
            |delta: i32| (model.sync_ms + delta).clamp(-MAX_AUDIO_OFFSET_MS, MAX_AUDIO_OFFSET_MS);
        let action = match hit {
            Hit::Settings => {
                self.open_in_panel(StudioSection::Settings);
                return;
            }
            Hit::Replay => {
                self.open_in_panel(StudioSection::Recording);
                return;
            }
            Hit::Sources => {
                self.open_in_panel(StudioSection::Sources);
                return;
            }
            Hit::Scenes => {
                self.open_in_panel(StudioSection::Scenes);
                return;
            }
            Hit::ReplaySave => Action::SaveReplay,
            Hit::Record => Action::ToggleRecord,
            Hit::Fullscreen => Action::ToggleFullscreen,
            Hit::Studio => Action::ToggleStudio,
            Hit::SyncMinus => Action::SetSync(sync(-SYNC_STEP_MS)),
            Hit::SyncPlus => Action::SetSync(sync(SYNC_STEP_MS)),
            Hit::Pill | Hit::Volume => return,
        };
        self.actions.push(action);
    }
}
