//! The scenes popover: pick a scene, make or delete one, and edit its layers
//! (show/hide, remove, add a source).
//!
//! It lives apart from `controls.rs` and only hooks into it through
//! `Hit::Scenes`, `Action::Scene`, `Model::scenes` and `PopoverKind::Scenes`.

use super::{
    fit_text, Controls, Hit, Model, PopoverKind, Rect, Ui, BODY, ICON_CHECK, LABEL_HEIGHT,
    POPOVER_GAP, POPOVER_PAD, POPOVER_WIDTH, ROW_HEIGHT, SECTION, SECTION_GAP,
};
use crate::i18n::{tr, Key};

const ICON_SCENES: char = '\u{e52a}';
const ICON_PLUS: char = '\u{e13d}';
const ICON_EYE: char = '\u{e0ba}';
const ICON_EYE_OFF: char = '\u{e0bb}';
const ICON_TRASH: char = '\u{e18d}';
const ICON_CLOSE: char = '\u{e1b2}';
const ICON_BACK: char = '\u{e06e}';

/// Scenes listed before the rest are cut off.
const MAX_LISTED_SCENES: usize = 6;
/// Rows per list in the "add a source" view, so it fits a 720p window.
const MAX_ADD_DEVICES: usize = 4;
const MAX_ADD_WINDOWS: usize = 6;
/// Size of the small icon buttons inside a row.
const ICON_BUTTON: f32 = 26.0;

/// What the scenes popover can be pointing at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneHit {
    /// The pill's button that opens the popover.
    Button,
    /// "Single source": no scene.
    Plain,
    Row(usize),
    Delete(usize),
    New,
    /// Show or hide a layer.
    Eye(usize),
    RemoveLayer(usize),
    /// Opens the list of sources to add.
    AddSource,
    Back,
    AddCapture(usize),
    AddMonitor(usize),
    AddWindow(usize),
}

impl SceneHit {
    /// Everything but the pill's button is inside the popover.
    pub fn in_popover(self) -> bool {
        self != Self::Button
    }
}

/// A change asked for in the scenes popover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneAction {
    /// Use this scene, or with `None` the plain source.
    Activate(Option<usize>),
    /// Make a scene out of the current source and use it.
    New,
    Delete(usize),
    /// Show or hide a layer of the active scene.
    ToggleLayer(usize),
    RemoveLayer(usize),
    /// Add the capture card, monitor or window at this position of its list.
    AddCapture(usize),
    AddMonitor(usize),
    AddWindow(usize),
}

/// One layer as the popover shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerView {
    pub name: String,
    pub visible: bool,
}

/// The scenes and the layers of the one in use.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScenesView {
    pub names: Vec<String>,
    pub active: Option<usize>,
    /// The first one is the base.
    pub layers: Vec<LayerView>,
    /// Whether another layer fits in the active scene.
    pub can_add: bool,
}

/// The text of a tooltip in the popover.
pub fn tooltip_key(hit: SceneHit, m: &Model) -> Option<Key> {
    Some(match hit {
        SceneHit::Button => Key::TipScenes,
        SceneHit::Plain => Key::TipScenePlain,
        SceneHit::Row(_) => Key::TipSceneRow,
        SceneHit::Delete(_) => Key::TipSceneDelete,
        SceneHit::New => Key::TipSceneNew,
        SceneHit::Eye(i) => {
            if m.scenes.layers.get(i).is_some_and(|l| l.visible) {
                Key::TipLayerHide
            } else {
                Key::TipLayerShow
            }
        }
        SceneHit::RemoveLayer(_) => Key::TipLayerRemove,
        SceneHit::AddSource => Key::TipSceneAdd,
        SceneHit::Back => Key::TipSceneBack,
        SceneHit::AddCapture(_) | SceneHit::AddMonitor(_) | SceneHit::AddWindow(_) => return None,
    })
}

