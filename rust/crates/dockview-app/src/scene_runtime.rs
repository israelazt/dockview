//! Scenes while the app runs: starts the sources of the active scene, keeps
//! them alive and feeds them to the compositor.
//!
//! The first layer of a scene is the *base*: it is the main source (the one
//! `App::source` runs, whose sound is recorded). The other layers are *overlays*
//! with sources of their own, drawn over it.
//!
//! The main source always publishes to `App::input_slot`. Without a scene that
//! slot forwards straight to `App::slot`, which the viewer, the replay and the
//! recording read; with one it keeps the frames for the compositor, which draws
//! into `App::slot`. Switching between the two never restarts anything.

use std::sync::Arc;
use std::time::Instant;

use dockview_core::capture::{CaptureOptions, CaptureStatus, VideoCapture};
use dockview_core::compose::{Layer, SceneCompositor};
use dockview_core::frame::FrameSlot;
use dockview_core::scene::{NormRect, Scene, SceneLayer, SourceRef};
use dockview_core::screen::{
    list_monitors, list_windows, CaptureTarget, ScreenCapture, ScreenOptions, ScreenStatus,
};

use crate::cli::PipSpec;
use crate::ui::toast::ToastKind;
use crate::{i18n, App, SourceKind, RECONNECT_DELAY};

/// The size of the picture a scene is drawn at.
pub const SCENE_WIDTH: u32 = 1920;
pub const SCENE_HEIGHT: u32 = 1080;
/// Width of the corner picture asked for on the command line, unless given.
const PIP_DEFAULT_SIZE: f32 = 0.28;

/// What produces an overlay's pictures.
pub enum OverlayRunning {
    Capture {
        capture: VideoCapture,
        link: String,
        retry_at: Option<Instant>,
    },
    Screen(ScreenCapture),
}

/// A layer above the base, with its running source.
pub struct Overlay {
    pub running: OverlayRunning,
    /// Where the source publishes; the compositor reads it.
    pub slot: Arc<FrameSlot>,
    pub layer: SceneLayer,
}

impl App {
    /// The layers to draw, bottom first: the base, then the overlays.
    fn scene_layers(&self, base: &SceneLayer) -> Vec<Layer> {
        let mut layers = vec![Layer {
            slot: Arc::clone(&self.input_slot),
            rect: base.rect,
            fit: base.fit,
            visible: base.visible,
        }];
        layers.extend(self.overlays.iter().map(|o| Layer {
            slot: Arc::clone(&o.slot),
            rect: o.layer.rect,
            fit: o.layer.fit,
            visible: o.layer.visible,
        }));
        layers
    }

    /// The main source as something that can be saved in a scene.
    pub fn main_source_ref(&self) -> Option<SourceRef> {
        match self.source_kind {
            SourceKind::Capture => self
                .video_index
                .and_then(|i| self.video_devices.get(i))
                .map(|d| SourceRef::Capture {
                    symbolic_link: d.symbolic_link.clone(),
                }),
            SourceKind::Monitor(handle) => {
                self.monitors
                    .iter()
                    .find(|m| m.handle == handle)
                    .map(|m| SourceRef::Monitor {
                        device_name: m.device_name.clone(),
                    })
            }
            SourceKind::Window { handle, .. } => self
                .windows
                .iter()
                .find(|w| w.handle == handle)
                .map(|w| SourceRef::Window {
                    title: w.title.clone(),
                }),
        }
    }

