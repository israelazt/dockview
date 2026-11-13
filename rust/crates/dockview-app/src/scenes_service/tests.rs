//! The service on its own, with a fake machine.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dockview_domain::frame::FrameSlot;
use dockview_domain::scene::{NormRect, Scene, SceneLayer, SourceRef};
use dockview_domain::sources::{CaptureStatus, CaptureTarget, ScreenOptions, ScreenStatus};

use super::{Overlay, OverlayRunning, SceneEnv, ScenesService};
use crate::sources::Reconnect;
use crate::testing::sources::{FakeSources, SharedSources};

fn source(name: &str) -> SourceRef {
    SourceRef::Capture {
        symbolic_link: format!("link:{name}"),
    }
}

fn corner() -> NormRect {
    NormRect::new(0.7, 0.7, 0.28, 0.28)
}

/// What the service runs against: a fake machine and the two slots.
struct World {
    machine: FakeSources,
    sources: SharedSources,
    input: Arc<FrameSlot>,
    output: Arc<FrameSlot>,
}

impl World {
    fn new() -> Self {
        let (machine, sources) = FakeSources::new();
        let input = Arc::new(FrameSlot::new());
        let output = Arc::new(FrameSlot::new());
        input.set_forward(Some(Arc::clone(&output)));
        Self {
            machine,
            sources,
            input,
            output,
        }
    }

    fn env(&self) -> SceneEnv<'_> {
        SceneEnv {
            machine: &self.machine,
            input_slot: &self.input,
            slot: &self.output,
        }
    }

    /// An overlay showing a monitor, with its fake screen capture.
    fn screen_overlay(&self, index: usize, name: &str) -> Overlay {
        use dockview_domain::ports::SourceMachine;
        let slot = Arc::new(FrameSlot::new());
        let capture = self.machine.start_screen(
            CaptureTarget::Monitor(1),
            ScreenOptions::default(),
            Arc::clone(&slot),
        );
        Overlay {
            running: OverlayRunning::Screen(capture),
            slot,
            layer: SceneLayer::new(source(name), corner()),
            index,
        }
    }

    /// An overlay showing a capture card, with its fake capture.
    fn card_overlay(&self, index: usize, name: &str) -> Overlay {
        use dockview_domain::ports::SourceMachine;
        let slot = Arc::new(FrameSlot::new());
        let link = format!("link:{name}");
        let capture = self.machine.start_capture(
            link.clone(),
            dockview_domain::sources::CaptureOptions::default(),
            Arc::clone(&slot),
        );
        Overlay {
            running: OverlayRunning::Capture {
                capture,
                link,
                reconnect: Reconnect::new(),
            },
            slot,
            layer: SceneLayer::new(source(name), corner()),
            index,
        }
    }
}

fn running_with_base(service: &mut ScenesService) {
    service.base = Some(SceneLayer::new(source("main"), NormRect::FULL));
}

#[test]
fn the_layers_are_the_base_from_the_input_slot_then_the_overlays() {
    let world = World::new();
    let mut service = ScenesService::new();
    service.overlays.push(world.screen_overlay(1, "a"));
    service.overlays.push(world.card_overlay(2, "b"));
    let base = SceneLayer::new(source("main"), NormRect::FULL);

    let layers = service.layers(&base, &world.input);

    assert_eq!(layers.len(), 3);
    assert!(Arc::ptr_eq(&layers[0].slot, &world.input));
    assert!(Arc::ptr_eq(&layers[1].slot, &service.overlays[0].slot));
    assert!(Arc::ptr_eq(&layers[2].slot, &service.overlays[1].slot));
    assert_eq!(layers[1].rect, corner());
    assert!(layers.iter().all(|l| l.opacity == 1.0));
}

#[test]
fn the_running_scene_is_the_base_and_the_overlays_in_their_saved_order() {
    let world = World::new();
    let mut service = ScenesService::new();
    // Nothing running and no main source: nothing to describe.
    assert!(service.running_scene(None).is_none());
    // Nothing running: the main source alone.
    let alone = service.running_scene(Some(source("main"))).unwrap();
    assert_eq!(alone.layers.len(), 1);

    running_with_base(&mut service);
    service.overlays.push(world.screen_overlay(2, "second"));
    service.overlays.push(world.screen_overlay(1, "first"));
    let scene = service.running_scene(None).unwrap();
    let names: Vec<_> = scene.layers.iter().map(|l| l.source.clone()).collect();
    assert_eq!(names, [source("main"), source("first"), source("second")]);
}

#[test]
fn a_single_full_picture_layer_needs_no_compositor() {
    let world = World::new();
    let mut service = ScenesService::new();
    running_with_base(&mut service);

    service.sync_compositor(&world.env());

    assert!(service.compositor.is_none());
    assert!(world.sources.lock().unwrap().compositors.is_empty());
}

#[test]
fn an_overlay_starts_the_compositor_and_later_changes_update_it() {
    let world = World::new();
    let mut service = ScenesService::new();
    running_with_base(&mut service);
    service.overlays.push(world.screen_overlay(1, "a"));

    service.sync_compositor(&world.env());

    {
        let sources = world.sources.lock().unwrap();
        assert_eq!(sources.compositors.len(), 1);
        assert_eq!(sources.compositors[0].lock().unwrap().layers, 2);
    }
    // One more layer goes to the same compositor, without a restart.
    service.overlays.push(world.card_overlay(2, "b"));
    service.sync_compositor(&world.env());
    let sources = world.sources.lock().unwrap();
    assert_eq!(sources.compositors.len(), 1);
    assert_eq!(sources.compositors[0].lock().unwrap().layers, 3);
}