/// Height of the popover for `m`.
pub fn scenes_height(m: &Model, adding: bool) -> f32 {
    if adding {
        // The back row, then a list each of cards, screens and windows.
        let section = |n: usize, limit: usize| LABEL_HEIGHT + n.clamp(1, limit) as f32 * ROW_HEIGHT;
        return POPOVER_PAD * 2.0
            + ROW_HEIGHT
            + 8.0
            + section(m.video_devices.len(), MAX_ADD_DEVICES)
            + section(m.sources.monitors.len(), MAX_ADD_DEVICES)
            + section(m.sources.windows.len(), MAX_ADD_WINDOWS)
            + SECTION_GAP * 2.0;
    }
    let scenes = m.scenes.names.len().min(MAX_LISTED_SCENES);
    let mut h = POPOVER_PAD * 2.0 + LABEL_HEIGHT + ROW_HEIGHT * (scenes + 2) as f32;
    if m.scenes.active.is_some() {
        h += SECTION_GAP + LABEL_HEIGHT + ROW_HEIGHT * m.scenes.layers.len() as f32;
        if m.scenes.can_add {
            h += ROW_HEIGHT;
        }
    }
    h
}

impl Controls {
    pub fn toggle_scenes(&mut self) {
        let was_open = self.scenes_open;
        self.close_popovers();
        self.scenes_open = !was_open;
        if self.scenes_open {
            self.popover_kind = PopoverKind::Scenes;
        }
    }

    pub fn scenes_open(&self) -> bool {
        self.scenes_open
    }

    /// Handles a click on something in the scenes popover.
    pub(super) fn activate_scene_hit(&mut self, hit: SceneHit) {
        use SceneAction as A;
        let action = match hit {
            SceneHit::Button => {
                self.toggle_scenes();
                return;
            }
            SceneHit::Plain => A::Activate(None),
            SceneHit::Row(i) => A::Activate(Some(i)),
            SceneHit::Delete(i) => A::Delete(i),
            SceneHit::New => A::New,
            SceneHit::Eye(i) => A::ToggleLayer(i),
            SceneHit::RemoveLayer(i) => A::RemoveLayer(i),
            SceneHit::AddSource => {
                self.scenes_adding = true;
                return;
            }
            SceneHit::Back => {
                self.scenes_adding = false;
                return;
            }
            // Adding something goes back to the list of layers.
            SceneHit::AddCapture(i) => {
                self.scenes_adding = false;
                A::AddCapture(i)
            }
            SceneHit::AddMonitor(i) => {
                self.scenes_adding = false;
                A::AddMonitor(i)
            }
            SceneHit::AddWindow(i) => {
                self.scenes_adding = false;
                A::AddWindow(i)
            }
        };
        self.actions.push(super::Action::Scene(action));
    }

    /// The pill's button.
    pub(super) fn scenes_button(&mut self, ui: &mut Ui, m: &Model, cx: f32, cy: f32) {
        let open = self.scenes_open;
        self.icon_button(
            ui,
            m,
            Hit::Scenes(SceneHit::Button),
            ICON_SCENES,
            cx,
            cy,
            super::ICON_SIZE,
            open,
        );
    }

    /// A clickable row with a hover wash; returns its rectangle.
    fn scene_row(&mut self, ui: &mut Ui, m: &Model, hit: SceneHit, x: f32, y: f32, w: f32) -> Rect {
        let row = Rect::new(x, y, w, ROW_HEIGHT);
        let hit = Hit::Scenes(hit);
        self.register(hit, row);
        let (hover, press) = self.amounts(hit);
        let alpha = m.theme.hover.a * (hover + 0.9 * press);
        if alpha > 0.002 {
            ui.rect(row, 9.0, m.theme.hover.with_alpha(alpha.min(1.0)));
        }
        row
    }

    /// A small icon button at the right end of a row, `from_right` pixels in.
    fn row_icon(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        hit: SceneHit,
        icon: char,
        row: Rect,
        left: f32,
    ) {
        let rect = Rect::new(
            left,
            row.y + (ROW_HEIGHT - ICON_BUTTON) / 2.0,
            ICON_BUTTON,
            ICON_BUTTON,
        );
        let hit = Hit::Scenes(hit);
        self.register(hit, rect);
        let (hover, press) = self.amounts(hit);
        let alpha = m.theme.hover.a * (hover + 0.9 * press);
        if alpha > 0.002 {
            ui.rect(rect, 8.0, m.theme.hover.with_alpha(alpha.min(1.0)));
        }
        let color = if hover > 0.01 {
            m.theme.text
        } else {
            m.theme.text_dim
        };
        ui.icon(
            icon,
            rect.x + rect.w / 2.0,
            rect.y + rect.h / 2.0,
            15.0,
            color,
        );
    }

