//! The layer editor's canvas tools on `Core`: the snap and grid switches and
//! "Fit".

use dockview_domain::scene::NormRect;
use dockview_domain::view::{Action, SceneAction};

use super::scene_tests::{rect_of_layer, rig_editing, rig_with_scenes};

#[test]
fn snap_and_grid_are_switches_that_are_saved() {
    let mut rig = rig_with_scenes(&["One"]);
    assert!(rig.core.model().scenes.snap && !rig.core.model().scenes.grid);
    rig.core.apply(Action::ToggleCanvasSnap);
    rig.core.apply(Action::ToggleCanvasGrid);
    assert!(!rig.core.model().scenes.snap && rig.core.model().scenes.grid);
    let saved = rig.saved_soon().expect("saved");
    assert!(!saved.canvas_snap && saved.canvas_grid);
    rig.core.apply(Action::ToggleCanvasSnap);
    assert!(rig.core.model().scenes.snap);
}

#[test]
fn fit_stretches_the_chosen_layer_over_the_picture_and_undo_puts_it_back() {
    use crate::keys::KeyCommand;
    let mut rig = rig_editing();
    rig.core.apply(Action::Scene(SceneAction::ToggleEdit));
    // Nothing chosen: nothing happens.
    rig.core.apply(Action::Scene(SceneAction::FitLayer));
    let start = rect_of_layer(&rig, 1);
    rig.core
        .apply(Action::Scene(SceneAction::SelectLayer(Some(1))));
    rig.core.apply(Action::Scene(SceneAction::FitLayer));
    assert_eq!(rect_of_layer(&rig, 1), NormRect::new(0.0, 0.0, 1.0, 1.0));
    rig.core.on_key(KeyCommand::Undo);
    assert_eq!(rect_of_layer(&rig, 1), start);
}

#[test]
fn fit_leaves_a_locked_layer_alone() {
    let mut rig = rig_editing();
    rig.core.apply(Action::Scene(SceneAction::ToggleLock(1)));
    let start = rect_of_layer(&rig, 1);
    rig.core
        .apply(Action::Scene(SceneAction::SelectLayer(Some(1))));
    rig.core.apply(Action::Scene(SceneAction::FitLayer));
    assert_eq!(rect_of_layer(&rig, 1), start);
}