#[test]
fn going_back_to_a_plain_picture_drops_the_compositor_and_feeds_the_viewer_again() {
    let world = World::new();
    let mut service = ScenesService::new();
    running_with_base(&mut service);
    service.overlays.push(world.screen_overlay(1, "a"));
    service.sync_compositor(&world.env());
    assert!(service.compositor.is_some());

    service.overlays.clear();
    service.sync_compositor(&world.env());

    assert!(service.compositor.is_none());
    assert!(
        !world.sources.lock().unwrap().compositors[0]
            .lock()
            .unwrap()
            .alive,
        "the compositor must be stopped"
    );
}

#[test]
fn stopping_the_scene_drops_everything_and_feeds_the_viewer_again() {
    let world = World::new();
    let mut service = ScenesService::new();
    running_with_base(&mut service);
    service.overlays.push(world.screen_overlay(1, "a"));
    service.sync_compositor(&world.env());

    service.stop(&world.env());

    assert!(service.compositor.is_none() && service.base.is_none());
    assert!(service.overlays.is_empty() && service.transition.is_none());
    assert!(!service.has_screen_overlay());
}

#[test]
fn layers_are_shown_hidden_and_removed_with_the_numbering_kept() {
    let world = World::new();
    let mut service = ScenesService::new();
    running_with_base(&mut service);
    for (index, name) in [(1, "a"), (2, "b"), (3, "c")] {
        service.overlays.push(world.screen_overlay(index, name));
    }

    service.set_layer_visible(0, false);
    service.set_layer_visible(2, false);
    assert!(!service.base.as_ref().unwrap().visible);
    assert!(!service.overlays[1].layer.visible && service.overlays[0].layer.visible);

    service.remove_overlay(2);
    let indexes: Vec<_> = service.overlays.iter().map(|o| o.index).collect();
    assert_eq!(indexes, [1, 2], "the layer after the removed one moves up");
}

#[test]
fn a_screen_overlay_that_ended_is_taken_off_and_a_card_that_dropped_is_reopened() {
    let world = World::new();
    let mut service = ScenesService::new();
    service.overlays.push(world.screen_overlay(1, "screen"));
    service.overlays.push(world.card_overlay(2, "card"));
    let start = Instant::now();
    assert!(!service.maintain(start, &world.machine), "all healthy");

    // The screen ends: it is taken off.
    world
        .sources
        .lock()
        .unwrap()
        .last_screen()
        .lock()
        .unwrap()
        .status = ScreenStatus::Ended("closed".into());
    assert!(service.maintain(start, &world.machine));
    assert_eq!(service.overlays.len(), 1);
    assert!(!service.has_screen_overlay());

    // The card drops out: reopened after the pause, not before.
    world
        .sources
        .lock()
        .unwrap()
        .last_capture()
        .lock()
        .unwrap()
        .status = CaptureStatus::Disconnected {
        reason: "unplugged".into(),
    };
    assert!(!service.maintain(start, &world.machine));
    assert_eq!(world.sources.lock().unwrap().captures.len(), 1);
    assert!(!service.maintain(start + Duration::from_secs(3), &world.machine));
    assert_eq!(world.sources.lock().unwrap().captures.len(), 2);
}

#[test]
fn a_change_between_scenes_feeds_the_compositor_and_can_be_finished_at_once() {
    let world = World::new();
    let mut service = ScenesService::new();
    running_with_base(&mut service);
    let old = Scene::with_base("old", source("main"));
    let mut new = Scene::with_base("new", source("main"));
    new.add_layer(source("a"));
    // The new scene's overlay is already running (Core started it).
    service.overlays.push(world.screen_overlay(1, "a"));

    service.begin_transition(&old, &new, Vec::new());
    assert!(service.transition.is_some());
    // Mid-change the compositor is fed by the change, not by `sync_compositor`.
    let finished = service.tick_transition(&world.env(), false);
    assert!(!finished);
    assert_eq!(world.sources.lock().unwrap().compositors.len(), 1);
    service.sync_compositor(&world.env());
    assert_eq!(world.sources.lock().unwrap().compositors.len(), 1);

    assert!(service.finish_transition(&world.env()));
    assert!(service.transition.is_none());
    assert!(
        !service.finish_transition(&world.env()),
        "nothing left to finish"
    );
    assert!(!service.leaving_has_screen());
}

#[test]
fn overlays_that_leave_keep_running_until_the_change_ends() {
    let world = World::new();
    let mut service = ScenesService::new();
    running_with_base(&mut service);
    let mut old = Scene::with_base("old", source("main"));
    old.add_layer(source("gone"));
    let new = Scene::with_base("new", source("main"));

    service.begin_transition(&old, &new, vec![world.screen_overlay(1, "gone")]);
    assert!(service.leaving_has_screen());

    service.finish_transition(&world.env());
    assert!(!service.leaving_has_screen());
    assert!(
        !world.sources.lock().unwrap().screens[0]
            .lock()
            .unwrap()
            .alive
    );
}
