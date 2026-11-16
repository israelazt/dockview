//! The service on its own, with a fake machine.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dockview_domain::device::{AudioDevice, VideoDevice};
use dockview_domain::diagnostics::VideoState;
use dockview_domain::events::AppEvent;
use dockview_domain::frame::FrameSlot;
use dockview_domain::scene::SourceRef;
use dockview_domain::sources::{
    CaptureOptions, CaptureStatus, CaptureTarget, MonitorInfo, ScreenStatus, WindowInfo,
};

use super::{Maintained, SourcesService};
use crate::sources::SourceKind;
use crate::testing::sources::{FakeSources, SharedSources};

fn card(name: &str) -> VideoDevice {
    VideoDevice {
        friendly_name: name.into(),
        symbolic_link: format!("link:{name}"),
    }
}

fn input(name: &str) -> AudioDevice {
    AudioDevice {
        friendly_name: name.into(),
        device_id: format!("id:{name}"),
    }
}

fn window(title: &str, process_id: u32) -> WindowInfo {
    WindowInfo {
        handle: process_id as isize,
        title: title.into(),
        process_id,
    }
}

fn monitor(handle: isize) -> MonitorInfo {
    MonitorInfo {
        handle,
        device_name: format!(r"\\.\DISPLAY{handle}"),
        width: 1920,
        height: 1080,
        primary: handle == 1,
    }
}

/// A service over a fake machine, with the cards given.
fn service(cards: &[&str]) -> (SourcesService, SharedSources) {
    let (machine, world) = FakeSources::new();
    let service = SourcesService::new(
        Box::new(machine),
        cards.iter().map(|c| card(c)).collect(),
        Vec::new(),
        Arc::new(FrameSlot::new()),
    );
    (service, world)
}

#[test]
fn refreshing_lists_the_monitors_and_the_windows_of_other_programs_only() {
    let (mut service, world) = service(&[]);
    {
        let mut world = world.lock().unwrap();
        world.monitors = vec![monitor(1)];
        world.windows = vec![window("A game", 7), window("Dockview", std::process::id())];
    }
    assert!(service.monitors.is_empty() && service.windows.is_empty());

    service.refresh();

    assert_eq!(service.monitors.len(), 1);
    assert_eq!(
        service
            .windows
            .iter()
            .map(|w| w.title.as_str())
            .collect::<Vec<_>>(),
        ["A game"]
    );
}

#[test]
fn the_lists_are_stale_only_after_the_interval() {
    let (service, _) = service(&[]);
    let interval = Duration::from_millis(1500);
    let now = Instant::now();
    assert!(!service.lists_are_stale(now, interval));
    assert!(service.lists_are_stale(now + Duration::from_secs(2), interval));
    // A clock that went backwards never counts as stale.
    assert!(!service.lists_are_stale(now - Duration::from_millis(1), interval));
}

#[test]
fn the_audio_of_a_card_is_found_by_the_name_they_share() {
    let (machine, _) = FakeSources::new();
    let service = SourcesService::new(
        Box::new(machine),
        vec![card("UGREEN 25173"), card("Integrated Camera")],
        vec![
            input("Microphone (Integrated Camera)"),
            input("HDMI (UGREEN 25173)"),
        ],
        Arc::new(FrameSlot::new()),
    );
    assert_eq!(service.match_audio(Some(0)), Some(1));
    assert_eq!(service.match_audio(Some(1)), Some(0));
    assert_eq!(service.match_audio(None), None);
    assert_eq!(service.match_audio(Some(9)), None);
}

#[test]
fn opening_a_card_starts_it_with_the_options_and_reports_its_state() {
    let (mut service, world) = service(&["UGREEN"]);
    assert_eq!(service.video_state(), VideoState::NoDevice);

    service.open_card(
        "link:UGREEN",
        CaptureOptions {
            prefer_compressed: true,
            ..CaptureOptions::default()
        },
    );

    assert_eq!(service.video_state(), VideoState::Running);
    assert!(service.native_mode().is_some());
    let capture = world.lock().unwrap().last_capture();
    assert_eq!(capture.lock().unwrap().link, "link:UGREEN");
    assert!(capture.lock().unwrap().options.prefer_compressed);

    service.release();
    assert!(!capture.lock().unwrap().alive, "the card must be let go");
    assert_eq!(service.video_state(), VideoState::NoDevice);
    assert_eq!(service.native_mode(), None);
}

