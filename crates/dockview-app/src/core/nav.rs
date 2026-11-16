//! The navigation row: the mode that is on and the status beside it.

use dockview_domain::broadcast::StreamState;
use dockview_domain::navigation::{encoder_badge, AppMode, EncoderInfo, EncoderState, NavView};
use dockview_domain::view::ReplayState;

use super::*;

impl Core {
    /// Goes to `mode`. The saved settings remember whether the Studio was the
    /// mode that was on; Settings remembers where it came from so that leaving
    /// it goes back there.
    pub(super) fn set_mode(&mut self, mode: AppMode) {
        if mode == AppMode::Settings && self.view.mode != AppMode::Settings {
            self.view.before_settings = self.view.mode;
        }
        self.view.mode = mode;
        let studio = mode == AppMode::Studio;
        if self.settings.studio_open != studio {
            self.settings.studio_open = studio;
            self.settings_changed();
        }
    }

    /// Opens the Studio, or goes back to the viewer if it is already on (the
    /// Tab key). From Settings it goes to the Studio.
    pub(super) fn toggle_studio(&mut self) {
        if self.view.mode == AppMode::Studio {
            self.set_mode(AppMode::Viewer);
        } else {
            self.open_studio();
        }
    }

    /// Goes to the Studio, leaving full screen (where only the picture shows)
    /// so that it is seen.
    pub(super) fn open_studio(&mut self) {
        self.go_to(AppMode::Studio);
    }

    /// Goes to a mode, leaving full screen when it is not the viewer.
    pub(super) fn go_to(&mut self, mode: AppMode) {
        if mode != AppMode::Viewer && self.view.fullscreen {
            self.set_fullscreen(false);
        }
        self.set_mode(mode);
    }

    /// What `Esc` does to the mode that is on: Settings goes back to where it
    /// came from, the Studio closes. Whether it did anything.
    pub(super) fn leave_mode(&mut self) -> bool {
        match self.view.mode {
            AppMode::Viewer => false,
            AppMode::Studio => {
                self.set_mode(AppMode::Viewer);
                true
            }
            AppMode::Settings => {
                self.set_mode(self.view.before_settings);
                true
            }
        }
    }

    pub(super) fn nav_view(&self) -> NavView {
        let replay_state = self
            .replay
            .buffer_info()
            .map(|b| match b.status {
                RecorderStatus::Starting => ReplayState::Starting,
                RecorderStatus::Recording => ReplayState::Recording,
                _ => ReplayState::Unavailable,
            })
            .unwrap_or(ReplayState::Off);
        let streaming = !matches!(
            self.stream.stats().state,
            StreamState::Offline | StreamState::Failed
        );
        let busy =
            replay_state == ReplayState::Recording || self.replay.recording_active() || streaming;
        let (encoder, encoder_name) = match self.replay.encoder() {
            _ if replay_state == ReplayState::Unavailable => (EncoderState::Unavailable, ""),
            EncoderInfo::Checking => (EncoderState::Unknown, ""),
            EncoderInfo::Missing => (EncoderState::Unavailable, ""),
            EncoderInfo::Found(name) => (
                if busy {
                    EncoderState::Active
                } else {
                    EncoderState::Ready
                },
                encoder_badge(&name),
            ),
        };
        NavView {
            encoder,
            encoder_name: encoder_name.to_string(),
            drops_per_sec: self.measures.stats.dropped_per_sec,
            stream: self.stream.stats().state,
        }
    }
}
