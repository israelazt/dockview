//! The scene actions of `Core` on fakes: renaming, the layer editor (placing,
//! ordering, undo, arrow keys) and the scene hotkeys.

use dockview_domain::settings::AppSettings;
use dockview_domain::view::Action;

use crate::testing::rig::Rig;

// -- Renaming a scene ----------------------------------------------------

pub(super) fn rig_with_scenes(names: &[&str]) -> Rig {
    use dockview_domain::scene::{Scene, SourceRef};
    let scenes = names
        .iter()
        .map(|name| {
            Scene::with_base(
                *name,
                SourceRef::Window {
                    title: "Game".into(),
                },
            )
        })
        .collect();
    Rig::builder()
        .settings(AppSettings {
            scenes,
            ..AppSettings::default()
        })
        .build()
}

#[test]
fn renaming_a_scene_keeps_the_clean_name_and_saves_it() {
    let mut rig = rig_with_scenes(&["One", "Two"]);
    rig.core.apply(Action::RenameScene(1, "  Stream  ".into()));
    let saved = rig.saved_soon().expect("saved");
    assert_eq!(saved.scenes[0].name, "One");
    assert_eq!(saved.scenes[1].name, "Stream");
}

#[test]
fn an_unusable_rename_changes_nothing() {
    let mut rig = rig_with_scenes(&["One"]);
    rig.core.apply(Action::RenameScene(0, "   ".into()));
    rig.core.apply(Action::RenameScene(5, "Ghost".into()));
    assert!(
        rig.saved_soon().is_none(),
        "nothing changed, nothing written"
    );
    assert_eq!(rig.core.model().scenes.names, ["One"]);
}

// -- Editing the layers of a scene -----------------------------------------

use dockview_domain::scene::{NormRect, Scene, SourceRef};
use dockview_domain::view::SceneAction;

/// A rig whose active scene has a base and two layers over it.
pub(super) fn rig_editing() -> Rig {
    let mut scene = Scene::with_base(
        "Edit",
        SourceRef::Window {
            title: "Base".into(),
        },
    );
    scene.add_layer(SourceRef::Window {
        title: "One".into(),
    });
    scene.add_layer(SourceRef::Window {
        title: "Two".into(),
    });
    Rig::builder()
        .settings(AppSettings {
            scenes: vec![scene],
            active_scene: Some(0),
            ..AppSettings::default()
        })
        .build()
}

fn layers_of(rig: &mut Rig) -> Vec<NormRect> {
    rig.saved_soon().expect("saved").scenes[0]
        .layers
        .iter()
        .map(|l| l.rect)
        .collect()
}

#[test]
fn a_dragged_layer_is_placed_saved_and_kept_inside_the_picture() {
    let mut rig = rig_editing();
    rig.core.apply(Action::Scene(SceneAction::SetLayerRect(
        1,
        [1000, 2000, 3000, 2500],
    )));
    let rects = layers_of(&mut rig);
    assert_eq!(rects[1], NormRect::new(0.1, 0.2, 0.3, 0.25));
    // Nonsense is pulled back inside.
    rig.core.apply(Action::Scene(SceneAction::SetLayerRect(
        2,
        [-9000, 99000, 500, 90000],
    )));
    let rects = layers_of(&mut rig);
    assert!(rects[2].x >= 0.0 && rects[2].y + rects[2].h <= 1.0 + 1e-6);
    // The base can be placed too; a layer that does not exist is ignored.
    rig.core.apply(Action::Scene(SceneAction::SetLayerRect(
        0,
        [0, 0, 5000, 5000],
    )));
    rig.core.apply(Action::Scene(SceneAction::SetLayerRect(
        9,
        [0, 0, 5000, 5000],
    )));
    assert_eq!(layers_of(&mut rig)[0], NormRect::new(0.0, 0.0, 0.5, 0.5));
}

