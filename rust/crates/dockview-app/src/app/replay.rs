//! The app's side of instant replay, clips, recording and the microphone: it
//! feeds the settings to the `ReplayService` and shows what it reports.

use super::*;

impl App {
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
            .chain(self.audio_devices.iter().map(|d| d.friendly_name.clone()))
            .collect()
    }

    /// Index in [`Self::mic_device_names`] of the chosen microphone.
    pub(super) fn mic_selected(&self) -> usize {
        mic_index_of(
            self.settings.last_mic_device_id.as_deref(),
            &self.audio_devices,
        )
    }

    /// Starts or stops recording the session.
    pub(super) fn toggle_record(&mut self) {
        // Recording rides on the replay buffer's encoder: switch it on if the
        // user had turned it off, and start once it is running.
        if self.replay.toggle_record() == RecordToggle::NeedsBuffer {
            self.settings.replay_enabled = true;
            self.sync_replay();
            self.save_settings();
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
            self.toast(i18n::Key::ToastTurnOnFirst, ToastKind::Info);
            return;
        }
        let device = self
            .settings
            .last_mic_device_id
            .clone()
            .or_else(|| self.automatic_mic());
        eprintln!(
            "microphone: {}",
            device
                .as_deref()
                .and_then(|id| self.audio_devices.iter().find(|d| d.device_id == id))
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
        let default = on_helper_thread(|| Some(default_capture_device()));
        let game_audio = self.audio_index.and_then(|i| self.audio_devices.get(i));
        pick_microphone(
            &self.audio_devices,
            default.as_ref(),
            game_audio,
            &self.video_devices,
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
        let owner = self.window_hwnd();
        let Some(folder) = clips::pick_folder(owner, &start) else {
            return; // cancelled
        };
        if !clips::is_writable_folder(&folder) {
            self.toast(i18n::Key::ToastFolderNotWritable, ToastKind::Error);
            return;
        }
        self.settings.replay_folder = Some(folder.display().to_string());
        self.needs_redraw = true;
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
            .map_or_else(clips::default_folder, std::path::PathBuf::from)
    }

    pub(super) fn replay_view(&self) -> ui::controls::ReplayView {
        use ui::controls::ReplayView;
        let mut view = ReplayView::off(hotkey::LABEL);
        view.record_hotkey = hotkey::RECORD_LABEL.to_owned();
        view.keep_seconds = self.settings.replay_seconds;
        view.bitrate_mbps = self.settings.replay_bitrate_mbps;
        view.memory_bytes = settings::replay_memory_bytes(
            self.settings.replay_seconds,
            self.settings.replay_bitrate_mbps,
        );
        view.at_limit = self.settings.replay_seconds >= self.replay_limit();
        view.folder = self.clips_folder().display().to_string();
        self.replay.fill_view(&mut view);
        view
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
        for notice in self.replay.take_notices() {
            self.toast(notice.key, notice.kind);
        }
    }
}
