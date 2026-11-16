use super::*;
use crate::test_model::model;
use dockview_domain::view::{LayerView, ScenesView};

const AREA: (f32, f32, f32, f32) = (0.0, 0.0, 1920.0, 1080.0);
const NO_SNAP: Modifiers = Modifiers {
    keep_aspect: false,
    no_snap: true,
    grid: false,
};

struct Rig {
    ui: Ui,
    editor: SceneEditor,
    model: Model,
    pointer: Pointer,
    mods: Modifiers,
}

fn layer(name: &str, visible: bool, rect: NormRect) -> LayerView {
    LayerView {
        kind: Default::default(),
        locked: false,
        name: name.into(),
        visible,
        rect,
    }
}

impl Rig {
    /// A base over the left half and a layer over the lower right, chosen.
    fn new() -> Self {
        let mut model = model();
        model.scenes = ScenesView {
            names: vec!["Scene".into()],
            active: Some(0),
            layers: vec![
                layer("Base", true, NormRect::new(0.0, 0.0, 0.5, 1.0)),
                layer("Corner", true, NormRect::new(0.6, 0.6, 0.3, 0.3)),
            ],
            can_add: true,
            selected: Some(1),
            editing: true,
            snap: true,
            ..ScenesView::default()
        };
        Self {
            ui: Ui::new(),
            editor: SceneEditor::new(),
            model,
            pointer: Pointer::default(),
            mods: NO_SNAP,
        }
    }

    fn frame(&mut self) {
        self.ui.begin((1920, 1080), 1.0);
        self.editor
            .frame(&mut self.ui, &self.model, AREA, &self.pointer, self.mods);
        self.pointer.end_frame();
    }

    /// Screen position of a point of the picture (the area is 1920×1080).
    fn at(nx: f32, ny: f32) -> (f32, f32) {
        (nx * 1920.0, ny * 1080.0)
    }

    fn move_to(&mut self, (x, y): (f32, f32)) {
        self.pointer.pos = Some((x, y));
        self.frame();
    }

    fn press(&mut self, to: (f32, f32)) {
        self.move_to(to);
        self.pointer.press();
        self.frame();
    }

    fn release(&mut self) {
        self.pointer.release();
        self.frame();
    }

    /// What the editor asked for, without the notes it leaves for the undo list.
    fn actions(&mut self) -> Vec<Action> {
        let mut all = self.editor.take_actions();
        all.retain(|a| !matches!(a, Action::Scene(SceneAction::BeginEdit(_))));
        all
    }
}

fn scene(action: SceneAction) -> Action {
    Action::Scene(action)
}

#[test]
fn nothing_is_drawn_or_asked_unless_the_editor_is_on_with_layers() {
    let mut rig = Rig::new();
    rig.model.scenes.editing = false;
    rig.press(Rig::at(0.7, 0.7));
    assert!(rig.actions().is_empty());
    assert!(rig.ui.vertices().is_empty());
    assert!(!rig.editor.is_dragging());

    rig.model.scenes.editing = true;
    rig.model.scenes.layers.clear();
    rig.frame();
    assert!(rig.ui.vertices().is_empty());
}

#[test]
fn the_editor_draws_outlines_handles_and_a_name() {
    let mut rig = Rig::new();
    rig.frame();
    // Two outlines, the chosen one's fill, its chip, and eight handles.
    assert!(rig.ui.vertices().len() > 12 * 4);
}

#[test]
fn pressing_a_layer_chooses_it_and_dragging_moves_it() {
    let mut rig = Rig::new();
    rig.press(Rig::at(0.2, 0.5)); // on the base, which is not the chosen one
    assert_eq!(rig.actions(), [scene(SceneAction::SelectLayer(Some(0)))]);
    assert!(rig.editor.is_dragging());
    // The base is as tall as the picture, so only sideways moves are possible.
    rig.move_to(Rig::at(0.3, 0.5));
    let moved = rig.actions();
    assert_eq!(
        moved,
        [scene(SceneAction::SetLayerRect(0, [1000, 0, 5000, 10000]))]
    );
    rig.release();
    assert!(!rig.editor.is_dragging());
    rig.move_to(Rig::at(0.4, 0.5));
    assert!(
        rig.actions().is_empty(),
        "no more changes after the release"
    );
}

#[test]
fn a_handle_of_the_chosen_layer_resizes_it() {
    let mut rig = Rig::new();
    // The bottom right handle of the corner layer is at (0.9, 0.9).
    rig.press(Rig::at(0.9, 0.9));
    assert!(
        rig.actions().is_empty(),
        "grabbing a handle chooses nothing new"
    );
    rig.move_to(Rig::at(0.95, 0.95));
    assert_eq!(
        rig.actions(),
        [scene(SceneAction::SetLayerRect(
            1,
            [6000, 6000, 3500, 3500]
        ))]
    );
}