    /// Makes `source` the main source, as if the user had picked it. Returns
    /// whether it was found.
    fn make_main(&mut self, source: &SourceRef) -> bool {
        match source {
            SourceRef::Capture { symbolic_link } => {
                let Some(index) = self
                    .video_devices
                    .iter()
                    .position(|d| &d.symbolic_link == symbolic_link)
                else {
                    return false;
                };
                if self.source_kind != SourceKind::Capture || self.video_index != Some(index) {
                    self.select_video(Some(index));
                    if let Some(audio) = self.match_audio(Some(index)) {
                        self.select_audio(Some(audio));
                    }
                }
                true
            }
            SourceRef::Monitor { device_name } => {
                self.refresh_sources();
                let Some(handle) = self
                    .monitors
                    .iter()
                    .find(|m| &m.device_name == device_name)
                    .map(|m| m.handle)
                else {
                    return false;
                };
                if self.source_kind != SourceKind::Monitor(handle) {
                    self.start_screen(CaptureTarget::Monitor(handle), SourceKind::Monitor(handle));
                }
                true
            }
            SourceRef::Window { title } => {
                self.refresh_sources();
                let Some(window) = self.find_window(title) else {
                    return false;
                };
                let kind = SourceKind::Window {
                    handle: window.0,
                    pid: window.1,
                };
                if self.source_kind != kind {
                    self.start_screen(CaptureTarget::Window(window.0), kind);
                }
                true
            }
        }
    }

    /// A window (its handle and process) by title: an exact match first,
    /// then one that contains the text. Never one of our own.
    fn find_window(&self, title: &str) -> Option<(isize, u32)> {
        let wanted = title.to_lowercase();
        let own = std::process::id();
        let others = || self.windows.iter().filter(|w| w.process_id != own);
        others()
            .find(|w| w.title.to_lowercase() == wanted)
            .or_else(|| others().find(|w| w.title.to_lowercase().contains(&wanted)))
            .map(|w| (w.handle, w.process_id))
    }

    /// Starts the source of an overlay, publishing to `slot`.
    fn start_overlay_source(
        &mut self,
        source: &SourceRef,
        slot: &Arc<FrameSlot>,
    ) -> Option<OverlayRunning> {
        match source {
            SourceRef::Capture { symbolic_link } => {
                // The device the main source already has open cannot be opened twice.
                let in_use = self.source_kind == SourceKind::Capture
                    && self
                        .video_index
                        .and_then(|i| self.video_devices.get(i))
                        .is_some_and(|d| &d.symbolic_link == symbolic_link);
                if in_use
                    || !self
                        .video_devices
                        .iter()
                        .any(|d| &d.symbolic_link == symbolic_link)
                {
                    return None;
                }
                Some(OverlayRunning::Capture {
                    capture: VideoCapture::start(
                        symbolic_link.clone(),
                        CaptureOptions::default(),
                        Arc::clone(slot),
                    ),
                    link: symbolic_link.clone(),
                    retry_at: None,
                })
            }
            SourceRef::Monitor { device_name } => {
                self.refresh_sources();
                let monitor = self
                    .monitors
                    .iter()
                    .find(|m| &m.device_name == device_name)?;
                Some(OverlayRunning::Screen(ScreenCapture::start(
                    CaptureTarget::Monitor(monitor.handle),
                    ScreenOptions::default(),
                    Arc::clone(slot),
                )))
            }
            SourceRef::Window { title } => {
                self.refresh_sources();
                let (handle, _) = self.find_window(title)?;
                Some(OverlayRunning::Screen(ScreenCapture::start(
                    CaptureTarget::Window(handle),
                    ScreenOptions::default(),
                    Arc::clone(slot),
                )))
            }
        }
    }

    /// Stops the scene: drops the compositor and the overlays, and lets the
    /// main source feed the viewer directly again.
    fn stop_scene(&mut self) {
        self.scene = None;
        self.overlays.clear();
        self.scene_base = None;
        self.input_slot.set_forward(Some(Arc::clone(&self.slot)));
    }

    /// Runs `scene`: starts its overlays and the compositor. The main source
    /// is left as it is (see [`App::activate_scene`] to change it too).
    fn run_scene(&mut self, scene: &Scene) {
        self.stop_scene();
        let Some(base) = scene.layers.first().cloned() else {
            return;
        };
        let mut missing = 0;
        for layer in scene.layers.iter().skip(1) {
            let slot = Arc::new(FrameSlot::new());
            match self.start_overlay_source(&layer.source, &slot) {
                Some(running) => self.overlays.push(Overlay {
                    running,
                    slot,
                    layer: layer.clone(),
                }),
                None => missing += 1,
            }
        }
        if missing > 0 {
            eprintln!("{missing} source(s) of the scene could not be started");
            self.toast(i18n::Key::ToastSceneMissing, ToastKind::Error);
        }
        // A base over the whole picture and no overlays needs no compositor.
        let plain = self.overlays.is_empty() && base.rect == NormRect::FULL && base.visible;
        if !plain {
            self.input_slot.set_forward(None);
            self.scene = Some(SceneCompositor::start(
                self.scene_layers(&base),
                Arc::clone(&self.slot),
                SCENE_WIDTH,
                SCENE_HEIGHT,
            ));
        }
        self.scene_base = Some(base);
        self.set_capture_exclusion(self.wants_exclusion());
    }

