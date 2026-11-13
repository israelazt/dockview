//! What the scenes popover asks for: pick, make and delete scenes, and edit the
//! layers of the one in use. The saved scene in the settings is changed first,
//! then the running layers (see `scene_runtime`) are brought in line with it.

use std::sync::Arc;

use dockview_domain::device_names;
use dockview_domain::events::AppEvent;
use dockview_domain::frame::FrameSlot;
use dockview_domain::scene::{Scene, SourceRef, MAX_SCENE_LAYERS};

use dockview_domain::i18n;
use dockview_domain::i18n::LangModeExt;
use dockview_domain::view::{LayerView, SceneAction, ScenesView};

use super::scene_runtime::Overlay;
use super::Core;

/// Scenes a user may keep.
const MAX_SCENES: usize = 12;

impl Core {
    /// How a source is named in the layer list.
    fn source_label(&self, source: &SourceRef, lang: i18n::Lang) -> String {
        match source {
            SourceRef::Capture { symbolic_link } => {
                let friendly = self
                    .video_devices
                    .iter()
                    .find(|d| &d.symbolic_link == symbolic_link)
                    .map_or("…", |d| d.friendly_name.as_str());
                device_names::display_name(symbolic_link, friendly, &self.settings.device_aliases)
            }
            SourceRef::Monitor { device_name } => {
                match self
                    .monitors
                    .iter()
                    .position(|m| &m.device_name == device_name)
                {
                    Some(i) => format!("{} {}", i18n::tr(lang, i18n::Key::SourceScreenWord), i + 1),
                    None => device_name.clone(),
                }
            }
            SourceRef::Window { title } => title.clone(),
        }
    }

    /// What the scenes popover shows.
    pub fn scenes_view(&self, lang: i18n::Lang) -> ScenesView {
        let active = self.settings.active_scene;
        let layers: Vec<LayerView> = active
            .and_then(|i| self.settings.scenes.get(i))
            .map(|scene| {
                scene
                    .layers
                    .iter()
                    .map(|l| LayerView {
                        name: self.source_label(&l.source, lang),
                        visible: l.visible,
                    })
                    .collect()
            })
            .unwrap_or_default();
        ScenesView {
            names: self
                .settings
                .scenes
                .iter()
                .map(|s| s.name.clone())
                .collect(),
            active,
            can_add: active.is_some() && layers.len() < MAX_SCENE_LAYERS,
            layers,
        }
    }

    pub fn apply_scene_action(&mut self, action: SceneAction) {
        // Edits start from the finished picture, not from the middle of a change.
        self.finish_transition();
        match action {
            SceneAction::Activate(index) => self.activate_scene(index),
            SceneAction::New => self.new_scene(),
            SceneAction::Delete(index) => self.delete_scene(index),
            SceneAction::ToggleLayer(index) => self.toggle_layer(index),
            SceneAction::RemoveLayer(index) => self.remove_layer(index),
            SceneAction::AddCapture(i) => {
                let source = self.video_devices.get(i).map(|d| SourceRef::Capture {
                    symbolic_link: d.symbolic_link.clone(),
                });
                self.add_layer(source);
            }
            SceneAction::AddMonitor(i) => {
                let source = self.monitors.get(i).map(|m| SourceRef::Monitor {
                    device_name: m.device_name.clone(),
                });
                self.add_layer(source);
            }
            SceneAction::AddWindow(i) => {
                let source = self.windows.get(i).map(|w| SourceRef::Window {
                    title: w.title.clone(),
                });
                self.add_layer(source);
            }
        }
        self.settings_changed();
    }

    /// A scene made of the current source, which becomes the active one.
    fn new_scene(&mut self) {
        let count = self.settings.scenes.len();
        let Some(base) = self.main_source_ref() else {
            self.emit(AppEvent::SceneMissing);
            return;
        };
        if count >= MAX_SCENES {
            return;
        }
        let lang = self.lang_mode().resolve(self.system_lang);
        let name =
            i18n::tr(lang, i18n::Key::SceneDefaultName).replace("{n}", &(count + 1).to_string());
        self.settings.scenes.push(Scene::with_base(name, base));
        self.activate_scene(Some(count));
    }

    fn delete_scene(&mut self, index: usize) {
        if index >= self.settings.scenes.len() {
            return;
        }
        self.settings.scenes.remove(index);
        match self.settings.active_scene {
            Some(active) if active == index => self.activate_scene(None),
            Some(active) if active > index => self.settings.active_scene = Some(active - 1),
            _ => {}
        }
    }

    fn active_scene_mut(&mut self) -> Option<&mut Scene> {
        let index = self.settings.active_scene?;
        self.settings.scenes.get_mut(index)
    }

    fn toggle_layer(&mut self, index: usize) {
        let Some(layer) = self
            .active_scene_mut()
            .and_then(|scene| scene.layers.get_mut(index))
        else {
            return;
        };
        layer.visible = !layer.visible;
        let visible = layer.visible;
        if index == 0 {
            if let Some(base) = self.scene_base.as_mut() {
                base.visible = visible;
            }
        } else if let Some(overlay) = self.overlays.iter_mut().find(|o| o.index == index) {
            overlay.layer.visible = visible;
        }
        self.sync_compositor();
    }

    /// Removes an overlay (the base cannot be removed).
    fn remove_layer(&mut self, index: usize) {
        let Some(scene) = self.active_scene_mut() else {
            return;
        };
        if index == 0 || index >= scene.layers.len() {
            return;
        }
        scene.layers.remove(index);
        self.overlays.retain(|o| o.index != index);
        for overlay in &mut self.overlays {
            if overlay.index > index {
                overlay.index -= 1;
            }
        }
        self.sync_compositor();
        self.set_capture_exclusion(self.wants_exclusion());
    }

    /// Adds `source` over the active scene, in the first free corner.
    fn add_layer(&mut self, source: Option<SourceRef>) {
        let Some(source) = source else {
            return;
        };
        let Some(scene) = self.active_scene_mut() else {
            return;
        };
        let Some(index) = scene.add_layer(source.clone()) else {
            return;
        };
        let layer = scene.layers[index].clone();
        let slot = Arc::new(FrameSlot::new());
        match self.start_overlay_source(&source, &slot) {
            Some(running) => self.overlays.push(Overlay {
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
