//! Scenes with nothing in them: a new scene starts empty, the first layer
//! added is the main source, and any layer, the first included, can be removed.

use dockview_domain::scene::{NormRect, Scene, SourceRef};
use dockview_domain::settings::AppSettings;
use dockview_domain::view::{Action, SceneAction};

use crate::testing::rig::Rig;

/// A rig with two capture cards and one empty scene in use.
fn rig_with_empty_scene() -> Rig {
    Rig::builder()
        .video(&[("Card one", "usb#one"), ("Card two", "usb#two")])
        .audio(&[("Card one audio", "audio:one")])
        .settings(AppSettings {
            scenes: vec![Scene::empty("Empty")],
            active_scene: Some(0),
            last_video_device_symbolic_link: Some("usb#one".into()),
            ..AppSettings::default()
        })
        .build()
}

fn card(link: &str) -> SourceRef {
    SourceRef::Capture {
        symbolic_link: link.into(),
    }
}

#[test]
fn a_new_scene_is_empty_and_has_no_main_source() {
    let mut rig = rig_with_empty_scene();
    rig.core.apply(Action::Scene(SceneAction::New));
    let scenes = &rig.core.model().scenes;
    assert_eq!(scenes.active, Some(1));
    assert!(scenes.layers.is_empty(), "it starts with nothing in it");
    assert!(rig.core.sources.idle);
    assert_eq!(rig.core.sources.main_source_ref(), None);
}

#[test]
fn the_first_layer_added_to_an_empty_scene_becomes_the_main_source() {
    let mut rig = rig_with_empty_scene();
    rig.core.apply(Action::Scene(SceneAction::AddCapture(1)));
    let scenes = &rig.core.model().scenes;
    assert_eq!(scenes.layers.len(), 1);
    assert_eq!(scenes.layers[0].rect, NormRect::FULL);
    assert!(!rig.core.sources.idle);
    assert_eq!(rig.core.sources.main_source_ref(), Some(card("usb#two")));
}

#[test]
fn removing_the_only_layer_leaves_the_scene_empty_and_stops_the_source() {
    let mut rig = rig_with_empty_scene();
    rig.core.apply(Action::Scene(SceneAction::AddCapture(0)));
    rig.core.apply(Action::Scene(SceneAction::RemoveLayer(0)));
    assert!(rig.core.model().scenes.layers.is_empty());
    assert!(rig.core.sources.idle);
    assert_eq!(rig.core.sources.main_source_ref(), None);
    let saved = rig.saved_soon().expect("saved");
    assert!(saved.scenes[0].layers.is_empty());
}

#[test]
fn removing_the_first_layer_makes_the_next_one_the_main_source() {
    let mut rig = rig_with_empty_scene();
    rig.core.apply(Action::Scene(SceneAction::AddCapture(0)));
    rig.core.apply(Action::Scene(SceneAction::AddCapture(1)));
    rig.core.apply(Action::Scene(SceneAction::RemoveLayer(0)));
    let scenes = &rig.core.model().scenes;
    assert_eq!(scenes.layers.len(), 1);
    assert_eq!(rig.core.sources.main_source_ref(), Some(card("usb#two")));
    assert_eq!(
        rig.saved_soon().expect("saved").scenes[0].base(),
        Some(&card("usb#two"))
    );
}

#[test]
fn going_back_to_no_scene_after_an_empty_one_brings_the_source_back() {
    let mut rig = rig_with_empty_scene();
    rig.core
        .apply(Action::Scene(SceneAction::Activate(Some(0))));
    assert!(rig.core.sources.idle);
    rig.core.apply(Action::Scene(SceneAction::Activate(None)));
    assert!(!rig.core.sources.idle);
    assert_eq!(rig.core.sources.main_source_ref(), Some(card("usb#one")));
}