#[test]
fn a_card_that_drops_out_is_opened_again_after_a_pause_with_the_options_given() {
    let (mut service, world) = service(&["UGREEN"]);
    service.video_index = Some(0);
    service.open_card("link:UGREEN", CaptureOptions::default());
    world.lock().unwrap().last_capture().lock().unwrap().status = CaptureStatus::Disconnected {
        reason: "unplugged".into(),
    };
    let start = Instant::now();
    let compressed = CaptureOptions {
        prefer_compressed: true,
        ..CaptureOptions::default()
    };

    assert_eq!(service.maintain(start, compressed), Maintained::Fine);
    assert_eq!(world.lock().unwrap().captures.len(), 1, "waits first");
    service.maintain(start + Duration::from_secs(3), compressed);

    let world = world.lock().unwrap();
    assert_eq!(world.captures.len(), 2);
    assert_eq!(world.open_captures(), 1);
    assert!(
        world
            .last_capture()
            .lock()
            .unwrap()
            .options
            .prefer_compressed
    );
}

#[test]
fn a_screen_that_ended_or_failed_is_reported_and_a_healthy_one_is_not() {
    let (mut service, world) = service(&[]);
    service.open_screen(CaptureTarget::Monitor(1), SourceKind::Monitor(1));
    assert_eq!(service.source_kind, SourceKind::Monitor(1));
    assert_eq!(
        service.maintain(Instant::now(), CaptureOptions::default()),
        Maintained::Fine
    );

    let screen = world.lock().unwrap().last_screen();
    screen.lock().unwrap().status = ScreenStatus::Ended("closed".into());
    assert_eq!(
        service.maintain(Instant::now(), CaptureOptions::default()),
        Maintained::ScreenLost(AppEvent::SourceLost)
    );
    screen.lock().unwrap().status = ScreenStatus::Failed("denied".into());
    assert_eq!(
        service.maintain(Instant::now(), CaptureOptions::default()),
        Maintained::ScreenLost(AppEvent::SourceFailed)
    );
}

#[test]
fn the_main_source_can_be_saved_as_a_scene_layer_whatever_it_is() {
    let (mut service, world) = service(&["UGREEN"]);
    // Nothing selected yet.
    assert_eq!(service.main_source_ref(), None);

    service.video_index = Some(0);
    assert_eq!(
        service.main_source_ref(),
        Some(SourceRef::Capture {
            symbolic_link: "link:UGREEN".into()
        })
    );

    world.lock().unwrap().monitors = vec![monitor(1)];
    world.lock().unwrap().windows = vec![window("A game", 7)];
    service.refresh();
    service.source_kind = SourceKind::Monitor(1);
    assert_eq!(
        service.main_source_ref(),
        Some(SourceRef::Monitor {
            device_name: r"\\.\DISPLAY1".into()
        })
    );
    service.source_kind = SourceKind::Window { handle: 7, pid: 7 };
    assert_eq!(
        service.main_source_ref(),
        Some(SourceRef::Window {
            title: "A game".into()
        })
    );
    assert_eq!(service.find_window("a gam"), Some((7, 7)));
    assert_eq!(service.find_window("nothing like it"), None);
}

// -- The sound -----------------------------------------------------------

use dockview_domain::audio_status::AudioStatus;
use dockview_domain::pcm::AudioTap;
use dockview_domain::ports::Loopback;

use super::audio::AudioCheck;

fn with_sound() -> (SourcesService, SharedSources) {
    let (machine, world) = FakeSources::new();
    let service = SourcesService::new(
        Box::new(machine),
        vec![card("UGREEN")],
        vec![input("HDMI (UGREEN)"), input("Microphone")],
        Arc::new(FrameSlot::new()),
    );
    (service, world)
}

fn tap() -> Option<Arc<AudioTap>> {
    Some(Arc::new(AudioTap::new(4)))
}

