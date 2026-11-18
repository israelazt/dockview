//! Changing the layers of the scene in use while it runs: moving, adding and
//! removing them. The first layer is the scene's main source, so changing it
//! starts the scene again; the others only start or stop their own source.

use std::sync::Arc;

use dockview_domain::events::AppEvent;
use dockview_domain::frame::FrameSlot;
use dockview_domain::scene::SourceRef;

use super::Core;
use crate::scenes_service::Overlay;

impl Core {
    /// Moves an overlay one step in the drawing order; the selection follows it.
    pub(super) fn move_layer(&mut self, index: usize, delta: i32) {
        let Some(scene) = self.active_scene_mut() else {
            return;
        };
        let target = index as i64 + i64::from(delta.signum());
        if index == 0 || target < 1 || target >= scene.layers.len() as i64 {
            return;
        }
        let target = target as usize;
        scene.layers.swap(index, target);
        self.forget_layer_edits();
        self.scenes.swap_overlays(index, target);
        if self.view.layer_selected == Some(index) {
            self.view.layer_selected = Some(target);
        } else if self.view.layer_selected == Some(target) {
            self.view.layer_selected = Some(index);
        }
        self.sync_compositor();
    }

    /// Removes a layer. Taking the first one away makes the next the main
    /// picture; taking the only one leaves the scene empty.
    pub(super) fn remove_layer(&mut self, index: usize) {
        let Some(scene) = self.active_scene_mut() else {
            return;
        };
        if index >= scene.layers.len() {
            return;
        }
        scene.layers.remove(index);
        self.forget_layer_edits();
        if index == 0 {
            self.view.layer_selected = None;
            self.rebuild_scene();
            return;
        }
        self.scenes.remove_overlay(index);
        self.sync_compositor();
        self.set_capture_exclusion(self.wants_exclusion());
    }

    /// Starts the active scene again from what is saved, without a transition:
    /// its first layer is the main source (or, with no layer, nothing is).
    pub(super) fn rebuild_scene(&mut self) {
        let Some(scene) = self
            .settings
            .active_scene
            .and_then(|i| self.settings.scenes.get(i).cloned())
        else {
            return;
        };
        self.stop_scene();
        match scene.base() {
            Some(base) => {
                if !self.make_main(base) {
                    self.emit(AppEvent::SceneMissing);
                }
            }
            None => self.stop_main(),
        }
        self.run_scene(&scene);
        self.set_capture_exclusion(self.wants_exclusion());
    }

    /// Adds `source` over the active scene, in the first free corner.
    pub(super) fn add_layer(&mut self, source: Option<SourceRef>) {
        let Some(source) = source else {
            return;
        };
        let Some(scene) = self.active_scene_mut() else {
            return;
        };
        let Some(index) = scene.add_layer(source.clone()) else {
            return;
        };
        // The first layer of a scene is its main source.
        if index == 0 {
            self.rebuild_scene();
            return;
        }
        let layer = scene.layers[index].clone();
        let slot = Arc::new(FrameSlot::new());
        match self.start_overlay_source(&source, &slot) {
            Some(running) => self.scenes.overlays.push(Overlay {
                running,
                slot,
                layer,
                index,
            }),
            // Kept in the scene, so it comes back when the source does.
            None => self.emit(AppEvent::SceneMissing),
        }
        self.sync_compositor();
        self.set_capture_exclusion(self.wants_exclusion());
    }
}
