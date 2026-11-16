//! Going live: the button, the sound that also goes to the stream, and what
//! the user is told as the stream connects, drops and ends.

use dockview_domain::pcm::AudioTap;
use dockview_domain::view::StreamView;

use super::*;
use crate::stream_service::{StartError, StreamEvent};

impl Core {
    /// Where the game's sound is delivered: the replay's queue when the replay
    /// runs (the stream, if there is one, gets a copy), else the stream's own.
    /// With neither, nowhere.
    pub(super) fn audio_destination(&mut self) -> Option<Arc<AudioTap>> {
        let stream = self.stream.audio_tap();
        let destination = match self.replay.audio_tap() {
            Some(replay) => {
                replay.set_mirror(stream);
                Some(replay)
            }
            None => stream,
        };
        // The microphone rides on the replay, so it reaches the stream only
        // while the replay runs.
        if let Some(mic) = self.replay.mic_tap() {
            mic.set_mirror(self.stream.mic_tap());
        }
        destination
    }

    /// Goes live, or ends the stream.
    pub(super) fn toggle_stream(&mut self) {
        if self.stream.stop() {
            self.route_audio();
            self.emit(AppEvent::StreamStopped);
            return;
        }
        match self
            .stream
            .start(Arc::clone(&self.slot), &self.settings.stream)
        {
            Ok(()) => self.route_audio(),
            Err(StartError::NoKey) => self.emit(AppEvent::StreamNoKey),
            Err(StartError::BadAddress) => self.emit(AppEvent::StreamBadAddress),
            Err(StartError::Failed(why)) => {
                dockview_domain::log_warn!("the stream could not start: {why}");
                self.emit(AppEvent::StreamStartFailed);
            }
        }
    }

    /// Tells the user what the stream did since the last tick.
    pub(super) fn poll_stream(&mut self) {
        for event in self.stream.poll() {
            match event {
                StreamEvent::WentLive => self.emit(AppEvent::StreamLive),
                StreamEvent::BackLive => self.emit(AppEvent::StreamBack),
                StreamEvent::Reconnecting(why) => {
                    dockview_domain::log_warn!("the stream dropped: {why}");
                    self.emit(AppEvent::StreamReconnecting);
                }
                StreamEvent::Failed(why) => {
                    dockview_domain::log_warn!("the stream ended: {why}");
                    self.route_audio();
                    self.emit(AppEvent::StreamFailed);
                }
            }
        }
    }

    pub(super) fn set_stream_key(&mut self, key: &str) {
        if let Err(why) = self.stream.set_key(key) {
            dockview_domain::log_warn!("the stream key could not be saved: {why}");
            self.emit(AppEvent::SettingsNotSaved);
        }
    }

    pub(super) fn stream_view(&self) -> StreamView {
        let stats = self.stream.stats();
        StreamView {
            state: stats.state,
            bitrate_kbps: stats.bitrate_bps / 1000,
            dropped_frames: stats.dropped_frames,
            reconnects: stats.reconnects,
            has_key: self.stream.has_key(),
            address: self.settings.stream.url.clone(),
            fps: self.settings.stream.fps,
            video_kbps: self.settings.stream.video_kbps,
        }
    }
}