    fn row_label(
        &self,
        ui: &mut Ui,
        text: &str,
        row: Rect,
        left: f32,
        right: f32,
        color: super::Color,
    ) {
        let label = fit_text(ui, text, BODY, (row.x + row.w - right) - left);
        let line_h = ui.measure("Ag", BODY).1;
        ui.text(
            &label,
            left,
            row.y + (ROW_HEIGHT - line_h) / 2.0,
            BODY,
            color,
        );
    }

    pub(super) fn draw_scenes_popover(&mut self, ui: &mut Ui, m: &Model, pill: Rect) {
        let t = &m.theme;
        let adding = self.scenes_adding;
        let height = scenes_height(m, adding);
        let (screen_w, _) = ui.logical_size();
        let x = (pill.x + pill.w - POPOVER_WIDTH)
            .min(screen_w - POPOVER_WIDTH - POPOVER_GAP)
            .max(POPOVER_GAP)
            .round();
        let top = (pill.y - POPOVER_GAP - height).max(POPOVER_GAP).round();
        let rect = Rect::new(x, top, POPOVER_WIDTH, height);
        self.register(Hit::Popover, rect);
        ui.shadow(rect, 20.0, 26.0, t.shadow);
        ui.panel(rect, 20.0, t.surface, t.border, 1.0);

        let inner_x = x + POPOVER_PAD;
        let inner_w = POPOVER_WIDTH - POPOVER_PAD * 2.0;
        let y = top + POPOVER_PAD;
        if adding {
            self.draw_add_view(ui, m, inner_x, y, inner_w);
        } else {
            self.draw_scene_list(ui, m, inner_x, y, inner_w);
        }
    }

    fn draw_scene_list(&mut self, ui: &mut Ui, m: &Model, x: f32, mut y: f32, w: f32) {
        let t = &m.theme;
        let lang = m.lang;
        ui.text(tr(lang, Key::ScenesSection), x, y, SECTION, t.text_dim);
        y += LABEL_HEIGHT;

        // No scene: the plain source.
        let row = self.scene_row(ui, m, SceneHit::Plain, x, y, w);
        let plain = m.scenes.active.is_none();
        let color = if plain { t.text } else { t.text_dim };
        self.row_label(ui, tr(lang, Key::ScenePlain), row, x + 10.0, 40.0, color);
        if plain {
            ui.icon(
                ICON_CHECK,
                x + w - 18.0,
                y + ROW_HEIGHT / 2.0,
                15.0,
                t.accent,
            );
        }
        y += ROW_HEIGHT;

        for (i, name) in m.scenes.names.iter().take(MAX_LISTED_SCENES).enumerate() {
            let row = self.scene_row(ui, m, SceneHit::Row(i), x, y, w);
            let active = m.scenes.active == Some(i);
            let color = if active { t.text } else { t.text_dim };
            // Room for the check mark and the trash button on the right.
            self.row_label(ui, name, row, x + 10.0, 62.0, color);
            if active {
                ui.icon(
                    ICON_CHECK,
                    x + w - 44.0,
                    y + ROW_HEIGHT / 2.0,
                    15.0,
                    t.accent,
                );
            }
            self.row_icon(
                ui,
                m,
                SceneHit::Delete(i),
                ICON_TRASH,
                row,
                x + w - ICON_BUTTON - 2.0,
            );
            y += ROW_HEIGHT;
        }

        let row = self.scene_row(ui, m, SceneHit::New, x, y, w);
        ui.icon(ICON_PLUS, x + 18.0, y + ROW_HEIGHT / 2.0, 15.0, t.accent);
        self.row_label(ui, tr(lang, Key::SceneNew), row, x + 36.0, 10.0, t.accent);
        y += ROW_HEIGHT;

        if m.scenes.active.is_none() {
            return;
        }
        y += SECTION_GAP;
        ui.text(tr(lang, Key::SceneLayers), x, y, SECTION, t.text_dim);
        y += LABEL_HEIGHT;
        for (i, layer) in m.scenes.layers.iter().enumerate() {
            let row = Rect::new(x, y, w, ROW_HEIGHT);
            let eye = if layer.visible {
                ICON_EYE
            } else {
                ICON_EYE_OFF
            };
            self.row_icon(ui, m, SceneHit::Eye(i), eye, row, x);
            let color = if layer.visible { t.text } else { t.text_dim };
            let name = if i == 0 {
                format!("{} · {}", layer.name, tr(lang, Key::SceneBaseTag))
            } else {
                layer.name.clone()
            };
            let right = if i == 0 { 10.0 } else { 40.0 };
            self.row_label(ui, &name, row, x + ICON_BUTTON + 8.0, right, color);
            if i > 0 {
                self.row_icon(
                    ui,
                    m,
                    SceneHit::RemoveLayer(i),
                    ICON_CLOSE,
                    row,
                    x + w - ICON_BUTTON - 2.0,
                );
            }
            y += ROW_HEIGHT;
        }
        if m.scenes.can_add {
            let row = self.scene_row(ui, m, SceneHit::AddSource, x, y, w);
            ui.icon(ICON_PLUS, x + 18.0, y + ROW_HEIGHT / 2.0, 15.0, t.accent);
            self.row_label(
                ui,
                tr(lang, Key::SceneAddSource),
                row,
                x + 36.0,
                10.0,
                t.accent,
            );
        }
    }

