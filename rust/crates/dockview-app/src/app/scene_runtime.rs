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

use dockview_domain::frame::FrameSlot;
use dockview_domain::scene::{NormRect, Scene, SceneLayer, SourceRef};
use dockview_domain::sources::Layer;
use dockview_domain::sources::{CaptureOptions, CaptureStatus};
use dockview_domain::sources::{CaptureTarget, ScreenOptions, ScreenStatus};
use dockview_platform::capture::VideoCapture;
use dockview_platform::compose::SceneCompositor;
use dockview_platform::screen::{list_monitors, list_windows, ScreenCapture};

use crate::cli::PipSpec;
use dockview_domain::i18n;
use dockview_ui::toast::ToastKind;

use super::{App, SourceKind};
use crate::sources::{find_window, Reconnect};

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
        reconnect: Reconnect,
    },
    Screen(ScreenCapture),
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

impl App {
    /// The layers to draw, bottom first: the base, then the overlays.
    fn scene_layers(&self, base: &SceneLayer) -> Vec<Layer> {
        let mut layers = vec![Layer {
            slot: Arc::clone(&self.input_slot),
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
        find_window(&self.windows, title, std::process::id()).map(|w| (w.handle, w.process_id))
    }

    /// Starts the source of an overlay, publishing to `slot`.
    pub(crate) fn start_overlay_source(
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
                    reconnect: Reconnect::new(),
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
        self.transition = None;
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
        for (index, layer) in scene.layers.iter().enumerate().skip(1) {
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
            self.toast(i18n::Key::ToastSceneMissing, ToastKind::Error);
        }
        self.scene_base = Some(base);
        self.sync_compositor();
        self.set_capture_exclusion(self.wants_exclusion());
    }

    /// Makes the compositor match the layers: none for a single full-picture
    /// layer (the main source feeds the viewer directly), else running with
    /// the current layers.
    pub fn sync_compositor(&mut self) {
        // A scene change in progress feeds the compositor by itself.
        if self.transition.is_some() {
            return;
        }
        let Some(base) = self.scene_base.clone() else {
            return;
        };
        let plain = self.overlays.is_empty() && base.rect == NormRect::FULL && base.visible;
        if plain {
            if self.scene.take().is_some() {
                self.input_slot.set_forward(Some(Arc::clone(&self.slot)));
            }
            return;
        }
        let layers = self.scene_layers(&base);
        match &self.scene {
            Some(scene) => scene.set_layers(layers),
            None => {
                self.input_slot.set_forward(None);
                self.scene = Some(SceneCompositor::start(
                    layers,
                    Arc::clone(&self.slot),
                    SCENE_WIDTH,
                    SCENE_HEIGHT,
                ));
            }
        }
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
                self.toast(i18n::Key::ToastSceneMissing, ToastKind::Error);
            }
        }
        self.morph_to(&scene, old);
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
                    reconnect,
                } => {
                    let gone = matches!(
                        capture.status(),
                        CaptureStatus::Disconnected { .. } | CaptureStatus::Failed(_)
                    );
                    if reconnect.tick(gone, Instant::now()) {
                        *capture = VideoCapture::start(
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
        if lost.is_empty() {
            return;
        }
        for index in lost.into_iter().rev() {
            self.overlays.remove(index);
        }
        self.sync_compositor();
        self.set_capture_exclusion(self.wants_exclusion());
        self.toast(i18n::Key::ToastPipLost, ToastKind::Error);
    }

    /// Whether Dockview's own window must stay out of the captured picture:
    /// whenever any layer is a screen.
    pub fn wants_exclusion(&self) -> bool {
        self.source_kind != SourceKind::Capture
            || self.leaving_has_screen()
            || self
                .overlays
                .iter()
                .any(|o| matches!(o.running, OverlayRunning::Screen(_)))
    }
}
