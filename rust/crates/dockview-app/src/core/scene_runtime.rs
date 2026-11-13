//! Scenes while the app runs: starting the sources of the active scene and
//! handing them to the scenes service, which keeps them and feeds the
//! compositor (see `scenes_service` for how the pictures flow).

use std::sync::Arc;

use dockview_domain::events::AppEvent;
use dockview_domain::frame::FrameSlot;
use dockview_domain::scene::{Scene, SourceRef};
use dockview_domain::sources::CaptureTarget;

use super::{Core, SourceKind};
use crate::scenes_service::Overlay;

/// Width of the corner picture asked for on the command line, unless given.
pub(super) const PIP_DEFAULT_SIZE: f32 = 0.28;

impl Core {
    /// Makes `source` the main source, as if the user had picked it. Returns
    /// whether it was found.
    pub(super) fn make_main(&mut self, source: &SourceRef) -> bool {
        match source {
            SourceRef::Capture { symbolic_link } => {
                let Some(index) = self
                    .sources
                    .video_devices
                    .iter()
                    .position(|d| &d.symbolic_link == symbolic_link)
                else {
                    return false;
                };
                if self.sources.source_kind != SourceKind::Capture
                    || self.sources.video_index != Some(index)
                {
                    self.select_video(Some(index));
                    if let Some(audio) = self.sources.match_audio(Some(index)) {
                        self.select_audio(Some(audio));
                    }
                }
                true
            }
            SourceRef::Monitor { device_name } => {
                self.refresh_sources();
                let Some(handle) = self
                    .sources
                    .monitors
                    .iter()
                    .find(|m| &m.device_name == device_name)
                    .map(|m| m.handle)
                else {
                    return false;
                };
                if self.sources.source_kind != SourceKind::Monitor(handle) {
                    self.start_screen(CaptureTarget::Monitor(handle), SourceKind::Monitor(handle));
                }
                true
            }
            SourceRef::Window { title } => {
                self.refresh_sources();
                let Some(window) = self.sources.find_window(title) else {
                    return false;
                };
                let kind = SourceKind::Window {
                    handle: window.0,
                    pid: window.1,
                };
                if self.sources.source_kind != kind {
                    self.start_screen(CaptureTarget::Window(window.0), kind);
                }
                true
            }
        }
    }

    /// Stops the scene: drops the compositor and the overlays, and lets the
    /// main source feed the viewer directly again.
    pub(super) fn stop_scene(&mut self) {
        let env = scene_env!(self);
        self.scenes.stop(&env);
    }

    /// Runs `scene`: starts its overlays and the compositor. The main source
    /// is left as it is (see [`Core::activate_scene`] to change it too).
    pub(super) fn run_scene(&mut self, scene: &Scene) {
        self.stop_scene();
        let Some(base) = scene.layers.first().cloned() else {
            return;
        };
        let mut missing = 0;
        for (index, layer) in scene.layers.iter().enumerate().skip(1) {
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
        self.sync_compositor();
        self.set_capture_exclusion(self.wants_exclusion());
    }

    /// Makes the compositor match the layers: none for a single full-picture
    /// layer (the main source feeds the viewer directly), else running with
    /// the current layers.
    pub fn sync_compositor(&mut self) {
        let env = scene_env!(self);
        self.scenes.sync_compositor(&env);
    }

    /// Switches to scene `index` (or, with `None`, to the plain main source).
    pub fn activate_scene(&mut self, index: Option<usize>) {
        self.finish_transition();
        // What is on screen now, before the main source is switched.
        let old = self.running_scene();
        let scene = index.and_then(|i| self.settings.scenes.get(i).cloned());
        self.settings.active_scene = index.filter(|_| scene.is_some());
        let Some(scene) = scene else {
            self.stop_scene();
            self.set_capture_exclusion(self.wants_exclusion());
            return;
        };
        if let Some(base) = scene.base() {
            if !self.make_main(base) {
                self.emit(AppEvent::SceneMissing);
            }
        }
        self.morph_to(&scene, old);
    }
}
