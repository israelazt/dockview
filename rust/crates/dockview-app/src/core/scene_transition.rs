//! The change from one scene to another while the app runs.
//!
//! Sources both scenes use keep running and glide to their new place; the ones
//! only the new scene has are started first and fade in; the ones only the old
//! scene had stay alive until the change is over and fade out. The maths is in
//! `dockview_domain::transition`; this module owns the sources and feeds the
//! compositor on every turn of the event loop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dockview_domain::events::AppEvent;
use dockview_domain::frame::FrameSlot;
use dockview_domain::scene::{Scene, SceneLayer};
use dockview_domain::sources::Layer;
use dockview_domain::transition::{Easing, Transition, DEFAULT_DURATION};

use super::scene_runtime::{Overlay, SCENE_HEIGHT, SCENE_WIDTH};
use super::Core;

/// A scene change in progress.
pub struct ActiveTransition {
    transition: Transition,
    started: Instant,
    /// Pictures asked of the compositor so far (printed with `--show-stats`).
    ticks: u32,
    /// Overlays of the old scene that are not in the new one: drawn fading out,
    /// then dropped.
    pub leaving: Vec<Overlay>,
}

impl Core {
    /// The scene as it is on screen now (the base and the overlays that run),
    /// or `None` when there is no main source to start from.
    pub fn running_scene(&self) -> Option<Scene> {
        let base = match &self.scene_base {
            Some(base) => base.clone(),
            None => SceneLayer::new(
                self.main_source_ref()?,
                dockview_domain::scene::NormRect::FULL,
            ),
        };
        let mut overlays: Vec<&Overlay> = self.overlays.iter().collect();
        overlays.sort_by_key(|o| o.index);
        let mut layers = vec![base];
        layers.extend(overlays.into_iter().map(|o| o.layer.clone()));
        Some(Scene {
            name: String::new(),
            layers,
        })
    }

    /// Goes to `scene` with a transition from `old` (what was on screen before
    /// the main source was switched). The base must already be the main source.
    pub(crate) fn morph_to(&mut self, scene: &Scene, old: Option<Scene>) {
        let Some(base) = scene.layers.first().cloned() else {
            return;
        };
        let mut previous = std::mem::take(&mut self.overlays);
        let mut missing = 0;
        for (index, layer) in scene.layers.iter().enumerate().skip(1) {
            // A source that is already running is kept, not reopened.
            if let Some(at) = previous.iter().position(|o| o.layer.source == layer.source) {
                let mut kept = previous.remove(at);
                kept.layer = layer.clone();
                kept.index = index;
                self.overlays.push(kept);
                continue;
            }
            let slot = Arc::new(FrameSlot::new());
            match self.start_overlay_source(&layer.source, &slot) {
                Some(running) => self.overlays.push(Overlay {
                    running,
                    slot,
                    layer: layer.clone(),
                    index,
                }),
                None => missing += 1,
            }
        }
        if missing > 0 {
            eprintln!("{missing} source(s) of the scene could not be started");
            self.emit(AppEvent::SceneMissing);
        }
        self.scene_base = Some(base);

        match old {
            Some(old) => {
                self.transition = Some(ActiveTransition {
                    transition: Transition::new(&old, scene, DEFAULT_DURATION, Easing::EaseInOut),
                    started: Instant::now(),
                    ticks: 0,
                    leaving: previous,
                });
                self.tick_transition();
            }
            // Nothing to come from: just show the scene.
            None => self.sync_compositor(),
        }
        self.set_capture_exclusion(self.wants_exclusion());
    }

    /// The compositor layers `elapsed` into the transition.
    fn transition_layers(&self, active: &ActiveTransition, elapsed: Duration) -> Vec<Layer> {
        let Some(base) = &self.scene_base else {
            return Vec::new();
        };
        let mut base_taken = false;
        let mut taken_overlays = vec![false; self.overlays.len()];
        let mut taken_leaving = vec![false; active.leaving.len()];
        let mut layers = Vec::new();
        for draw in active.transition.sample(elapsed) {
            let slot = if !base_taken && draw.source == base.source {
                base_taken = true;
                Some(Arc::clone(&self.input_slot))
            } else if let Some(at) = self
                .overlays
                .iter()
                .enumerate()
                .position(|(i, o)| !taken_overlays[i] && o.layer.source == draw.source)
            {
                taken_overlays[at] = true;
                Some(Arc::clone(&self.overlays[at].slot))
            } else {
                active
                    .leaving
                    .iter()
                    .enumerate()
                    .position(|(i, o)| !taken_leaving[i] && o.layer.source == draw.source)
                    .map(|at| {
                        taken_leaving[at] = true;
                        Arc::clone(&active.leaving[at].slot)
                    })
            };
            // A source that could not be started has nothing to draw.
            if let Some(slot) = slot {
                layers.push(Layer {
                    slot,
                    rect: draw.rect,
                    fit: draw.fit,
                    visible: true,
                    opacity: draw.opacity,
                });
            }
        }
        layers
    }

    /// Moves the change along; called on every turn of the event loop.
    pub fn tick_transition(&mut self) {
        let Some(mut active) = self.transition.take() else {
            return;
        };
        let elapsed = active.started.elapsed();
        if active.transition.is_done(elapsed) {
            if self.cli.show_stats {
                eprintln!(
                    "scene change: {} pictures in {} ms",
                    active.ticks,
                    elapsed.as_millis()
                );
            }
            // Dropping `active` stops the sources that left.
            drop(active);
            self.sync_compositor();
            self.set_capture_exclusion(self.wants_exclusion());
            return;
        }
        active.ticks += 1;
        let layers = self.transition_layers(&active, elapsed);
        match &self.scene {
            Some(scene) => scene.set_layers(layers),
            None => {
                self.input_slot.set_forward(None);
                self.scene = Some(self.machine.start_compositor(
                    layers,
                    Arc::clone(&self.slot),
                    SCENE_WIDTH,
                    SCENE_HEIGHT,
                ));
            }
        }
        self.transition = Some(active);
    }

    /// Ends a change in progress at once, showing the target scene.
    pub fn finish_transition(&mut self) {
        if self.transition.take().is_some() {
            self.sync_compositor();
            self.set_capture_exclusion(self.wants_exclusion());
        }
    }

    /// Whether the sources of the old scene still run.
    pub fn leaving_has_screen(&self) -> bool {
        self.transition.as_ref().is_some_and(|t| {
            t.leaving
                .iter()
                .any(|o| matches!(o.running, super::scene_runtime::OverlayRunning::Screen(_)))
        })
    }
}
