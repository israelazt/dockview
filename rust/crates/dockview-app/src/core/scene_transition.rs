//! The change from one scene to another while the app runs.
//!
//! Sources both scenes use keep running and glide to their new place; the ones
//! only the new scene has are started first and fade in; the ones only the old
//! scene had stay alive until the change is over and fade out. The maths is in
//! `dockview_domain::transition`; the scenes service owns the sources and feeds
//! the compositor on every turn of the event loop. Here is what needs the rest
//! of the application: which sources to start and keep.

use std::sync::Arc;

use dockview_domain::events::AppEvent;
use dockview_domain::frame::FrameSlot;
use dockview_domain::scene::Scene;

use super::Core;
use crate::scenes_service::Overlay;

impl Core {
    /// The scene as it is on screen now (the base and the overlays that run),
    /// or `None` when there is no main source to start from.
    pub fn running_scene(&self) -> Option<Scene> {
        self.scenes.running_scene(self.sources.main_source_ref())
    }

    /// Goes to `scene` with a transition from `old` (what was on screen before
    /// the main source was switched). The base must already be the main source.
    pub(crate) fn morph_to(&mut self, scene: &Scene, old: Option<Scene>) {
        let Some(base) = scene.layers.first().cloned() else {
            return;
        };
        let mut previous = std::mem::take(&mut self.scenes.overlays);
        let mut missing = 0;
        for (index, layer) in scene.layers.iter().enumerate().skip(1) {
            // A source that is already running is kept, not reopened.
            if let Some(at) = previous.iter().position(|o| o.layer.source == layer.source) {
                let mut kept = previous.remove(at);
                kept.layer = layer.clone();
                kept.index = index;
                self.scenes.overlays.push(kept);
                continue;
            }
            let slot = Arc::new(FrameSlot::new());
            match self.start_overlay_source(&layer.source, &slot) {
                Some(running) => self.scenes.overlays.push(Overlay {
                    running,
                    slot,
                    layer: layer.clone(),
                    index,
                }),
                None => missing += 1,
            }
        }
        if missing > 0 {
            dockview_domain::log_warn!("{missing} source(s) of the scene could not be started");
            self.emit(AppEvent::SceneMissing);
        }
        self.scenes.base = Some(base);

        match old {
            Some(old) => {
                self.scenes.begin_transition(&old, scene, previous);
                self.tick_transition();
            }
            // Nothing to come from: just show the scene.
            None => self.sync_compositor(),
        }
        self.set_capture_exclusion(self.wants_exclusion());
    }

    /// Moves the change along; called on every turn of the event loop.
    pub fn tick_transition(&mut self) {
        let env = scene_env!(self);
        if self.scenes.tick_transition(&env, self.cli.show_stats) {
            self.set_capture_exclusion(self.wants_exclusion());
        }
    }

    /// Ends a change in progress at once, showing the target scene.
    pub fn finish_transition(&mut self) {
        let env = scene_env!(self);
        if self.scenes.finish_transition(&env) {
            self.set_capture_exclusion(self.wants_exclusion());
        }
    }

    /// Whether the sources of the old scene still run.
    pub fn leaving_has_screen(&self) -> bool {
        self.scenes.leaving_has_screen()
    }
}
