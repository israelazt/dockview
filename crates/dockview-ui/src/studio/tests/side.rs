//! The cards of the left column: the scenes and, under them, the layers of the
//! scene in use (or the sources when there is none).

use dockview_domain::view::{Action, LayerKind, LayerView, SceneAction, ScenesView};

use super::*;
use crate::studio::sections::Scope;

fn layer(name: &str, kind: LayerKind) -> LayerView {
    LayerView {
        name: name.into(),
        kind,
        visible: true,
        locked: false,
        rect: dockview_domain::scene::NormRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
    }
}

fn scene(action: SceneAction) -> Action {
    Action::Scene(action)
}

fn side_rig(scenes: ScenesView) -> Rig {
    let mut rig = Rig::new();
    rig.panel.set_scope(Scope::Side);
    rig.model.scenes = scenes;
    rig.frame(0.016);
    rig.frame(0.016);
    rig
}

fn two_layers() -> ScenesView {
    ScenesView {
        names: vec!["Juego".into()],
        layer_counts: vec![3],
        active: Some(0),
        layers: vec![
            layer("UGREEN", LayerKind::Capture),
            layer("Pantalla 2", LayerKind::Monitor),
            layer("Chat", LayerKind::Window),
        ],
        can_add: false,
        selected: Some(1),
        ..ScenesView::default()
    }
}

#[test]
fn the_cards_are_always_open_and_nothing_folds() {
    let rig = side_rig(two_layers());
    assert!(rig.panel.shows(StudioSection::Scenes) && rig.panel.shows(StudioSection::Sources));
    assert!(
        !rig.panel
            .hits
            .iter()
            .any(|(h, _)| matches!(h, PanelHit::Header(_))),
        "the titles are not buttons"
    );
}

#[test]
fn the_scenes_card_has_the_new_scene_button_and_a_row_per_scene() {
    let rig = side_rig(two_layers());
    let items = &rig.panel.items;
    assert!(items.contains(&scene(SceneAction::New)));
    assert!(items.contains(&scene(SceneAction::Activate(None))));
    assert!(items.contains(&scene(SceneAction::Activate(Some(0)))));
}

#[test]
fn with_a_scene_in_use_the_second_card_has_its_layers_and_not_the_sources() {
    let rig = side_rig(two_layers());
    let items = &rig.panel.items;
    for i in 0..3 {
        assert!(
            items.contains(&scene(SceneAction::SelectLayer(Some(i)))),
            "{i}"
        );
        assert!(items.contains(&scene(SceneAction::ToggleLayer(i))), "{i}");
    }
    assert!(
        !items.iter().any(|a| matches!(a, Action::SelectVideo(_))),
        "the sources are not listed"
    );
}

#[test]
fn the_footer_acts_on_the_chosen_layer_only() {
    let rig = side_rig(two_layers());
    let items = &rig.panel.items;
    // The layer over the base, with one above it: it can go up, not down.
    assert!(items.contains(&scene(SceneAction::MoveLayer(1, 1))));
    assert!(!items.contains(&scene(SceneAction::MoveLayer(1, -1))));
    assert!(items.contains(&scene(SceneAction::RemoveLayer(1))));
    // Only the chosen layer (the first over the base) has its buttons here.
    assert!(!items.contains(&scene(SceneAction::RemoveLayer(0))));
}

#[test]
fn the_eye_of_a_layer_does_not_also_choose_it() {
    let mut rig = side_rig(two_layers());
    let eye = rig
        .panel
        .items
        .iter()
        .position(|a| *a == scene(SceneAction::ToggleLayer(2)))
        .unwrap();
    rig.click_on(PanelHit::Item(eye));
    assert_eq!(
        rig.panel.take_actions(),
        [scene(SceneAction::ToggleLayer(2))]
    );
}

#[test]
fn without_a_scene_the_second_card_lists_the_sources() {
    let mut rig = Rig::new();
    rig.panel.set_scope(Scope::Side);
    rig.model.video_devices = vec!["Card A".into(), "Card B".into()];
    rig.frame(0.016);
    rig.frame(0.016);
    assert!(rig
        .panel
        .items
        .iter()
        .any(|a| matches!(a, Action::SelectVideo(_))));
}