    fn draw_add_view(&mut self, ui: &mut Ui, m: &Model, x: f32, mut y: f32, w: f32) {
        let t = &m.theme;
        let row = self.scene_row(ui, m, SceneHit::Back, x, y, w);
        ui.icon(ICON_BACK, x + 14.0, y + ROW_HEIGHT / 2.0, 15.0, t.text_dim);
        self.row_label(
            ui,
            tr(m.lang, Key::SceneAddTitle),
            row,
            x + 32.0,
            10.0,
            t.text,
        );
        y += ROW_HEIGHT + 8.0;

        y = self.list_section(
            ui,
            m,
            Key::SourceCapture,
            &m.video_devices,
            None,
            |i| Hit::Scenes(SceneHit::AddCapture(i)),
            x,
            y,
            w,
            MAX_ADD_DEVICES,
        );
        y += SECTION_GAP;
        y = self.list_section(
            ui,
            m,
            Key::SourceScreen,
            &m.sources.monitors,
            None,
            |i| Hit::Scenes(SceneHit::AddMonitor(i)),
            x,
            y,
            w,
            MAX_ADD_DEVICES,
        );
        y += SECTION_GAP;
        self.list_section(
            ui,
            m,
            Key::SourceApp,
            &m.sources.windows,
            None,
            |i| Hit::Scenes(SceneHit::AddWindow(i)),
            x,
            y,
            w,
            MAX_ADD_WINDOWS,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::Rig;
    use super::super::Action;
    use super::*;
    use dockview_domain::scene::MAX_SCENE_LAYERS;

    fn view(active: Option<usize>) -> ScenesView {
        ScenesView {
            names: vec!["Juego".into(), "Charla".into()],
            active,
            layers: vec![
                LayerView {
                    name: "Pacoxi".into(),
                    visible: true,
                },
                LayerView {
                    name: "Pantalla 2".into(),
                    visible: false,
                },
            ],
            can_add: true,
        }
    }

    /// A rig with the popover open and drawn.
    fn open(view: ScenesView) -> Rig {
        let mut rig = Rig::new(true);
        rig.model.scenes = view;
        rig.model.video_devices = vec!["Cam".into()];
        rig.model.sources.monitors = vec!["Pantalla 1".into(), "Pantalla 2".into()];
        rig.model.sources.windows = vec!["Mi juego".into()];
        rig.click_on(Hit::Scenes(SceneHit::Button));
        rig.frame(0.1);
        rig.frame(0.1);
        rig
    }

    fn click(rig: &mut Rig, hit: SceneHit) -> Vec<Action> {
        rig.click_on(Hit::Scenes(hit));
        rig.actions()
    }

    #[test]
    fn the_button_opens_and_closes_the_popover() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::Scenes(SceneHit::Button));
        assert!(rig.controls.scenes_open());
        rig.frame(0.1);
        rig.click_on(Hit::Scenes(SceneHit::Button));
        assert!(!rig.controls.scenes_open());
    }