#[test]
fn a_locked_layer_is_saved_locked_and_cannot_be_placed_but_can_still_be_moved_in_the_order() {
    let mut rig = rig_editing();
    rig.core.apply(Action::Scene(SceneAction::ToggleLock(1)));
    let saved = rig.saved_soon().expect("the lock is saved");
    assert!(saved.scenes[0].layers[1].locked);
    assert!(!saved.scenes[0].layers[2].locked);
    assert!(
        rig.core.model().scenes.layers[1].locked,
        "the list shows it"
    );
    let before = saved.scenes[0].layers[1].rect;
    rig.core.apply(Action::Scene(SceneAction::SetLayerRect(
        1,
        [1000, 2000, 3000, 2500],
    )));
    assert_eq!(
        rig.saved_soon().unwrap().scenes[0].layers[1].rect,
        before,
        "a locked layer does not move"
    );
    assert_eq!(rig.core.model().scenes.layers[1].rect, before);
    // Its place in the order is not its place in the picture.
    rig.core.apply(Action::Scene(SceneAction::MoveLayer(1, 1)));
    assert!(rig.core.model().scenes.layers[2].locked);
    // And it unlocks.
    rig.core.apply(Action::Scene(SceneAction::ToggleLock(2)));
    assert!(!rig.core.model().scenes.layers[2].locked);
    rig.core.apply(Action::Scene(SceneAction::ToggleLock(9)));
}

#[test]
fn layers_move_up_and_down_but_the_base_stays_at_the_bottom() {
    let mut rig = rig_editing();
    let names = |rig: &Rig| {
        rig.core
            .model()
            .scenes
            .layers
            .iter()
            .map(|l| l.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&rig), ["Base", "One", "Two"]);
    rig.core
        .apply(Action::Scene(SceneAction::SelectLayer(Some(1))));
    rig.core.apply(Action::Scene(SceneAction::MoveLayer(1, 1)));
    assert_eq!(names(&rig), ["Base", "Two", "One"]);
    assert_eq!(
        rig.core.model().scenes.selected,
        Some(2),
        "the selection follows"
    );
    // The top one cannot go higher, nor the base anywhere, nor under the base.
    rig.core.apply(Action::Scene(SceneAction::MoveLayer(2, 1)));
    rig.core.apply(Action::Scene(SceneAction::MoveLayer(0, 1)));
    rig.core.apply(Action::Scene(SceneAction::MoveLayer(1, -1)));
    assert_eq!(names(&rig), ["Base", "Two", "One"]);
    rig.core.apply(Action::Scene(SceneAction::MoveLayer(2, -1)));
    assert_eq!(names(&rig), ["Base", "One", "Two"]);
}

pub(super) fn rect_of_layer(rig: &Rig, i: usize) -> NormRect {
    rig.core.model().scenes.layers[i].rect
}

fn drag_layer(rig: &mut Rig, layer: usize, units: [i32; 4]) {
    rig.core.apply(Action::Scene(SceneAction::BeginEdit(layer)));
    rig.core
        .apply(Action::Scene(SceneAction::SetLayerRect(layer, units)));
}

#[test]
fn undo_puts_a_dragged_layer_back_one_gesture_at_a_time() {
    let mut rig = rig_editing();
    let start = rect_of_layer(&rig, 1);
    // One drag is many small changes but one step of undo.
    rig.core.apply(Action::Scene(SceneAction::BeginEdit(1)));
    for x in [2000, 2500, 3000] {
        rig.core.apply(Action::Scene(SceneAction::SetLayerRect(
            1,
            [x, 3000, 2000, 2000],
        )));
    }
    assert_eq!(rect_of_layer(&rig, 1), NormRect::new(0.3, 0.3, 0.2, 0.2));
    drag_layer(&mut rig, 1, [5000, 5000, 2000, 2000]);
    rig.core.on_key(crate::keys::KeyCommand::Undo);
    assert_eq!(rect_of_layer(&rig, 1), NormRect::new(0.3, 0.3, 0.2, 0.2));
    rig.core.on_key(crate::keys::KeyCommand::Undo);
    assert_eq!(rect_of_layer(&rig, 1), start);
    // Nothing more to undo: nothing happens.
    rig.core.on_key(crate::keys::KeyCommand::Undo);
    assert_eq!(rect_of_layer(&rig, 1), start);
}

#[test]
fn a_press_that_changes_nothing_leaves_nothing_to_undo() {
    let mut rig = rig_editing();
    drag_layer(&mut rig, 1, [5000, 5000, 2000, 2000]);
    // Holding a layer without moving it must not add a step.
    rig.core.apply(Action::Scene(SceneAction::BeginEdit(2)));
    rig.core.apply(Action::Scene(SceneAction::BeginEdit(1)));
    rig.core.on_key(crate::keys::KeyCommand::Undo);
    assert_ne!(rect_of_layer(&rig, 1), NormRect::new(0.5, 0.5, 0.2, 0.2));
}