#[test]
fn the_card_is_as_tall_as_its_rows_say() {
    let m = {
        let mut m = model();
        m.scenes = two_layers();
        m
    };
    let in_card = StudioSection::Sources.body_height_in(
        Scope::Side,
        &mut Ui::new(),
        &m,
        &Stats::default(),
        200.0,
    );
    let as_section = section_height(StudioSection::Sources, &m);
    assert!(in_card > 0.0);
    assert_ne!(
        in_card, as_section,
        "the same section differs by where it lives"
    );
}

#[test]
fn every_layer_has_a_lock_and_all_but_the_base_a_grip() {
    let rig = side_rig(two_layers());
    for i in 0..3 {
        assert!(
            rig.panel.items.contains(&scene(SceneAction::ToggleLock(i))),
            "{i}"
        );
    }
    let grips = |i| rig.panel.hits.iter().any(|(h, _)| *h == PanelHit::Grip(i));
    assert!(!grips(0), "the base stays at the bottom");
    assert!(grips(1) && grips(2));
}

#[test]
fn the_top_layer_is_listed_first() {
    let rig = side_rig(two_layers());
    let y_of = |i: usize| {
        let idx = rig
            .panel
            .items
            .iter()
            .position(|a| *a == scene(SceneAction::SelectLayer(Some(i))))
            .unwrap();
        rig.rect_of(PanelHit::Item(idx)).y
    };
    assert!(y_of(2) < y_of(1) && y_of(1) < y_of(0));
}

#[test]
fn dragging_a_grip_down_lowers_the_layer_when_it_is_let_go() {
    let mut rig = side_rig(two_layers());
    let (gx, gy) = rig.rect_of(PanelHit::Grip(2)).center();
    let (top, rows) = rig.panel.layer_rows.expect("drawn");
    assert_eq!(rows, 3);
    rig.move_to(gx, gy);
    rig.pointer.press();
    rig.frame(0.016);
    // Down to the second row (where layer 1 is), then let go.
    rig.pointer.pos = Some((gx, top + 1.5 * layers_card::LAYER_ROW_H));
    rig.frame(0.016);
    assert_eq!(
        rig.panel.reorder,
        Some((2, 1)),
        "a line shows where it lands"
    );
    rig.pointer.release();
    rig.frame(0.016);
    assert_eq!(
        rig.panel.take_actions(),
        [scene(SceneAction::MoveLayer(2, -1))]
    );
    assert_eq!(rig.panel.reorder, None);
}

#[test]
fn a_drag_that_ends_where_it_began_moves_nothing() {
    let mut rig = side_rig(two_layers());
    let (gx, gy) = rig.rect_of(PanelHit::Grip(1)).center();
    rig.move_to(gx, gy);
    rig.pointer.press();
    rig.frame(0.016);
    rig.pointer.release();
    rig.frame(0.016);
    assert_eq!(rig.panel.take_actions(), []);
}

#[test]
fn where_a_drag_lands_never_touches_the_base() {
    let rows = 4;
    let at = |shown: f32, layer| {
        layers_card::reorder_target(shown * layers_card::LAYER_ROW_H + 5.0, 0.0, rows, layer)
    };
    assert_eq!(at(0.0, 1), 3, "the first row is the top layer");
    assert_eq!(at(2.0, 3), 1);
    assert_eq!(at(3.0, 3), 1, "the base row is not a place to land");
    assert_eq!(at(9.0, 2), 1, "below the list is the lowest place");
    assert_eq!(
        layers_card::reorder_target(-30.0, 0.0, rows, 2),
        3,
        "above is the top"
    );
    assert_eq!(at(1.0, 0), 0, "the base never moves");
}

#[test]
fn a_move_of_several_places_is_one_step_at_a_time() {
    assert_eq!(
        layers_card::reorder_actions(1, 4),
        [
            scene(SceneAction::MoveLayer(1, 1)),
            scene(SceneAction::MoveLayer(2, 1)),
            scene(SceneAction::MoveLayer(3, 1)),
        ]
    );
    assert_eq!(
        layers_card::reorder_actions(3, 1),
        [
            scene(SceneAction::MoveLayer(3, -1)),
            scene(SceneAction::MoveLayer(2, -1)),
        ]
    );
    assert!(layers_card::reorder_actions(2, 2).is_empty());
}
