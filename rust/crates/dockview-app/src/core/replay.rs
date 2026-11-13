//! The app's side of instant replay, clips, recording and the microphone: it
//! feeds the settings to the `ReplayService` and shows what it reports.

use super::*;

impl Core {
    /// Registers the hotkey and starts the replay buffer if it is enabled.
    pub(super) fn start_replay_services(&mut self) {
        if self.cli.ui_preview {
            return;
        }
        self.clamp_replay_length();
        self.replay.register_hotkeys();
        self.sync_replay();
    }

    /// Makes the recorder match `settings.replay_enabled`.
    pub(super) fn sync_replay(&mut self) {
        if self.settings.replay_enabled == self.replay.is_on() {
            return;
        }
        if self.settings.replay_enabled {
            self.replay.start_buffer(
                Arc::clone(&self.slot),
                ReplaySettings {
                    // A little extra so a clip of the chosen length always fits
                    // (it starts at the keyframe before the cut).
                    keep_seconds: f64::from(
                        self.settings.replay_seconds + settings::REPLAY_MARGIN_SECONDS,
                    ),
                    bitrate_mbps: self.settings.replay_bitrate_mbps,
                    fps: 60,
                },
            );
        } else {
            self.replay.stop_buffer();
        }
        self.route_audio();
    }

    /// Names for the microphone list: the Windows default, then every input.
    pub(super) fn mic_device_names(&self, lang: Lang) -> Vec<String> {
        std::iter::once(i18n::tr(lang, i18n::Key::MicDefault).to_owned())
            .chain(
                self.sources
                    .audio_devices
                    .iter()
                    .map(|d| d.friendly_name.clone()),
            )
            .collect()
    }

    /// Index in [`Self::mic_device_names`] of the chosen microphone.
    pub(super) fn mic_selected(&self) -> usize {
        mic_index_of(
            self.settings.last_mic_device_id.as_deref(),
            &self.sources.audio_devices,
        )
    }

    /// Starts or stops recording the session.
    pub(super) fn toggle_record(&mut self) {
        // Recording rides on the replay buffer's encoder: switch it on if the
        // user had turned it off, and start once it is running.
        if self.replay.toggle_record() == RecordToggle::NeedsBuffer {
            self.settings.replay_enabled = true;
            self.sync_replay();
            self.settings_changed();
            self.replay.wait_for_buffer();
        }
    }

    /// Opens or closes the microphone. It needs the replay buffer, which is
    /// what carries its audio.
    pub(super) fn set_mic(&mut self, on: bool) {
        if !on {
            self.replay.close_mic();
            return;
        }
        if !self.replay.is_on() {
            self.emit(AppEvent::ReplayIsOff);
            return;
        }
        let device = self
            .settings
            .last_mic_device_id
            .clone()
            .or_else(|| self.automatic_mic());
        dockview_domain::log_info!(
            "microphone: {}",
            device
                .as_deref()
                .and_then(|id| self
                    .sources
                    .audio_devices
                    .iter()
                    .find(|d| d.device_id == id))
                .map_or("Windows default input".to_owned(), |d| d
                    .friendly_name
                    .clone())
        );
        self.replay.open_mic(device);
    }

    /// The microphone to use when the user has not chosen one: Windows'
    /// default, unless that is the capture card's own audio (see
    /// [`dockview_domain::device::pick_microphone`]).
    pub(super) fn automatic_mic(&self) -> Option<String> {
        let default = self.sources.machine.default_capture_device();
        let game_audio = self
            .sources
            .audio_index
            .and_then(|i| self.sources.audio_devices.get(i));
        pick_microphone(
            &self.sources.audio_devices,
            default.as_ref(),
            game_audio,
            &self.sources.video_devices,
        )
        .map(|d| d.device_id.clone())
    }

    /// The longest replay this PC's memory allows, in seconds.
    pub(super) fn replay_limit(&self) -> u32 {
        settings::max_replay_seconds(self.settings.replay_bitrate_mbps, self.total_ram)
    }

    /// Brings a saved length that is too long for this PC down to the limit.
    pub(super) fn clamp_replay_length(&mut self) {
        self.settings.replay_seconds = self.settings.replay_seconds.min(self.replay_limit());
    }

    /// Starts the buffer over, for a change that needs a new encoder. The
    /// microphone, which feeds the old recorder, is reopened on the new one.
    pub(super) fn restart_recorder(&mut self) {
        let mic_on = self.replay.mic_on();
        self.replay.stop_buffer();
        self.sync_replay();
        if mic_on {
            self.set_mic(true);
        }
    }

    /// Lets the user pick the folder for clips and recordings.
    pub(super) fn choose_folder(&mut self) {
        let start = self.clips_folder();
        let Some(folder) = self.host.pick_folder(&start) else {
            return; // cancelled
        };
        if !self.replay.is_writable_folder(&folder) {
            self.emit(AppEvent::FolderNotWritable);
            return;
        }
        self.settings.replay_folder = Some(folder.display().to_string());
        self.host.request_redraw();
    }

    /// Opens Explorer on the newest file saved this session, or on the folder
    /// when there is none (or it was moved).
    pub(super) fn show_saved(&self) {
        self.replay.show_saved(&self.clips_folder());
    }

    /// Folder clips are saved to.
    pub(super) fn clips_folder(&self) -> std::path::PathBuf {
        self.settings
            .replay_folder
            .as_deref()
            .map_or_else(|| self.replay.default_folder(), std::path::PathBuf::from)
    }