#[test]
fn the_undo_list_is_forgotten_when_layers_move_or_the_scene_changes() {
    let mut rig = rig_editing();
    drag_layer(&mut rig, 1, [5000, 5000, 2000, 2000]);
    rig.core.apply(Action::Scene(SceneAction::MoveLayer(1, 1)));
    let after_move = rect_of_layer(&rig, 1);
    rig.core.on_key(crate::keys::KeyCommand::Undo);
    assert_eq!(
        rect_of_layer(&rig, 1),
        after_move,
        "the order changed: forget"
    );

    drag_layer(&mut rig, 1, [1000, 1000, 2000, 2000]);
    rig.core.apply(Action::Scene(SceneAction::Activate(None)));
    rig.core
        .apply(Action::Scene(SceneAction::Activate(Some(0))));
    let now = rect_of_layer(&rig, 1);
    rig.core.on_key(crate::keys::KeyCommand::Undo);
    assert_eq!(rect_of_layer(&rig, 1), now);
}

#[test]
fn the_arrows_push_the_chosen_layer_in_the_editor_and_undo_goes_back() {
    use crate::keys::KeyCommand;
    let mut rig = rig_editing();
    rig.core.apply(Action::Scene(SceneAction::ToggleEdit));
    rig.core
        .apply(Action::Scene(SceneAction::SelectLayer(Some(1))));
    let start = rect_of_layer(&rig, 1);
    rig.core.on_key(KeyCommand::SyncLater); // right arrow
    rig.core.on_key(KeyCommand::VolumeDown); // down arrow
    let moved = rect_of_layer(&rig, 1);
    assert!((moved.x - start.x - 0.005).abs() < 1e-5 && (moved.y - start.y - 0.005).abs() < 1e-5);
    assert_eq!(rig.core.model().volume, 1.0, "the volume did not change");
    rig.core.on_key(KeyCommand::Undo);
    rig.core.on_key(KeyCommand::Undo);
    assert_eq!(rect_of_layer(&rig, 1), start);
}

#[test]
fn without_the_editor_or_a_chosen_layer_the_arrows_do_what_they_always_did() {
    use crate::keys::KeyCommand;
    let mut rig = rig_editing();
    let start = rect_of_layer(&rig, 1);
    rig.core
        .apply(Action::Scene(SceneAction::SelectLayer(Some(1))));
    rig.core.on_key(KeyCommand::VolumeDown);
    assert!(
        rig.core.model().volume < 1.0,
        "the editor is off: it is the volume"
    );
    assert_eq!(rect_of_layer(&rig, 1), start);
    rig.core.apply(Action::Scene(SceneAction::ToggleEdit));
    rig.core
        .apply(Action::Scene(SceneAction::SelectLayer(None)));
    let volume = rig.core.model().volume;
    rig.core.on_key(KeyCommand::VolumeDown);
    assert!(
        rig.core.model().volume < volume,
        "nothing chosen: the volume again"
    );
}

#[test]
fn the_editor_is_a_switch_and_the_selection_only_counts_for_existing_layers() {
    let mut rig = rig_editing();
    assert!(!rig.core.model().scenes.editing);
    rig.core.apply(Action::Scene(SceneAction::ToggleEdit));
    assert!(rig.core.model().scenes.editing);
    rig.core
        .apply(Action::Scene(SceneAction::SelectLayer(Some(2))));
    assert_eq!(rig.core.model().scenes.selected, Some(2));
    rig.core
        .apply(Action::Scene(SceneAction::SelectLayer(Some(7))));
    assert_eq!(
        rig.core.model().scenes.selected,
        None,
        "there is no layer 7"
    );
    rig.core.apply(Action::Scene(SceneAction::ToggleEdit));
    assert!(!rig.core.model().scenes.editing);
}