    /// Switches to scene `index` (or, with `None`, to the plain main source).
    pub fn activate_scene(&mut self, index: Option<usize>) {
        let scene = index.and_then(|i| self.settings.scenes.get(i).cloned());
        self.settings.active_scene = index.filter(|_| scene.is_some());
        let Some(scene) = scene else {
            self.stop_scene();
            self.set_capture_exclusion(self.wants_exclusion());
            return;
        };
        if let Some(base) = scene.base() {
            if !self.make_main(base) {
                self.toast(i18n::Key::ToastSceneMissing, ToastKind::Error);
            }
        }
        self.run_scene(&scene);
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
        } else if let Some(base) = self.main_source_ref() {
            base
        } else {
            eprintln!("--pip needs a main source to draw over");
            return;
        };
        let source = match &spec {
            PipSpec::Video(text) => {
                let wanted = text.to_lowercase();
                self.video_devices
                    .iter()
                    .find(|d| d.friendly_name.to_lowercase().contains(&wanted))
                    .map(|d| SourceRef::Capture {
                        symbolic_link: d.symbolic_link.clone(),
                    })
            }
            PipSpec::Monitor(n) => {
                list_monitors()
                    .into_iter()
                    .nth(*n)
                    .map(|m| SourceRef::Monitor {
                        device_name: m.device_name,
                    })
            }
            PipSpec::Window(text) => {
                let wanted = text.to_lowercase();
                let own = std::process::id();
                list_windows()
                    .into_iter()
                    .find(|w| w.process_id != own && w.title.to_lowercase().contains(&wanted))
                    .map(|w| SourceRef::Window { title: w.title })
            }
        };
        let Some(source) = source else {
            eprintln!("could not find the corner source {spec:?}");
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
    pub fn maintain_overlays(&mut self) {
        let mut lost = Vec::new();
        for (index, overlay) in self.overlays.iter_mut().enumerate() {
            match &mut overlay.running {
                OverlayRunning::Capture {
                    capture,
                    link,
                    retry_at,
                } => match capture.status() {
                    CaptureStatus::Disconnected { .. } | CaptureStatus::Failed(_) => {
                        let due = retry_at.unwrap_or_else(|| Instant::now() + RECONNECT_DELAY);
                        if Instant::now() >= due {
                            *capture = VideoCapture::start(
                                link.clone(),
                                CaptureOptions::default(),
                                Arc::clone(&overlay.slot),
                            );
                            *retry_at = None;
                        } else {
                            *retry_at = Some(due);
                        }
                    }
                    _ => *retry_at = None,
                },
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
        if lost.is_empty() {
            return;
        }
        for index in lost.into_iter().rev() {
            self.overlays.remove(index);
        }
        self.refresh_scene_layers();
        self.set_capture_exclusion(self.wants_exclusion());
        self.toast(i18n::Key::ToastPipLost, ToastKind::Error);
    }

    /// Hands the compositor the current layers (after an edit or a loss).
    pub fn refresh_scene_layers(&mut self) {
        let Some(base) = self.scene_base.clone() else {
            return;
        };
        let layers = self.scene_layers(&base);
        if let Some(scene) = &self.scene {
            scene.set_layers(layers);
        }
    }

    /// Whether Dockview's own window must stay out of the captured picture:
    /// whenever any layer is a screen.
    pub fn wants_exclusion(&self) -> bool {
        self.source_kind != SourceKind::Capture
            || self
                .overlays
                .iter()
                .any(|o| matches!(o.running, OverlayRunning::Screen(_)))
    }
}