    #[test]
    fn only_one_popover_is_open_at_a_time() {
        let mut rig = open(view(None));
        rig.click_on(Hit::Sources);
        assert!(rig.controls.sources_open() && !rig.controls.scenes_open());
        rig.frame(0.1);
        rig.click_on(Hit::Scenes(SceneHit::Button));
        assert!(rig.controls.scenes_open() && !rig.controls.sources_open());
    }

    #[test]
    fn rows_ask_to_switch_make_and_delete_scenes() {
        let mut rig = open(view(Some(0)));
        let scene = |a| vec![Action::Scene(a)];
        assert_eq!(
            click(&mut rig, SceneHit::Row(1)),
            scene(SceneAction::Activate(Some(1)))
        );
        assert_eq!(
            click(&mut rig, SceneHit::Plain),
            scene(SceneAction::Activate(None))
        );
        assert_eq!(click(&mut rig, SceneHit::New), scene(SceneAction::New));
        assert_eq!(
            click(&mut rig, SceneHit::Delete(0)),
            scene(SceneAction::Delete(0))
        );
        assert!(rig.controls.scenes_open(), "the popover stays open");
    }

    #[test]
    fn deleting_does_not_also_switch_to_the_scene() {
        // The trash button sits on top of its row: only one of them fires.
        let mut rig = open(view(Some(1)));
        let actions = click(&mut rig, SceneHit::Delete(1));
        assert_eq!(actions, vec![Action::Scene(SceneAction::Delete(1))]);
    }

    #[test]
    fn layers_can_be_hidden_and_removed_but_not_the_base() {
        let mut rig = open(view(Some(0)));
        let scene = |a| vec![Action::Scene(a)];
        assert_eq!(
            click(&mut rig, SceneHit::Eye(1)),
            scene(SceneAction::ToggleLayer(1))
        );
        assert_eq!(
            click(&mut rig, SceneHit::Eye(0)),
            scene(SceneAction::ToggleLayer(0))
        );
        assert_eq!(
            click(&mut rig, SceneHit::RemoveLayer(1)),
            scene(SceneAction::RemoveLayer(1))
        );
        let drawn = |rig: &Rig, hit| rig.controls.hits.iter().any(|(h, _)| *h == hit);
        assert!(!drawn(&rig, Hit::Scenes(SceneHit::RemoveLayer(0))));
    }

    #[test]
    fn the_layers_only_show_while_a_scene_is_active() {
        let rig = open(view(None));
        let drawn = |hit| rig.controls.hits.iter().any(|(h, _)| *h == hit);
        assert!(!drawn(Hit::Scenes(SceneHit::Eye(0))));
        assert!(!drawn(Hit::Scenes(SceneHit::AddSource)));
        assert!(drawn(Hit::Scenes(SceneHit::New)));
    }

    #[test]
    fn a_full_scene_offers_no_more_layers() {
        let mut full = view(Some(0));
        full.can_add = false;
        let rig = open(full);
        assert!(!rig
            .controls
            .hits
            .iter()
            .any(|(h, _)| *h == Hit::Scenes(SceneHit::AddSource)));
    }

