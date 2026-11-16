//! The navigation row: the mode that is on and the status beside it.

use dockview_domain::broadcast::StreamState;
use dockview_domain::navigation::{encoder_badge, AppMode, EncoderInfo, EncoderState, NavView};
use dockview_domain::view::ReplayState;

use super::*;

impl Core {
    /// Goes to `mode` and remembers it: the saved settings say whether the
    /// panel was open, and the panel comes back on the last mode it had.
    pub(super) fn set_mode(&mut self, mode: AppMode) {
        self.view.mode = mode;
        if mode.shows_panel() {
            self.view.last_panel = mode;
        }
        if self.settings.studio_open != mode.shows_panel() {
            self.settings.studio_open = mode.shows_panel();
            self.settings_changed();
        }
    }

    /// Opens the Studio panel when it is closed, or closes it.
    pub(super) fn toggle_studio(&mut self) {
        if self.view.mode.shows_panel() {
            self.set_mode(AppMode::Viewer);
        } else {
            self.set_mode(self.view.last_panel);
        }
    }

    /// Opens the Studio panel if it is closed, leaving full screen (where it
    /// is hidden) so that it shows.
    pub(super) fn open_studio(&mut self) {
        if self.view.fullscreen {
            self.set_fullscreen(false);
        }
        if !self.view.mode.shows_panel() {
            self.set_mode(self.view.last_panel);
        }
    }

    /// Goes to a mode. Studio and Settings open the Studio panel (the section
    /// is chosen by the interface, which also sees the click).
    pub(super) fn go_to(&mut self, mode: AppMode) {
        if mode == AppMode::Viewer {
            self.set_mode(AppMode::Viewer);
            return;
        }
        self.open_studio();
        self.set_mode(mode);
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
