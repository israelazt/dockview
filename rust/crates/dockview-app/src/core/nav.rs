//! The navigation row: which tab is on and the status beside it.

use dockview_domain::broadcast::StreamState;
use dockview_domain::navigation::{encoder_badge, EncoderInfo, EncoderState, NavTab, NavView};
use dockview_domain::view::ReplayState;

use super::*;

impl Core {
    /// Goes to a tab. Until the Recordings and Settings tabs have screens of
    /// their own they open the Studio panel (the section is chosen by the
    /// interface, which also sees the click).
    pub(super) fn go_to(&mut self, tab: NavTab) {
        if tab == NavTab::Viewer {
            if self.settings.studio_open {
                self.toggle_studio();
            }
            return;
        }
        self.open_studio();
        // The panel opened by the key or the pill later comes back on the tab
        // that was chosen last.
        self.view.nav_tab = tab;
    }

    pub(super) fn nav_view(&self) -> NavView {
        let panel = self.settings.studio_open && !self.view.fullscreen;
        let tab = if !panel {
            NavTab::Viewer
        } else if self.view.nav_tab == NavTab::Viewer {
            NavTab::Studio
        } else {
            self.view.nav_tab
        };
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
            tab,
            encoder,
            encoder_name: encoder_name.to_string(),
            drops: self.slot.dropped() + self.stream.stats().dropped_frames,
            stream: self.stream.stats().state,
        }
    }
}