#[test]
fn the_body_of_the_chosen_layer_moves_it_without_choosing_anything_else() {
    let mut rig = Rig::new();
    rig.press(Rig::at(0.75, 0.75));
    assert!(rig.actions().is_empty());
    rig.move_to(Rig::at(0.65, 0.65));
    assert_eq!(
        rig.actions(),
        [scene(SceneAction::SetLayerRect(
            1,
            [5000, 5000, 3000, 3000]
        ))]
    );
}

#[test]
fn a_press_on_empty_picture_chooses_nothing_and_outside_it_does_nothing() {
    let mut rig = Rig::new();
    rig.press(Rig::at(0.55, 0.2)); // between the base and the corner layer
    assert_eq!(rig.actions(), [scene(SceneAction::SelectLayer(None))]);
    assert!(!rig.editor.is_dragging());
    rig.release();

    // Letterboxed: a wide area leaves bars on the sides where there is no picture.
    let mut rig = Rig::new();
    rig.ui.begin((3000, 1080), 1.0);
    rig.editor.frame(
        &mut rig.ui,
        &rig.model,
        (0.0, 0.0, 3000.0, 1080.0),
        &Pointer {
            pos: Some((50.0, 500.0)),
            down: true,
            just_pressed: true,
            ..Pointer::default()
        },
        NO_SNAP,
    );
    assert!(rig.actions().is_empty(), "the bars are not the picture");
}

#[test]
fn a_hidden_layer_cannot_be_picked() {
    let mut rig = Rig::new();
    rig.model.scenes.layers[0].visible = false;
    rig.press(Rig::at(0.2, 0.5));
    assert_eq!(rig.actions(), [scene(SceneAction::SelectLayer(None))]);
}

#[test]
fn the_cursor_hint_follows_the_handle_under_the_pointer() {
    let mut rig = Rig::new();
    rig.move_to(Rig::at(0.9, 0.9));
    assert_eq!(rig.editor.hover(), Some(Handle::BottomRight));
    rig.move_to(Rig::at(0.75, 0.75));
    assert_eq!(rig.editor.hover(), Some(Handle::Move));
    rig.move_to(Rig::at(0.2, 0.2));
    assert_eq!(rig.editor.hover(), None);
}

#[test]
fn the_drawn_layer_follows_the_drag_before_the_model_answers() {
    let mut rig = Rig::new();
    rig.press(Rig::at(0.75, 0.75));
    rig.move_to(Rig::at(0.65, 0.65));
    rig.actions();
    // The model still has the old rectangle; the editor shows the new one.
    assert_eq!(
        rig.editor.rect_of(&rig.model, 1),
        NormRect::new(0.5, 0.5, 0.3, 0.3)
    );
    // Once the model has it, the editor follows the model again.
    rig.model.scenes.layers[1].rect = NormRect::new(0.5, 0.5, 0.3, 0.3);
    rig.release();
    assert_eq!(rig.editor.live, None);
}

#[test]
fn snapping_pulls_a_move_to_the_centre_unless_it_is_turned_off() {
    let mut rig = Rig::new();
    rig.mods = Modifiers::default();
    // Move the corner layer so its centre lands 3 px from the picture's.
    rig.press(Rig::at(0.75, 0.75));
    rig.move_to((
        0.75 * 1920.0 - 0.25 * 1920.0 + 3.0,
        0.75 * 1080.0 - 0.25 * 1080.0 + 3.0,
    ));
    let [Action::Scene(SceneAction::SetLayerRect(_, units))] = &rig.actions()[..] else {
        panic!("one rect expected");
    };
    // The layer is 0.3 wide: centred it starts at 0.35.
    assert_eq!(&units[..2], &[3500, 3500]);
}

/// The corner layer moved so its centre is 3 px past the picture's centre;
/// the first (and only) rectangle asked for.
fn drag_near_the_centre(rig: &mut Rig) -> [i32; 4] {
    rig.press(Rig::at(0.75, 0.75));
    rig.move_to((
        0.75 * 1920.0 - 0.25 * 1920.0 + 3.0,
        0.75 * 1080.0 - 0.25 * 1080.0 + 3.0,
    ));
    let [Action::Scene(SceneAction::SetLayerRect(_, units))] = &rig.actions()[..] else {
        panic!("one rect expected");
    };
    *units
}

