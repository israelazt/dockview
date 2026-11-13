//! The pictures drawn over the main source: starting a layer's source, keeping
//! it alive, and the corner picture asked for on the command line.

use dockview_domain::scene::{NormRect, Scene, SourceRef};

use super::scene_runtime::PIP_DEFAULT_SIZE;
use super::*;
use crate::cli::PipSpec;
use crate::scenes_service::OverlayRunning;

impl Core {
    /// Starts the source of an overlay, publishing to `slot`.
    pub(crate) fn start_overlay_source(
        &mut self,
        source: &SourceRef,
        slot: &Arc<FrameSlot>,
    ) -> Option<OverlayRunning> {
        match source {
            SourceRef::Capture { symbolic_link } => {
                // The device the main source already has open cannot be opened twice.
                let in_use = self.sources.source_kind == SourceKind::Capture
                    && self
                        .sources
                        .video_index
                        .and_then(|i| self.sources.video_devices.get(i))
                        .is_some_and(|d| &d.symbolic_link == symbolic_link);
                if in_use
                    || !self
                        .sources
                        .video_devices
                        .iter()
                        .any(|d| &d.symbolic_link == symbolic_link)
                {
                    return None;
                }
                Some(OverlayRunning::Capture {
                    capture: self.sources.machine.start_capture(
                        symbolic_link.clone(),
                        CaptureOptions::default(),
                        Arc::clone(slot),
                    ),
                    link: symbolic_link.clone(),
                    reconnect: Reconnect::new(),
                })
            }
            SourceRef::Monitor { device_name } => {
                self.refresh_sources();
                let monitor = self
                    .sources
                    .monitors
                    .iter()
                    .find(|m| &m.device_name == device_name)?;
                Some(OverlayRunning::Screen(self.sources.machine.start_screen(
                    CaptureTarget::Monitor(monitor.handle),
                    ScreenOptions::default(),
                    Arc::clone(slot),
                )))
            }
            SourceRef::Window { title } => {
                self.refresh_sources();
                let (handle, _) = self.sources.find_window(title)?;
                Some(OverlayRunning::Screen(self.sources.machine.start_screen(
                    CaptureTarget::Window(handle),
                    ScreenOptions::default(),
                    Arc::clone(slot),
                )))
            }
        }
    }

    /// Starts the scene asked for on the command line: the main source with a
    /// second one in the bottom-right corner. It is not saved.
    pub fn start_cli_scene(&mut self) {
        let Some(spec) = self.cli.pip.clone() else {
            return;
        };
        if self.cli.ui_preview {
            return;
        }
        self.refresh_sources();
        let base = if self.cli.test_pattern {
            // The synthetic picture has no device; the base is never looked up.
            SourceRef::Capture {
                symbolic_link: "test-pattern".into(),
            }
        } else if let Some(base) = self.sources.main_source_ref() {
            base
        } else {
            dockview_domain::log_warn!("--pip needs a main source to draw over");
            return;
        };
        let source = match &spec {
            PipSpec::Video(text) => {
                let wanted = text.to_lowercase();
                self.sources
                    .video_devices
                    .iter()
                    .find(|d| d.friendly_name.to_lowercase().contains(&wanted))
                    .map(|d| SourceRef::Capture {
                        symbolic_link: d.symbolic_link.clone(),
                    })
            }
            PipSpec::Monitor(n) => self
                .sources
                .machine
                .list_monitors()
                .into_iter()
                .nth(*n)
                .map(|m| SourceRef::Monitor {
                    device_name: m.device_name,
                }),
            PipSpec::Window(text) => {
                let wanted = text.to_lowercase();
                let own = std::process::id();
                self.sources
                    .machine
                    .list_windows()
                    .into_iter()
                    .find(|w| w.process_id != own && w.title.to_lowercase().contains(&wanted))
                    .map(|w| SourceRef::Window { title: w.title })
            }
        };
        let Some(source) = source else {
            dockview_domain::log_warn!("could not find the corner source {spec:?}");
            return;
        };
        let mut scene = Scene::with_base("--pip", base);
        scene.add_layer(source);
        if let (Some(layer), Some(w)) = (scene.layers.get_mut(1), self.cli.pip_size) {
            // 16:9 like the picture, so the height is the same fraction.
            layer.rect = NormRect::new(1.0 - w - 0.02, 1.0 - w - 0.035, w, w);
        } else if let Some(layer) = scene.layers.get_mut(1) {
            let w = PIP_DEFAULT_SIZE;
            layer.rect = NormRect::new(1.0 - w - 0.02, 1.0 - w - 0.035, w, w);
        }
        self.run_scene(&scene);
    }

    /// Keeps the overlays alive: a capture card that drops out is reopened,
    /// and a screen source that ends is taken off the picture.
    pub fn maintain_overlays(&mut self, now: Instant) {
        if self.scenes.maintain(now, &*self.sources.machine) {
            self.sync_compositor();
            self.set_capture_exclusion(self.wants_exclusion());
            self.emit(AppEvent::PipLost);
        }
    }

    /// Whether Dockview's own window must stay out of the captured picture:
    /// whenever any layer is a screen.
    pub fn wants_exclusion(&self) -> bool {
        self.sources.source_kind != SourceKind::Capture
            || self.scenes.leaving_has_screen()
            || self.scenes.has_screen_overlay()
    }
}
