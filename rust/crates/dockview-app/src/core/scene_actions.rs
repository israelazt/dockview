//! What the scenes popover asks for: pick, make and delete scenes, and edit the
//! layers of the one in use. The saved scene in the settings is changed first,
//! then the running layers (see `scene_runtime`) are brought in line with it.

use std::sync::Arc;

use dockview_domain::device_names;
use dockview_domain::events::AppEvent;
use dockview_domain::frame::FrameSlot;
use dockview_domain::layer_edit;
use dockview_domain::scene::{clean_scene_name, NormRect, Scene, SourceRef, MAX_SCENE_LAYERS};

use dockview_domain::i18n;
use dockview_domain::i18n::LangModeExt;
use dockview_domain::view::{LayerKind, LayerView, SceneAction, ScenesView};

use super::Core;
use crate::scenes_service::Overlay;

/// Scenes a user may keep.
const MAX_SCENES: usize = 12;
/// Edits of layers that can be undone.
const UNDO_LIMIT: usize = 50;

impl Core {
    /// How a source is named in the layer list.
    fn source_label(&self, source: &SourceRef, lang: i18n::Lang) -> String {
        match source {
            SourceRef::Capture { symbolic_link } => {
                let friendly = self
                    .sources
                    .video_devices
                    .iter()
                    .find(|d| &d.symbolic_link == symbolic_link)
                    .map_or("…", |d| d.friendly_name.as_str());
                device_names::display_name(symbolic_link, friendly, &self.settings.device_aliases)
            }
            SourceRef::Monitor { device_name } => {
                match self
                    .sources
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
                        kind: match l.source {
                            SourceRef::Capture { .. } => LayerKind::Capture,
                            SourceRef::Monitor { .. } => LayerKind::Monitor,
                            SourceRef::Window { .. } => LayerKind::Window,
                        },
                        visible: l.visible,
                        locked: l.locked,
                        rect: l.rect,
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
            layer_counts: self
                .settings
                .scenes
                .iter()
                .map(|s| s.layers.len())
                .collect(),
            active,
            can_add: active.is_some() && layers.len() < MAX_SCENE_LAYERS,
            selected: self.view.layer_selected.filter(|i| *i < layers.len()),
            editing: self.view.scene_editing && active.is_some(),
            transition: self.settings.scene_transition,
            transition_ms: self.settings.scene_transition_ms,
            snap: self.settings.canvas_snap,
            grid: self.settings.canvas_grid,
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
            SceneAction::ToggleLock(index) => self.toggle_lock(index),
            SceneAction::RemoveLayer(index) => self.remove_layer(index),
            SceneAction::SetLayerRect(index, units) => self.set_layer_rect(index, units),
            SceneAction::MoveLayer(index, delta) => self.move_layer(index, delta),
            SceneAction::FitLayer => self.fit_layer(),
            SceneAction::BeginEdit(index) => {
                self.begin_edit(index);
                return;
            }
            SceneAction::Undo => {
                self.undo_layer_edit();
                return;
            }
            SceneAction::SelectLayer(index) => {
                self.view.layer_selected = index;
                return;
            }
            SceneAction::ToggleEdit => {
                self.view.scene_editing = !self.view.scene_editing;
                return;
            }
            SceneAction::AddCapture(i) => {
                let source = self
                    .sources
                    .video_devices
                    .get(i)
                    .map(|d| SourceRef::Capture {
                        symbolic_link: d.symbolic_link.clone(),
                    });
                self.add_layer(source);
            }
            SceneAction::AddMonitor(i) => {
                let source = self.sources.monitors.get(i).map(|m| SourceRef::Monitor {
                    device_name: m.device_name.clone(),
                });
                self.add_layer(source);
            }
            SceneAction::AddWindow(i) => {
                let source = self.sources.windows.get(i).map(|w| SourceRef::Window {
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
        let Some(base) = self.sources.main_source_ref() else {
            self.emit(AppEvent::SceneMissing);
            return;
        };
        if count >= MAX_SCENES {
            return;
        }
        let lang = self.lang_mode().resolve(self.view.system_lang);
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
        self.forget_layer_edits();
        match self.settings.active_scene {
            Some(active) if active == index => self.activate_scene(None),
            Some(active) if active > index => self.settings.active_scene = Some(active - 1),
            _ => {}
        }
    }

    /// Gives a scene the name that was typed, if it is a usable one.
    pub(super) fn rename_scene(&mut self, index: usize, typed: &str) {
        let Some(name) = clean_scene_name(typed) else {
            return;
        };
        let Some(scene) = self.settings.scenes.get_mut(index) else {
            return;
        };
        if scene.name != name {
            scene.name = name;
            self.settings_changed();
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
        self.scenes.set_layer_visible(index, visible);
        self.sync_compositor();
    }

    /// Locks or unlocks a layer of the scene in use.
    fn toggle_lock(&mut self, index: usize) {
        if let Some(layer) = self
            .active_scene_mut()
            .and_then(|scene| scene.layers.get_mut(index))
        {
            layer.locked = !layer.locked;
        }
    }

    /// The positions the undo list remembers no longer mean what they did (a
    /// layer or scene was removed or moved in the order).
    pub(super) fn forget_layer_edits(&mut self) {
        self.view.layer_undo.clear();
        self.view.edit_pending = None;
    }

    /// Remembers where `index` is, to go back to it if the editor moves it.
    fn begin_edit(&mut self, index: usize) {
        let (Some(scene), Some(layer)) = (
            self.settings.active_scene,
            self.active_scene_mut()
                .and_then(|s| s.layers.get(index).map(|l| l.rect)),
        ) else {
            return;
        };
        self.view.edit_pending = Some((scene, index, layer));
    }

    /// Puts the last layer the editor moved back where it was.
    pub(super) fn undo_layer_edit(&mut self) {
        while let Some((scene, index, rect)) = self.view.layer_undo.pop() {
            if self.settings.active_scene != Some(scene) {
                continue; // the scene is not the one in use any more
            }
            if self.place_layer(index, rect) {
                self.settings_changed();
                return;
            }
        }
    }

    /// Pushes the chosen layer a step (arrow keys in the editor).
    pub(super) fn nudge_layer(&mut self, dx: i32, dy: i32) {
        let Some(index) = self.view.layer_selected else {
            return;
        };
        let Some(current) = self
            .active_scene_mut()
            .and_then(|scene| scene.layers.get(index).map(|l| l.rect))
        else {
            return;
        };
        let rect = layer_edit::nudged(current, dx, dy);
        if rect == current {
            return;
        }
        self.begin_edit(index);
        self.commit_pending_edit(index);
        self.place_layer(index, rect);
        self.settings_changed();
    }

    /// The held layer's first change: its old place joins the undo list.
    fn commit_pending_edit(&mut self, index: usize) {
        if let Some(held) = self.view.edit_pending.take() {
            if held.1 == index && Some(held.0) == self.settings.active_scene {
                self.view.layer_undo.push(held);
                let excess = self.view.layer_undo.len().saturating_sub(UNDO_LIMIT);
                self.view.layer_undo.drain(..excess);
            }
        }
    }

    /// Puts layer `index` of the active scene in `rect`; whether it exists.
    fn place_layer(&mut self, index: usize, rect: NormRect) -> bool {
        let Some(layer) = self
            .active_scene_mut()
            .and_then(|scene| scene.layers.get_mut(index))
        else {
            return false;
        };
        layer.rect = rect;
        self.scenes.set_layer_rect(index, rect);
        self.sync_compositor();
        true
    }

    /// Puts a layer where the editor dragged it.
    fn set_layer_rect(&mut self, index: usize, units: [i32; 4]) {
        let rect = layer_edit::from_units(units);
        let Some(layer) = self
            .active_scene_mut()
            .and_then(|scene| scene.layers.get_mut(index))
        else {
            return;
        };
        if layer.rect == rect || layer.locked {
            return;
        }
        self.commit_pending_edit(index);
        self.place_layer(index, rect);
    }

    /// Stretches the chosen layer over the whole picture (a locked one stays);
    /// Undo puts it back.
    fn fit_layer(&mut self) {
        let Some(index) = self.view.layer_selected else {
            return;
        };
        let whole = NormRect::new(0.0, 0.0, 1.0, 1.0);
        let Some(layer) = self
            .active_scene_mut()
            .and_then(|scene| scene.layers.get(index))
        else {
            return;
        };
        if layer.locked || layer.rect == whole {
            return;
        }
        self.begin_edit(index);
        self.commit_pending_edit(index);
        self.place_layer(index, whole);
    }

    /// Moves an overlay one step in the drawing order; the selection follows it.
    fn move_layer(&mut self, index: usize, delta: i32) {
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

    /// Removes an overlay (the base cannot be removed).
    fn remove_layer(&mut self, index: usize) {
        let Some(scene) = self.active_scene_mut() else {
            return;
        };
        if index == 0 || index >= scene.layers.len() {
            return;
        }
        scene.layers.remove(index);
        self.forget_layer_edits();
        self.scenes.remove_overlay(index);
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