    #[test]
    fn adding_a_source_goes_through_the_list_and_back() {
        let mut rig = open(view(Some(0)));
        assert!(click(&mut rig, SceneHit::AddSource).is_empty());
        rig.frame(0.1);
        // The sources are listed; the scene's own rows are not.
        assert_eq!(
            click(&mut rig, SceneHit::AddMonitor(1)),
            vec![Action::Scene(SceneAction::AddMonitor(1))]
        );
        rig.frame(0.1);
        // Back on the list of layers.
        assert!(rig
            .controls
            .hits
            .iter()
            .any(|(h, _)| *h == Hit::Scenes(SceneHit::AddSource)));
        click(&mut rig, SceneHit::AddSource);
        rig.frame(0.1);
        assert!(click(&mut rig, SceneHit::Back).is_empty());
        rig.frame(0.1);
        assert!(rig
            .controls
            .hits
            .iter()
            .any(|(h, _)| *h == Hit::Scenes(SceneHit::New)));
        click(&mut rig, SceneHit::AddSource);
        rig.frame(0.1);
        assert_eq!(
            click(&mut rig, SceneHit::AddCapture(0)),
            vec![Action::Scene(SceneAction::AddCapture(0))]
        );
        click(&mut rig, SceneHit::AddSource);
        rig.frame(0.1);
        assert_eq!(
            click(&mut rig, SceneHit::AddWindow(0)),
            vec![Action::Scene(SceneAction::AddWindow(0))]
        );
    }

    #[test]
    fn closing_the_popover_forgets_the_add_list() {
        let mut rig = open(view(Some(0)));
        click(&mut rig, SceneHit::AddSource);
        rig.frame(0.1);
        rig.click_on(Hit::Scenes(SceneHit::Button)); // close
        rig.frame(0.1);
        rig.click_on(Hit::Scenes(SceneHit::Button)); // open again
        rig.frame(0.1);
        rig.frame(0.1);
        assert!(rig
            .controls
            .hits
            .iter()
            .any(|(h, _)| *h == Hit::Scenes(SceneHit::New)));
    }

    #[test]
    fn the_popover_fits_a_720p_window_in_both_views() {
        let layers = |n: usize| -> Vec<LayerView> {
            (0..n)
                .map(|i| LayerView {
                    name: format!("Capa {i}"),
                    visible: true,
                })
                .collect()
        };
        let fits = |rig: &mut Rig, what: &str| {
            rig.frame(0.3);
            let popover = rig.controls.rect_of(Hit::Popover);
            let pill = rig.controls.rect_of(Hit::Pill);
            assert!(popover.y >= 0.0, "{what}: top {}", popover.y);
            assert!(popover.y + popover.h <= pill.y, "{what}: overlaps the pill");
        };

        // The fullest scene list: all the scenes and a full scene of layers.
        let mut full = view(Some(0));
        full.names = (0..12).map(|i| format!("Escena {i}")).collect();
        full.layers = layers(MAX_SCENE_LAYERS);
        full.can_add = false;
        let mut rig = open(full);
        fits(&mut rig, "the list with a full scene");

        // One layer short of full, so the add list can be reached, with as many
        // sources as the lists show.
        let mut almost = view(Some(0));
        almost.layers = layers(MAX_SCENE_LAYERS - 1);
        rig.model.scenes = almost;
        rig.model.video_devices = (0..6).map(|i| format!("Cam {i}")).collect();
        rig.model.sources.monitors = (0..4).map(|i| format!("Pantalla {i}")).collect();
        rig.model.sources.windows = (0..10).map(|i| format!("Ventana {i}")).collect();
        rig.frame(0.1);
        fits(&mut rig, "the list with room to add");
        click(&mut rig, SceneHit::AddSource);
        fits(&mut rig, "the list of sources to add");
    }

    #[test]
    fn tips_exist_in_both_languages_and_follow_the_layer() {
        use crate::i18n::Lang;
        let mut m = Rig::new(true).model;
        m.scenes = view(Some(0));
        assert_eq!(tooltip_key(SceneHit::Eye(0), &m), Some(Key::TipLayerHide));
        assert_eq!(tooltip_key(SceneHit::Eye(1), &m), Some(Key::TipLayerShow));
        assert_eq!(tooltip_key(SceneHit::AddMonitor(0), &m), None);
        for hit in [
            SceneHit::Button,
            SceneHit::Plain,
            SceneHit::Row(0),
            SceneHit::Delete(0),
            SceneHit::New,
            SceneHit::RemoveLayer(1),
            SceneHit::AddSource,
            SceneHit::Back,
        ] {
            let key = tooltip_key(hit, &m).expect("a tip");
            for lang in [Lang::En, Lang::Es] {
                assert!(!tr(lang, key).is_empty());
            }
        }
    }
}
