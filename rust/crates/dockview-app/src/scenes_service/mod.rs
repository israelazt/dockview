//! The scene that is running: the layers drawn over the main source (the
//! overlays, each with a source of its own), the compositor that draws them
//! together, and the change from one scene to another.
//!
//! The first layer of a scene is the *base*: it is the main source, which the
//! sources service runs and whose sound is recorded. The other layers are
//! *overlays*. The main source always publishes to the *input* slot. Without a
//! scene that slot forwards straight to the *output* slot, which the viewer, the
//! replay and the recording read; with one it keeps the frames for the
//! compositor, which draws into the output slot. Switching between the two never
//! restarts anything.
//!
//! The service owns that state and keeps it alive. Starting the source of a
//! layer needs the devices, the settings and the events, so `Core` does that and
//! hands the service the result.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dockview_domain::frame::FrameSlot;
use dockview_domain::ports::{CaptureHandle, SceneCompositor, ScreenHandle, SourceMachine};
use dockview_domain::scene::{NormRect, Scene, SceneLayer, SourceRef};
use dockview_domain::sources::{CaptureOptions, CaptureStatus, Layer, ScreenStatus};
use dockview_domain::transition::{Easing, Transition, DEFAULT_DURATION};

use crate::sources::Reconnect;

#[cfg(test)]
mod tests;

/// The size of the picture a scene is drawn at.
pub const SCENE_WIDTH: u32 = 1920;
pub const SCENE_HEIGHT: u32 = 1080;

/// What the service needs from the rest of the application to run the
/// compositor.
pub struct SceneEnv<'a> {
    /// What the compositor is started on.
    pub machine: &'a dyn SourceMachine,
    /// Where the main source publishes.
    pub input_slot: &'a Arc<FrameSlot>,
    /// Where the finished picture goes (what the viewer and the recorder read).
    pub slot: &'a Arc<FrameSlot>,
}

/// What produces an overlay's pictures.
pub enum OverlayRunning {
    Capture {
        capture: Box<dyn CaptureHandle>,
        link: String,
        reconnect: Reconnect,
    },
    Screen(Box<dyn ScreenHandle>),
}

/// A layer above the base, with its running source.
pub struct Overlay {
    pub running: OverlayRunning,
    /// Where the source publishes; the compositor reads it.
    pub slot: Arc<FrameSlot>,
    pub layer: SceneLayer,
    /// The position of this layer in the saved scene.
    pub index: usize,
}

/// A scene change in progress.
pub struct ActiveTransition {
    transition: Transition,
    started: Instant,
    /// Pictures asked of the compositor so far (printed with `--stats`).
    ticks: u32,
    /// Overlays of the old scene that are not in the new one: drawn fading out,
    /// then dropped.
    leaving: Vec<Overlay>,
}

#[derive(Default)]
pub struct ScenesService {
    /// Draws the layers of the scene; `Some` only while it is needed.
    pub(crate) compositor: Option<Box<dyn SceneCompositor>>,
    /// The layers above the base of the active scene, with their sources.
    pub(crate) overlays: Vec<Overlay>,
    /// The base layer of the active scene (its source is the main source).
    pub(crate) base: Option<SceneLayer>,
    /// A change from one scene to another in progress.
    pub(crate) transition: Option<ActiveTransition>,
}

impl ScenesService {
    pub fn new() -> Self {
        Self::default()
    }

    // -- What is on screen -------------------------------------------------

    /// The layers to draw, bottom first: the base (the main source, which
    /// publishes to `input_slot`), then the overlays.
    pub fn layers(&self, base: &SceneLayer, input_slot: &Arc<FrameSlot>) -> Vec<Layer> {
        let mut layers = vec![Layer {
            slot: Arc::clone(input_slot),
            rect: base.rect,
            fit: base.fit,
            visible: base.visible,
            opacity: 1.0,
        }];
        layers.extend(self.overlays.iter().map(|o| Layer {
            slot: Arc::clone(&o.slot),
            rect: o.layer.rect,
            fit: o.layer.fit,
            visible: o.layer.visible,
            opacity: 1.0,
        }));
        layers
    }

