//! Going live in `Core`, on fakes.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dockview_domain::broadcast::{SecretStore, StreamState};
use dockview_domain::events::AppEvent;
use dockview_domain::view::Action;

use crate::testing::rig::Rig;

fn tick(rig: &mut Rig) {
    rig.advance(Duration::from_millis(50));
}

fn ready() -> Rig {
    let mut rig = Rig::new();
    rig.core
        .apply(Action::SetStreamUrl("rtmp://live.example/app".into()));
    rig.core.apply(Action::SetStreamKey("live_abc".into()));
    rig.core.take_events();
    rig
}

#[test]
fn it_says_what_is_missing_before_going_live() {
    let mut rig = Rig::new();
    rig.core.apply(Action::ToggleStream);
    assert_eq!(rig.core.take_events(), [AppEvent::StreamBadAddress]);
    rig.core
        .apply(Action::SetStreamUrl("rtmp://live.example/app".into()));
    rig.core.apply(Action::ToggleStream);
    assert_eq!(rig.core.take_events(), [AppEvent::StreamNoKey]);
    assert!(rig.stream.lock().unwrap().requests.is_empty());
}

#[test]
fn the_address_is_saved_and_the_key_is_not() {
    let mut rig = ready();
    let saved = rig.saved_soon().unwrap();
    assert_eq!(saved.stream.url, "rtmp://live.example/app");
    assert!(!saved.to_json().unwrap().contains("live_abc"));
    assert_eq!(rig.secrets.get("stream-key").as_deref(), Some("live_abc"));
    let model = rig.core.model();
    assert!(model.stream.has_key);
    assert_eq!(model.stream.address, "rtmp://live.example/app");
}

#[test]
fn the_button_goes_live_and_ends_the_stream() {
    let mut rig = ready();
    rig.core.apply(Action::ToggleStream);
    let requests = rig.stream.lock().unwrap().requests.clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].key, "live_abc");
    assert_eq!(rig.core.model().stream.state, StreamState::Connecting);

    rig.stream.lock().unwrap().state = StreamState::Live;
    tick(&mut rig);
    assert_eq!(rig.core.take_events(), [AppEvent::StreamLive]);
    assert_eq!(rig.core.model().stream.state, StreamState::Live);

    rig.core.apply(Action::ToggleStream);
    assert_eq!(rig.core.take_events(), [AppEvent::StreamStopped]);
    assert_eq!(rig.stream.lock().unwrap().alive, 0);
    assert_eq!(rig.core.model().stream.state, StreamState::Offline);
}

#[test]
fn a_dropped_connection_and_its_return_are_told_and_a_failure_ends_the_stream() {
    let mut rig = ready();
    rig.core.apply(Action::ToggleStream);
    rig.stream.lock().unwrap().state = StreamState::Live;
    tick(&mut rig);
    rig.stream.lock().unwrap().state = StreamState::Reconnecting;
    tick(&mut rig);
    rig.stream.lock().unwrap().state = StreamState::Live;
    tick(&mut rig);
    rig.stream.lock().unwrap().state = StreamState::Failed;
    tick(&mut rig);
    assert_eq!(
        rig.core.take_events(),
        [
            AppEvent::StreamLive,
            AppEvent::StreamReconnecting,
            AppEvent::StreamBack,
            AppEvent::StreamFailed
        ]
    );
    assert_eq!(rig.stream.lock().unwrap().alive, 0);
    assert_eq!(rig.core.model().stream.state, StreamState::Offline);
}

#[test]
fn a_machine_that_cannot_stream_is_told_to_the_user() {
    let mut rig = ready();
    rig.stream.lock().unwrap().refuse = Some("no hardware encoder".into());
    rig.core.apply(Action::ToggleStream);
    assert_eq!(rig.core.take_events(), [AppEvent::StreamStartFailed]);
}

fn playback_tap(rig: &Rig) -> Arc<dockview_domain::pcm::AudioTap> {
    let sources = rig.sources.lock().unwrap();
    let playback = sources.last_playback();
    let state = playback.lock().unwrap();
    Arc::clone(state.tap.as_ref().expect("the sound is fed to a queue"))
}

#[test]
fn the_sound_goes_to_the_replay_and_also_to_the_stream() {
    let mut rig = ready();
    rig.core.apply(Action::ToggleStream);
    let stream_audio = rig.stream.lock().unwrap().audio.clone().unwrap();
    let tap = playback_tap(&rig);
    tap.offer(Instant::now(), &[0.1; 20]);
    assert!(stream_audio.take(Duration::ZERO).is_some());

    // Ending the stream stops the copies.
    rig.core.apply(Action::ToggleStream);
    tap.offer(Instant::now(), &[0.1; 20]);
    assert!(stream_audio.take(Duration::ZERO).is_none());
}

#[test]
fn with_the_replay_off_the_sound_still_reaches_the_stream() {
    let mut rig = ready();
    rig.core.apply(Action::ToggleReplay); // off
    rig.core.apply(Action::ToggleStream);
    let stream_audio = rig.stream.lock().unwrap().audio.clone().unwrap();
    let tap = playback_tap(&rig);
    tap.offer(Instant::now(), &[0.1; 20]);
    assert!(stream_audio.take(Duration::ZERO).is_some());
}

#[test]
fn the_frame_rate_and_the_bitrate_are_chosen_kept_in_range_and_saved() {
    let mut rig = Rig::new();
    let view = rig.core.model().stream;
    assert_eq!((view.fps, view.video_kbps), (30, 4500), "the defaults");
    rig.core.apply(Action::SetStreamFps(60));
    rig.core.apply(Action::SetStreamKbps(6000));
    let view = rig.core.model().stream;
    assert_eq!((view.fps, view.video_kbps), (60, 6000));
    let saved = rig.saved_soon().expect("saved");
    assert_eq!((saved.stream.fps, saved.stream.video_kbps), (60, 6000));
    // Absurd values are brought into the range the encoder accepts.
    rig.core.apply(Action::SetStreamFps(0));
    rig.core.apply(Action::SetStreamKbps(10_000_000));
    let view = rig.core.model().stream;
    assert_eq!(view.fps, dockview_domain::stream_settings::MIN_FPS);
    assert_eq!(view.video_kbps, dockview_domain::stream_settings::MAX_KBPS);
}

#[test]
fn the_view_of_the_stream_carries_what_the_section_shows() {
    let mut rig = Rig::new();
    rig.core.apply(Action::SetStreamUrl(
        "rtmps://live.twitch.tv:443/app".into(),
    ));
    rig.core.apply(Action::SetStreamKey("live_key".into()));
    let view = rig.core.model().stream;
    assert_eq!(view.address, "rtmps://live.twitch.tv:443/app");
    assert!(view.has_key, "the key is there but its text never is");
}