#[test]
fn playing_an_input_starts_it_with_the_volume_and_sync_and_says_which_one() {
    let (mut service, world) = with_sound();
    assert!(!service.has_audio());

    let id = service.play(Some(1), 0.4, 30, tap());

    assert_eq!(id.as_deref(), Some("id:Microphone"));
    assert!(service.has_audio());
    let playback = world.lock().unwrap().last_playback();
    let playback = playback.lock().unwrap();
    assert_eq!(playback.device_id, "id:Microphone");
    assert_eq!((playback.volume, playback.offset_ms), (0.4, 30));
    assert!(playback.tapped, "the recorder is fed");
    assert_eq!(service.audio_index, Some(1));
}

#[test]
fn playing_another_input_lets_go_of_the_first_and_none_stops_the_sound() {
    let (mut service, world) = with_sound();
    service.play(Some(0), 1.0, 0, None);
    service.play(Some(1), 1.0, 0, None);
    assert_eq!(world.lock().unwrap().open_playbacks(), 1);

    assert_eq!(service.play(None, 1.0, 0, None), None);
    assert!(!service.has_audio());
    assert_eq!(world.lock().unwrap().open_playbacks(), 0);
}

#[test]
fn while_a_screen_is_captured_the_choice_is_noted_but_nothing_is_played() {
    let (mut service, world) = with_sound();
    service.source_kind = SourceKind::Monitor(1);
    assert_eq!(service.play(Some(0), 1.0, 0, None), None);
    assert_eq!(service.audio_index, Some(0));
    assert!(world.lock().unwrap().playbacks.is_empty());
}

#[test]
fn volume_and_sync_reach_the_playing_sound() {
    let (mut service, world) = with_sound();
    service.set_volume(0.5); // nothing playing: harmless
    service.play(Some(0), 1.0, 0, None);
    service.set_volume(0.25);
    service.set_offset_ms(-20);
    let playback = world.lock().unwrap().last_playback();
    {
        let playback = playback.lock().unwrap();
        assert_eq!((playback.volume, playback.offset_ms), (0.25, -20));
    }
    assert_eq!(service.audio_ok(), Some(true));
    assert_eq!(service.audio_buffered_ms(), Some(50));
}

#[test]
fn lost_sound_is_reported_once_restarted_after_a_pause_and_reported_back() {
    let (mut service, world) = with_sound();
    service.play(Some(0), 1.0, 0, None);
    let start = Instant::now();
    assert_eq!(service.check_audio(start), AudioCheck::default());

    world.lock().unwrap().last_playback().lock().unwrap().status =
        AudioStatus::Disconnected("unplugged".into());
    assert_eq!(service.audio_ok(), Some(false));
    let first = service.check_audio(start);
    assert_eq!(first.event, Some(AppEvent::AudioLost));
    assert!(!first.restart, "it waits first");
    assert_eq!(
        service.check_audio(start + Duration::from_secs(1)).event,
        None
    );
    let later = service.check_audio(start + Duration::from_secs(3));
    assert!(later.restart && later.event.is_none());

    // The caller starts it again; the new one runs.
    service.play(Some(0), 1.0, 0, None);
    let back = service.check_audio(start + Duration::from_secs(4));
    assert_eq!(back.event, Some(AppEvent::AudioBack));
    assert!(!back.restart);
}

#[test]
fn sound_with_no_input_selected_is_waited_for_not_restarted() {
    let (mut service, world) = with_sound();
    service.play(Some(0), 1.0, 0, None);
    world.lock().unwrap().last_playback().lock().unwrap().status =
        AudioStatus::Failed("gone".into());
    service.audio_index = None; // the input vanished from the list
    let start = Instant::now();
    service.check_audio(start);
    assert!(!service.check_audio(start + Duration::from_secs(5)).restart);
}

#[test]
fn the_recorders_sound_comes_from_the_card_or_from_the_loopback_of_a_screen() {
    let (mut service, world) = with_sound();
    service.play(Some(0), 1.0, 0, None);
    service.route_audio(tap());
    assert!(world.lock().unwrap().last_playback().lock().unwrap().tapped);
    assert!(world.lock().unwrap().loopbacks.is_empty());

    service.source_kind = SourceKind::Window { handle: 7, pid: 42 };
    service.route_audio(tap());
    let world = world.lock().unwrap();
    assert_eq!(world.loopbacks.len(), 1);
    assert_eq!(
        world.loopbacks[0].lock().unwrap().source,
        Loopback::Process(42)
    );
}