    /// The scene as it is on screen now (the base and the overlays that run),
    /// or `None` when there is no base. `main` is the main source, used as the
    /// base when no scene is running.
    pub fn running_scene(&self, main: Option<SourceRef>) -> Option<Scene> {
        let base = match &self.base {
            Some(base) => base.clone(),
            None => SceneLayer::new(main?, NormRect::FULL),
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

    /// Whether any overlay shows a screen or a window.
    pub fn has_screen_overlay(&self) -> bool {
        self.overlays
            .iter()
            .any(|o| matches!(o.running, OverlayRunning::Screen(_)))
    }

    /// Whether the sources of the old scene still run, and one is a screen.
    pub fn leaving_has_screen(&self) -> bool {
        self.transition.as_ref().is_some_and(|t| {
            t.leaving
                .iter()
                .any(|o| matches!(o.running, OverlayRunning::Screen(_)))
        })
    }

    // -- Running a scene ---------------------------------------------------

    /// Stops the scene: drops the compositor and the overlays, and lets the
    /// main source feed the viewer directly again.
    pub fn stop(&mut self, env: &SceneEnv) {
        self.transition = None;
        self.compositor = None;
        self.overlays.clear();
        self.base = None;
        env.input_slot.set_forward(Some(Arc::clone(env.slot)));
    }

    /// Makes the compositor match the layers: none for a single full-picture
    /// layer (the main source feeds the viewer directly), else running with
    /// the current layers.
    pub fn sync_compositor(&mut self, env: &SceneEnv) {
        // A scene change in progress feeds the compositor by itself.
        if self.transition.is_some() {
            return;
        }
        let Some(base) = self.base.clone() else {
            return;
        };
        let plain = self.overlays.is_empty() && base.rect == NormRect::FULL && base.visible;
        if plain {
            if self.compositor.take().is_some() {
                env.input_slot.set_forward(Some(Arc::clone(env.slot)));
            }
            return;
        }
        let layers = self.layers(&base, env.input_slot);
        self.feed(layers, env);
    }

    /// Gives the compositor `layers`, starting it if it was not running.
    fn feed(&mut self, layers: Vec<Layer>, env: &SceneEnv) {
        match &self.compositor {
            Some(compositor) => compositor.set_layers(layers),
            None => {
                env.input_slot.set_forward(None);
                self.compositor = Some(env.machine.start_compositor(
                    layers,
                    Arc::clone(env.slot),
                    SCENE_WIDTH,
                    SCENE_HEIGHT,
                ));
            }
        }
    }

    // -- Editing the running scene -----------------------------------------

    /// Shows or hides the layer at `index` (0 is the base).
    pub fn set_layer_visible(&mut self, index: usize, visible: bool) {
        if index == 0 {
            if let Some(base) = self.base.as_mut() {
                base.visible = visible;
            }
        } else if let Some(overlay) = self.overlays.iter_mut().find(|o| o.index == index) {
            overlay.layer.visible = visible;
        }
    }

    /// Drops the overlay at `index` and renumbers the ones after it.
    pub fn remove_overlay(&mut self, index: usize) {
        self.overlays.retain(|o| o.index != index);
        for overlay in &mut self.overlays {
            if overlay.index > index {
                overlay.index -= 1;
            }
        }
    }

    /// Keeps the overlays alive: a capture card that drops out is reopened
    /// (on `machine`) after a pause, and a screen source that ended is taken
    /// off. Returns whether any was taken off.
    pub fn maintain(&mut self, now: Instant, machine: &dyn SourceMachine) -> bool {
        let mut lost = Vec::new();
        for (index, overlay) in self.overlays.iter_mut().enumerate() {
            match &mut overlay.running {
                OverlayRunning::Capture {
                    capture,
                    link,
                    reconnect,
                } => {
                    let gone = matches!(
                        capture.status(),
                        CaptureStatus::Disconnected { .. } | CaptureStatus::Failed(_)
                    );
                    if reconnect.tick(gone, now) {
                        *capture = machine.start_capture(
                            link.clone(),
                            CaptureOptions::default(),
                            Arc::clone(&overlay.slot),
                        );
                    }
                }
                OverlayRunning::Screen(capture) => {
                    if matches!(
                        capture.status(),
                        ScreenStatus::Ended(_) | ScreenStatus::Failed(_)
                    ) {
                        lost.push(index);
                    }
                }
            }
        }
        for index in lost.iter().rev() {
            self.overlays.remove(*index);
        }
        !lost.is_empty()
    }

    // -- Changing from one scene to another --------------------------------

    /// Starts the change from `old` to `new`. `leaving` are the overlays of the
    /// old scene that the new one does not have: they fade out and are dropped
    /// when the change ends.
    pub fn begin_transition(&mut self, old: &Scene, new: &Scene, leaving: Vec<Overlay>) {
        self.transition = Some(ActiveTransition {
            transition: Transition::new(old, new, DEFAULT_DURATION, Easing::EaseInOut),
            started: Instant::now(),
            ticks: 0,
            leaving,
        });
    }

    /// Moves the change along (the compositor gets the layers of this moment);
    /// call it on every turn of the event loop. Returns `true` on the turn the
    /// change ended: the sources that left are dropped and the compositor is
    /// made to match the new scene. With `log`, says how smooth it was.
    pub fn tick_transition(&mut self, env: &SceneEnv, log: bool) -> bool {
        let Some(mut active) = self.transition.take() else {
            return false;
        };
        let elapsed = active.started.elapsed();
        if active.transition.is_done(elapsed) {
            if log {
                dockview_domain::log_info!(
                    "scene change: {} pictures in {} ms",
                    active.ticks,
                    elapsed.as_millis()
                );
            }
            // Dropping `active` stops the sources that left.
            drop(active);
            self.sync_compositor(env);
            return true;
        }
        active.ticks += 1;
        let layers = self.transition_layers(&active, elapsed, env.input_slot);
        self.feed(layers, env);
        self.transition = Some(active);
        false
    }

    /// Ends a change in progress at once, showing the target scene. Returns
    /// whether there was one.
    pub fn finish_transition(&mut self, env: &SceneEnv) -> bool {
        if self.transition.take().is_none() {
            return false;
        }
        self.sync_compositor(env);
        true
    }

    /// The compositor layers `elapsed` into the transition.
    fn transition_layers(
        &self,
        active: &ActiveTransition,
        elapsed: Duration,
        input_slot: &Arc<FrameSlot>,
    ) -> Vec<Layer> {
        let Some(base) = &self.base else {
            return Vec::new();
        };
        let mut base_taken = false;
        let mut taken_overlays = vec![false; self.overlays.len()];
        let mut taken_leaving = vec![false; active.leaving.len()];
        let mut layers = Vec::new();
        for draw in active.transition.sample(elapsed) {
            let slot = if !base_taken && draw.source == base.source {
                base_taken = true;
                Some(Arc::clone(input_slot))
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
}
