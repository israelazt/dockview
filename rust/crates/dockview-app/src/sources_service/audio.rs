//! The sound of the main source: the capture card's audio input played to the
//! speakers, or, while a screen is captured, the loopback of the PC (or of one
//! program) that only the recorder hears. It also notices when the card's sound
//! stops and says when to start it again.

use std::sync::Arc;
use std::time::Instant;

use dockview_domain::audio_status::AudioStatus;
use dockview_domain::events::AppEvent;
use dockview_domain::pcm::AudioTap;
use dockview_domain::ports::Loopback;

use super::SourcesService;
use crate::sources::{audio_is_lost, AudioRoute, SourceKind};

/// What [`SourcesService::check_audio`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AudioCheck {
    /// Something the user should be told: the sound was lost, or it is back.
    pub event: Option<AppEvent>,
    /// The sound is lost and it is time to start it again.
    pub restart: bool,
}

impl SourcesService {
    /// Points the recorder's game-sound input at the right place: the capture
    /// card's audio (fed to `tap`), or the loopback of the PC (a monitor) or of
    /// one program (a window). Neither is played back by Dockview.
    pub fn route_audio(&mut self, tap: Option<Arc<AudioTap>>) {
        self.loopback = None;
        match self.source_kind.audio_route() {
            AudioRoute::Card => {
                if let Some(audio) = &self.audio {
                    audio.set_tap(tap);
                }
            }
            route => {
                let source = match route {
                    AudioRoute::Process(pid) => Loopback::Process(pid),
                    _ => Loopback::System,
                };
                if let Some(tap) = tap {
                    self.loopback = Some(self.machine.start_loopback(source, tap));
                }
            }
        }
    }

    /// Plays the audio input at `index` (or, with `None`, stops the sound),
    /// feeding `tap` to the recorder. Returns the id of the input it started,
    /// for the caller to remember. While a screen is captured its own sound is
    /// used, so nothing is started (the choice is only noted).
    pub fn play(
        &mut self,
        index: Option<usize>,
        volume: f32,
        offset_ms: i32,
        tap: Option<Arc<AudioTap>>,
    ) -> Option<String> {
        self.audio_index = index;
        if self.source_kind != SourceKind::Capture {
            return None;
        }
        self.audio = None;
        let device_id = index
            .and_then(|i| self.audio_devices.get(i))?
            .device_id
            .clone();
        let audio = self
            .machine
            .start_playback(device_id.clone(), volume, offset_ms);
        if let Some(tap) = tap {
            audio.set_tap(Some(tap));
        }
        self.audio = Some(audio);
        Some(device_id)
    }

    /// Stops the card's sound (not needed while a screen is captured).
    pub fn stop_audio(&mut self) {
        self.audio = None;
    }

    /// Whether a sound pipeline exists (it can be a dead one, waiting to be
    /// started again).
    pub fn has_audio(&self) -> bool {
        self.audio.is_some()
    }

    pub fn set_volume(&self, volume: f32) {
        if let Some(audio) = &self.audio {
            audio.set_volume(volume);
        }
    }

    pub fn set_offset_ms(&self, offset_ms: i32) {
        if let Some(audio) = &self.audio {
            audio.set_offset_ms(offset_ms);
        }
    }

    /// Whether the sound works: `None` when there is no sound pipeline.
    pub fn audio_ok(&self) -> Option<bool> {
        self.audio
            .as_ref()
            .map(|audio| !audio_is_lost(&audio.status()))
    }

    /// Audio buffered for the sync offset, in milliseconds.
    pub fn audio_buffered_ms(&self) -> Option<u32> {
        self.audio.as_ref().map(|audio| audio.buffered_ms())
    }

    /// Watches the card's sound. It reports when it was lost and when it is
    /// back (each once), and when to start it again: after a pause, and only
    /// while an input is selected (an unplugged one is waited for).
    pub fn check_audio(&mut self, now: Instant) -> AudioCheck {
        if self.source_kind != SourceKind::Capture {
            return AudioCheck::default();
        }
        let Some(status) = self.audio.as_ref().map(|audio| audio.status()) else {
            return AudioCheck::default();
        };
        let lost = audio_is_lost(&status);
        let mut event = None;
        if lost && !self.audio_lost {
            self.audio_lost = true;
            event = Some(AppEvent::AudioLost);
        } else if self.audio_lost && status == AudioStatus::Running {
            self.audio_lost = false;
            event = Some(AppEvent::AudioBack);
        }
        let restart = self.audio_reconnect.tick(lost, now) && self.audio_index.is_some();
        AudioCheck { event, restart }
    }
}