#[test]
fn the_snap_switch_in_the_bar_turns_the_pull_off() {
    let mut rig = Rig::new();
    rig.mods = Modifiers::default();
    rig.model.scenes.snap = false;
    let units = drag_near_the_centre(&mut rig);
    // 3 px of 1920 and 1080 past 0.35, unpulled.
    assert_eq!(units[0], 3516);
    assert_eq!(units[1], 3528);
}

#[test]
fn the_grid_pulls_a_move_to_its_lines() {
    let mut rig = Rig::new();
    rig.mods = Modifiers::default();
    rig.model.scenes.snap = true;
    rig.model.scenes.grid = true;
    // The corner layer starts at x = 0.6. Its left edge goes to 4 px past
    // 7/16 = 0.4375, a line only the grid has (nothing else is near).
    rig.press(Rig::at(0.75, 0.75));
    let target_left = 0.4375 * 1920.0 + 4.0;
    let grab = 0.75 * 1920.0;
    let dx = target_left - 0.6 * 1920.0;
    rig.move_to((grab + dx, 0.75 * 1080.0));
    let [Action::Scene(SceneAction::SetLayerRect(_, units))] = &rig.actions()[..] else {
        panic!("one rect expected");
    };
    assert_eq!(units[0], 4375, "the left edge lands on 7/16");
}

/// What one plain rectangle adds to the drawing.
fn rect_vertices() -> usize {
    let mut ui = Ui::new();
    ui.begin((100, 100), 1.0);
    let before = ui.vertices().len();
    ui.rect(
        Rect::new(0.0, 0.0, 10.0, 1.0),
        0.0,
        dockview_domain::theme::Color::hex(0xFFFFFF, 1.0),
    );
    ui.vertices().len() - before
}

#[test]
fn the_grid_is_drawn_only_when_it_is_on() {
    let mut rig = Rig::new();
    rig.frame();
    let without = rig.ui.vertices().len();
    rig.model.scenes.grid = true;
    rig.frame();
    // 17 + 10 lines.
    assert_eq!(rig.ui.vertices().len(), without + 27 * rect_vertices());
}

#[test]
fn guides_show_while_a_layer_sits_on_a_line_and_go_when_the_drag_ends() {
    let mut rig = Rig::new();
    rig.mods = Modifiers::default();
    rig.frame();
    let plain = rig.ui.vertices().len();
    // Pulled to the picture's centre on both axes: two guides.
    drag_near_the_centre(&mut rig);
    rig.frame();
    assert_eq!(rig.ui.vertices().len(), plain + 2 * rect_vertices());
    rig.release();
    rig.model.scenes.layers[1].rect = NormRect::new(0.35, 0.35, 0.3, 0.3);
    rig.frame();
    assert_eq!(rig.ui.vertices().len(), plain, "no guides at rest");
}

#[test]
fn a_press_at_rest_does_not_nudge_a_layer_towards_a_snap_line() {
    let mut rig = Rig::new();
    rig.mods = Modifiers::default();
    // The chosen layer's right edge is at 0.9: 0.45 of its width away from the
    // centre line, nothing near; move it close to the right edge of the picture
    // first, then press on it without moving.
    rig.model.scenes.layers[1].rect = NormRect::new(0.695, 0.6, 0.3, 0.3);
    rig.press(Rig::at(0.8, 0.75));
    rig.move_to((Rig::at(0.8, 0.75).0 + 1.0, Rig::at(0.8, 0.75).1));
    assert!(rig.actions().is_empty(), "a wobble of a pixel is a click");
    rig.move_to((Rig::at(0.8, 0.75).0 + 8.0, Rig::at(0.8, 0.75).1));
    assert!(!rig.actions().is_empty(), "past the slop it is a drag");
}

#[test]
fn the_drawn_rectangle_goes_back_to_the_model_if_it_never_answers() {
    let mut rig = Rig::new();
    rig.press(Rig::at(0.75, 0.75));
    rig.move_to(Rig::at(0.65, 0.65));
    rig.release();
    for _ in 0..6 {
        rig.frame();
    }
    assert_eq!(rig.editor.live, None);
}

#[test]
fn taking_hold_of_a_layer_tells_the_application_which_one_for_the_undo_list() {
    let mut rig = Rig::new();
    rig.press(Rig::at(0.75, 0.75)); // the chosen layer's body
    assert_eq!(
        rig.editor.take_actions(),
        [scene(SceneAction::BeginEdit(1))]
    );
    rig.release();
    // A press on empty picture holds nothing.
    rig.press(Rig::at(0.55, 0.2));
    assert_eq!(
        rig.editor.take_actions(),
        [scene(SceneAction::SelectLayer(None))]
    );
}
