//! Going live, as the app sees it: start and stop a stream, keep the key
//! safe, and turn what the stream thread reports into things worth telling the
//! user (it is live, the connection dropped, it came back, it gave up). It only
//! talks to the ports, so it is tested with fakes.

use std::sync::Arc;

use dockview_domain::broadcast::{
    SecretStore, StreamHandle, StreamMachine, StreamState, StreamStats,
};
use dockview_domain::frame::FrameSlot;
use dockview_domain::pcm::AudioTap;
use dockview_domain::stream_settings::{StreamSettings, StreamSetupError};

/// The name the stream key is kept under.
const KEY: &str = "stream-key";

/// Why a stream did not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError {
    /// The address is empty or not an `rtmp://` / `rtmps://` one.
    BadAddress,
    /// No stream key was saved.
    NoKey,
    /// The machine refused, in its own words.
    Failed(String),
}

/// What happened to a running stream, once each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    /// The server accepted the stream.
    WentLive,
    /// The connection dropped; it keeps trying.
    Reconnecting(String),
    /// It is back after a drop.
    BackLive,
    /// It gave up (or the server refused the key). The stream is over.
    Failed(String),
}

pub struct StreamService {
    machine: Box<dyn StreamMachine>,
    secrets: Box<dyn SecretStore>,
    handle: Option<Box<dyn StreamHandle>>,
    /// The state the user was last told about.
    told: StreamState,
}

impl StreamService {
    pub fn new(machine: Box<dyn StreamMachine>, secrets: Box<dyn SecretStore>) -> Self {
        Self {
            machine,
            secrets,
            handle: None,
            told: StreamState::Offline,
        }
    }

    /// Saves the stream key (kept apart from the settings).
    pub fn set_key(&self, key: &str) -> Result<(), String> {
        if key.trim().is_empty() {
            self.secrets.remove(KEY);
            return Ok(());
        }
        self.secrets.set(KEY, key.trim())
    }

    pub fn has_key(&self) -> bool {
        self.secrets.get(KEY).is_some_and(|k| !k.trim().is_empty())
    }

    /// Starts streaming the pictures of `slot`. Does nothing when a stream is
    /// already running.
    pub fn start(
        &mut self,
        slot: Arc<FrameSlot>,
        settings: &StreamSettings,
    ) -> Result<(), StartError> {
        if self.handle.is_some() {
            return Ok(());
        }
        let key = self.secrets.get(KEY).unwrap_or_default();
        let request = settings.request(&key).map_err(|e| match e {
            StreamSetupError::BadAddress => StartError::BadAddress,
            StreamSetupError::NoKey => StartError::NoKey,
        })?;
        let handle = self
            .machine
            .start_stream(slot, request)
            .map_err(StartError::Failed)?;
        self.handle = Some(handle);
        self.told = StreamState::Connecting;
        Ok(())
    }

    /// Ends the stream; whether there was one.
    pub fn stop(&mut self) -> bool {
        self.told = StreamState::Offline;
        self.handle.take().is_some()
    }

    /// Whether a stream is running (connecting, live or reconnecting).
    pub fn is_active(&self) -> bool {
        self.handle.is_some()
    }

    pub fn stats(&self) -> StreamStats {
        self.handle.as_ref().map(|h| h.stats()).unwrap_or_default()
    }

    /// Where the game's sound should also be delivered while streaming.
    pub fn audio_tap(&self) -> Option<Arc<AudioTap>> {
        self.handle.as_ref().map(|h| h.audio_tap())
    }

    /// Where the microphone's sound should also be delivered while streaming.
    pub fn mic_tap(&self) -> Option<Arc<AudioTap>> {
        self.handle.as_ref().map(|h| h.mic_tap())
    }