    pub(super) fn replay_view(&self) -> dockview_domain::view::ReplayView {
        use dockview_domain::view::ReplayView;
        let (save_label, record_label) = self.replay.hotkey_labels();
        let mut view = ReplayView::off(&save_label);
        view.record_hotkey = record_label;
        view.keep_seconds = self.settings.replay_seconds;
        view.bitrate_mbps = self.settings.replay_bitrate_mbps;
        view.memory_bytes = settings::replay_memory_bytes(
            self.settings.replay_seconds,
            self.settings.replay_bitrate_mbps,
        );
        view.at_limit = self.settings.replay_seconds >= self.replay_limit();
        view.folder = self.clips_folder().display().to_string();
        view.saving = self.replay.saving();
        if let Some(buffer) = self.replay.buffer_info() {
            view.state = replay_state_of(&buffer.status);
            view.buffered_seconds = buffer.buffered_seconds as f32;
            view.buffered_mb = buffer.buffered_bytes as f32 / 1e6;
        }
        view
    }

    /// What the record button shows.
    pub fn rec_view(&self) -> RecView {
        rec_view_of(
            self.replay.waiting_to_record(),
            self.replay.recording_info(),
        )
    }

    /// What the microphone controls show.
    pub(super) fn mic_view(&self) -> MicView {
        mic_view_of(self.replay.mic_status(), self.replay.mic_level())
    }

    pub(super) fn save_replay(&mut self) {
        let folder = self.clips_folder();
        self.replay
            .save_clip(&folder, f64::from(self.settings.replay_seconds));
    }

    /// Per-tick work: the hotkeys, and whatever the replay service reports.
    pub(super) fn poll_replay(&mut self) {
        let (save, record) = self.replay.take_hotkeys();
        if save {
            self.save_replay();
        }
        if record {
            self.toggle_record();
        }
        let folder = self.clips_folder();
        self.replay.poll(&folder);
        let replay_events = self.replay.take_events();
        self.events.extend(replay_events);
    }
}

/// What the record button shows: "starting" while it waits for the buffer.
fn rec_view_of(waiting: bool, info: Option<RecordingInfo>) -> RecView {
    if waiting {
        return RecView {
            state: RecState::Starting,
            seconds: 0.0,
        };
    }
    let Some(info) = info else {
        return RecView::idle();
    };
    RecView {
        state: match info.state {
            RecordingState::Idle => RecState::Idle,
            RecordingState::Starting => RecState::Starting,
            RecordingState::Recording => RecState::Recording,
        },
        seconds: info.seconds as f32,
    }
}

/// What the microphone controls show for the microphone's state (`None` is off).
fn mic_view_of(status: Option<AudioStatus>, level: f32) -> MicView {
    let state = match status {
        None | Some(AudioStatus::Stopped) => MicState::Off,
        Some(AudioStatus::Starting) => MicState::Starting,
        Some(AudioStatus::Running) => MicState::Live,
        Some(AudioStatus::Disconnected(_) | AudioStatus::Failed(_)) => MicState::Unavailable,
    };
    MicView { state, level }
}

/// How the replay popover shows the state of the buffer.
fn replay_state_of(status: &RecorderStatus) -> ReplayState {
    match status {
        RecorderStatus::Starting => ReplayState::Starting,
        RecorderStatus::Recording => ReplayState::Recording,
        RecorderStatus::Unsupported(_) | RecorderStatus::Failed(_) => ReplayState::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(state: RecordingState, seconds: f64) -> RecordingInfo {
        RecordingInfo {
            state,
            seconds,
            bytes: 0,
            low_disk: false,
        }
    }

    #[test]
    fn the_record_button_follows_the_wait_then_the_recording() {
        assert_eq!(rec_view_of(false, None), RecView::idle());
        // Waiting for the buffer wins over whatever the recorder says.
        let waiting = rec_view_of(true, Some(info(RecordingState::Idle, 0.0)));
        assert_eq!((waiting.state, waiting.seconds), (RecState::Starting, 0.0));
        let live = rec_view_of(false, Some(info(RecordingState::Recording, 12.5)));
        assert_eq!((live.state, live.seconds), (RecState::Recording, 12.5));
        assert!(rec_view_of(false, Some(info(RecordingState::Starting, 0.0))).is_on());
    }

    #[test]
    fn the_microphone_light_follows_its_state() {
        assert_eq!(mic_view_of(None, 0.0).state, MicState::Off);
        assert_eq!(
            mic_view_of(Some(AudioStatus::Starting), 0.0).state,
            MicState::Starting
        );
        assert_eq!(
            mic_view_of(Some(AudioStatus::Running), 0.4).state,
            MicState::Live
        );
        assert_eq!(mic_view_of(Some(AudioStatus::Running), 0.4).level, 0.4);
        assert_eq!(
            mic_view_of(Some(AudioStatus::Failed("x".into())), 0.0).state,
            MicState::Unavailable
        );
    }

    #[test]
    fn the_buffer_state_maps_to_the_popover_state() {
        assert_eq!(
            replay_state_of(&RecorderStatus::Starting),
            ReplayState::Starting
        );
        assert_eq!(
            replay_state_of(&RecorderStatus::Recording),
            ReplayState::Recording
        );
        assert_eq!(
            replay_state_of(&RecorderStatus::Unsupported("no encoder".into())),
            ReplayState::Unavailable
        );
    }
}