#[test]
fn escape_closes_the_layer_editor_before_anything_else() {
    use crate::keys::KeyCommand;
    let mut rig = rig_editing();
    rig.core.apply(Action::ToggleStudio);
    rig.core.apply(Action::Scene(SceneAction::ToggleEdit));
    rig.core.on_key(KeyCommand::Escape);
    assert!(!rig.core.model().scenes.editing);
    assert!(
        rig.core.model().studio,
        "the panel stays until the next Esc"
    );
    rig.core.on_key(KeyCommand::Escape);
    assert!(!rig.core.model().studio);
}

#[test]
fn the_editor_is_off_when_no_scene_is_in_use() {
    let mut rig = Rig::new();
    rig.core.apply(Action::Scene(SceneAction::ToggleEdit));
    assert!(!rig.core.model().scenes.editing);
}

// -- Scene hotkeys ---------------------------------------------------------

fn press_scene_hotkey(rig: &mut Rig, slot: usize) {
    rig.replay.lock().unwrap().scene_hotkey = Some(slot);
    rig.advance(std::time::Duration::from_millis(50));
}

#[test]
fn the_scene_change_can_be_a_cut_and_the_choice_is_saved() {
    use dockview_domain::settings::SceneTransition;
    let mut rig = rig_with_scenes(&["One", "Two"]);
    assert_eq!(rig.core.model().scenes.transition, SceneTransition::Smooth);
    rig.core
        .apply(Action::SetSceneTransition(SceneTransition::Cut));
    assert_eq!(rig.core.model().scenes.transition, SceneTransition::Cut);
    assert_eq!(
        rig.saved_soon().expect("saved").scene_transition,
        SceneTransition::Cut
    );
    // With a cut nothing is left travelling after a change.
    rig.core
        .apply(Action::Scene(SceneAction::Activate(Some(1))));
    assert!(rig.core.scenes.transition.is_none());
}

#[test]
fn the_length_of_a_smooth_change_is_chosen_kept_in_range_and_saved() {
    let mut rig = rig_with_scenes(&["One", "Two"]);
    assert_eq!(rig.core.model().scenes.transition_ms, 400);
    rig.core.apply(Action::SetTransitionMs(800));
    assert_eq!(rig.core.model().scenes.transition_ms, 800);
    assert_eq!(rig.saved_soon().expect("saved").scene_transition_ms, 800);
    rig.core.apply(Action::SetTransitionMs(1));
    assert_eq!(
        rig.core.model().scenes.transition_ms,
        100,
        "not shorter than 100 ms"
    );
    rig.core.apply(Action::SetTransitionMs(60_000));
    assert_eq!(
        rig.core.model().scenes.transition_ms,
        2000,
        "not longer than 2 s"
    );
    // The change that follows lasts as long as was chosen.
    rig.core.apply(Action::SetTransitionMs(800));
    rig.core
        .apply(Action::Scene(SceneAction::Activate(Some(0))));
    rig.core.finish_transition();
    rig.core
        .apply(Action::Scene(SceneAction::Activate(Some(1))));
    let running = rig.core.scenes.transition.as_ref().expect("travelling");
    assert_eq!(
        running.transition.duration(),
        std::time::Duration::from_millis(800)
    );
}

#[test]
fn the_scene_hotkeys_pick_a_scene_the_plain_source_or_nothing() {
    let mut rig = rig_with_scenes(&["One", "Two"]);
    assert_eq!(rig.core.model().scenes.active, None);
    press_scene_hotkey(&mut rig, 2);
    assert_eq!(
        rig.core.model().scenes.active,
        Some(1),
        "F2 is the second scene"
    );
    press_scene_hotkey(&mut rig, 1);
    assert_eq!(rig.core.model().scenes.active, Some(0));
    // A slot with no scene behind it does nothing; pressing the same one again
    // keeps it.
    press_scene_hotkey(&mut rig, 7);
    assert_eq!(rig.core.model().scenes.active, Some(0));
    press_scene_hotkey(&mut rig, 1);
    assert_eq!(rig.core.model().scenes.active, Some(0));
    // F12 goes back to the plain source.
    press_scene_hotkey(&mut rig, 0);
    assert_eq!(rig.core.model().scenes.active, None);
}

#[test]
fn a_scene_hotkey_is_a_scene_change_and_gets_saved() {
    let mut rig = rig_with_scenes(&["One", "Two"]);
    press_scene_hotkey(&mut rig, 2);
    assert_eq!(rig.saved_soon().expect("saved").active_scene, Some(1));
}