    /// What changed since the last call. A stream that failed is ended here.
    pub fn poll(&mut self) -> Vec<StreamEvent> {
        let Some(handle) = &self.handle else {
            return Vec::new();
        };
        let state = handle.stats().state;
        let error = handle.last_error().unwrap_or_default();
        if state == self.told {
            return Vec::new();
        }
        let before = std::mem::replace(&mut self.told, state);
        match state {
            StreamState::Live if before == StreamState::Reconnecting => vec![StreamEvent::BackLive],
            StreamState::Live => vec![StreamEvent::WentLive],
            StreamState::Reconnecting => vec![StreamEvent::Reconnecting(error)],
            StreamState::Failed => {
                self.handle = None;
                self.told = StreamState::Offline;
                vec![StreamEvent::Failed(error)]
            }
            StreamState::Connecting | StreamState::Offline => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::stream::{world, FakeStream};
    use dockview_domain::broadcast::MemorySecrets;

    fn service() -> (StreamService, crate::testing::stream::Shared) {
        let w = world();
        let s = StreamService::new(
            Box::new(FakeStream(w.clone())),
            Box::new(MemorySecrets::default()),
        );
        (s, w)
    }

    fn settings() -> StreamSettings {
        StreamSettings {
            url: "rtmp://live.example/app".into(),
            ..StreamSettings::default()
        }
    }

    fn slot() -> Arc<FrameSlot> {
        Arc::new(FrameSlot::new())
    }

    #[test]
    fn it_does_not_start_without_an_address_or_a_key() {
        let (mut s, w) = service();
        assert_eq!(
            s.start(slot(), &StreamSettings::default()),
            Err(StartError::BadAddress)
        );
        assert_eq!(s.start(slot(), &settings()), Err(StartError::NoKey));
        assert!(w.lock().unwrap().requests.is_empty());
        assert!(!s.is_active());
    }

    #[test]
    fn it_starts_with_the_saved_key_and_the_settings() {
        let (mut s, w) = service();
        s.set_key(" live_abc ").unwrap();
        assert!(s.has_key());
        let mut set = settings();
        set.fps = 60;
        set.video_kbps = 6000;
        s.start(slot(), &set).unwrap();
        let requests = w.lock().unwrap().requests.clone();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            (
                requests[0].key.as_str(),
                requests[0].fps,
                requests[0].video_kbps
            ),
            ("live_abc", 60, 6000)
        );
        assert!(s.is_active());
        // Starting again is a no-op.
        s.start(slot(), &set).unwrap();
        assert_eq!(w.lock().unwrap().requests.len(), 1);
    }

    #[test]
    fn a_blank_key_clears_it() {
        let (s, _) = service();
        s.set_key("abc").unwrap();
        s.set_key("   ").unwrap();
        assert!(!s.has_key());
    }

    #[test]
    fn the_machine_refusing_is_reported_with_its_words() {
        let (mut s, w) = service();
        s.set_key("k").unwrap();
        w.lock().unwrap().refuse = Some("no hardware encoder".into());
        assert_eq!(
            s.start(slot(), &settings()),
            Err(StartError::Failed("no hardware encoder".into()))
        );
        assert!(!s.is_active());
    }

    #[test]
    fn going_live_dropping_and_coming_back_are_each_told_once() {
        let (mut s, w) = service();
        s.set_key("k").unwrap();
        s.start(slot(), &settings()).unwrap();
        assert!(s.poll().is_empty(), "still connecting");
        w.lock().unwrap().state = StreamState::Live;
        assert_eq!(s.poll(), [StreamEvent::WentLive]);
        assert!(s.poll().is_empty());
        {
            let mut w = w.lock().unwrap();
            w.state = StreamState::Reconnecting;
            w.error = Some("connection reset".into());
        }
        assert_eq!(
            s.poll(),
            [StreamEvent::Reconnecting("connection reset".into())]
        );
        w.lock().unwrap().state = StreamState::Live;
        assert_eq!(s.poll(), [StreamEvent::BackLive]);
    }

    #[test]
    fn a_failed_stream_is_told_and_ended() {
        let (mut s, w) = service();
        s.set_key("k").unwrap();
        s.start(slot(), &settings()).unwrap();
        {
            let mut w = w.lock().unwrap();
            w.state = StreamState::Failed;
            w.error = Some("refused: NetStream.Publish.BadName".into());
        }
        assert_eq!(
            s.poll(),
            [StreamEvent::Failed(
                "refused: NetStream.Publish.BadName".into()
            )]
        );
        assert!(!s.is_active());
        assert_eq!(w.lock().unwrap().alive, 0, "the handle was dropped");
        assert!(s.poll().is_empty());
    }

    #[test]
    fn stopping_drops_the_stream_and_exposes_no_taps() {
        let (mut s, w) = service();
        s.set_key("k").unwrap();
        s.start(slot(), &settings()).unwrap();
        assert!(s.audio_tap().is_some() && s.mic_tap().is_some());
        assert!(s.stop());
        assert!(!s.stop());
        assert_eq!(w.lock().unwrap().alive, 0);
        assert!(s.audio_tap().is_none());
        assert_eq!(s.stats().state, StreamState::Offline);
    }
}
