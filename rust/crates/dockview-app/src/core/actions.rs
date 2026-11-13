//! What the user asks for (clicks and keys) and what the controls are shown.

use super::*;

impl Core {
    /// Tells the user something happened; it is shown on the next tick.
    pub(super) fn emit(&mut self, event: AppEvent) {
        self.events.push(event);
    }

    pub(super) fn change_volume(&mut self, delta: f32) {
        self.settings.volume = settings::step_volume(self.settings.volume, delta);
        self.sources.set_volume(self.settings.volume);
        self.settings_changed();
    }

    pub(super) fn change_offset(&mut self, delta_ms: i32) {
        self.settings.audio_offset_ms =
            settings::step_audio_offset(self.settings.audio_offset_ms, delta_ms);
        self.sources.set_offset_ms(self.settings.audio_offset_ms);
        self.settings_changed();
    }

    pub(super) fn cycle_preset(&mut self) {
        self.settings.render_preset = next_preset(self.settings.render_preset);
        self.host.set_render_preset(self.settings.render_preset);
        self.settings_changed();
    }

    /// Enters or leaves full screen.
    pub fn set_fullscreen(&mut self, on: bool) {
        self.view.fullscreen = on;
        self.host.set_fullscreen(on);
        self.settings_changed();
    }

    /// Something in the settings changed. It is written on a following tick,
    /// at most twice a second, so dragging a slider is not a write per pixel.
    pub(super) fn settings_changed(&mut self) {
        self.storage.changed();
    }

    /// Writes the settings now. The window calls it when it closes.
    pub fn save_settings(&mut self) {
        if self.cli.ui_preview {
            self.storage.discard();
            return;
        }
        self.settings.was_fullscreen = self.view.fullscreen;
        if self.storage.save(&self.settings) {
            self.emit(AppEvent::SettingsNotSaved);
        }
    }

    /// Writes the settings if they changed and it has been long enough since
    /// the last write.
    pub(super) fn flush_settings(&mut self, now: Instant) {
        if self.storage.is_due(now) {
            self.save_settings();
            self.storage.mark_handled(now);
        }
    }

    pub(super) fn theme_mode(&self) -> ThemeMode {
        self.cli.theme.unwrap_or(self.settings.theme)
    }

    pub(super) fn lang_mode(&self) -> LangMode {
        self.cli.lang.unwrap_or(self.settings.language)
    }

    /// Snapshot of everything the controls display.
    pub fn model(&self) -> Model {
        let status = status_kind(self.sources.video_state());
        let resolution =
            resolution_label(self.view.video_size, self.measures.telemetry.shown_fps());
        let appearance = self.theme_mode().resolve(self.view.system_appearance);
        let names = |devices: &[String]| devices.to_vec();
        Model {
            theme: Theme::for_appearance(appearance),
            lang: self.lang_mode().resolve(self.view.system_lang),
            status,
            resolution,
            volume: self.settings.volume,
            sync_ms: self.settings.audio_offset_ms,
            fullscreen: self.view.fullscreen,
            // The names shown can differ from the ones Windows gives (aliases).
            video_devices: names(
                &self
                    .sources
                    .video_devices
                    .iter()
                    .map(|d| {
                        device_names::display_name(
                            &d.symbolic_link,
                            &d.friendly_name,
                            &self.settings.device_aliases,
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
            video_selected: self.sources.video_index,
            audio_devices: names(
                &self
                    .sources
                    .audio_devices
                    .iter()
                    .map(|d| d.friendly_name.clone())
                    .collect::<Vec<_>>(),
            ),
            audio_selected: self.sources.audio_index,
            preset: self.settings.render_preset,
            theme_mode: self.theme_mode(),
            lang_mode: self.lang_mode(),
            video_mode: self.settings.video_mode,
            stats_open: self.view.stats_open,
            studio: self.settings.studio_open,
            replay: self.replay_view(),
            mic_devices: self.mic_device_names(self.lang_mode().resolve(self.view.system_lang)),
            mic_selected: Some(self.mic_selected()),
            mic: self.mic_view(),
            rec: self.rec_view(),
            sources: self
                .sources
                .view(self.lang_mode().resolve(self.view.system_lang)),
            scenes: self.scenes_view(self.lang_mode().resolve(self.view.system_lang)),
        }
    }

    /// Applies a change requested through the controls.
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::SetVolume(volume) => {
                self.settings.volume = volume;
                self.sources.set_volume(volume);
            }
            Action::SetSync(ms) => {
                self.settings.audio_offset_ms = ms;
                self.sources.set_offset_ms(ms);
            }
            Action::Scene(action) => {
                self.apply_scene_action(action);
                return;
            }
            Action::ToggleFullscreen => {
                self.set_fullscreen(!self.view.fullscreen);
                return;
            }
            Action::SelectVideo(index) => {
                self.select_video(Some(index));
                // Follow the new video device with its matching audio.
                if let Some(audio) = self.sources.match_audio(Some(index)) {
                    self.select_audio(Some(audio));
                }
            }
            Action::SelectAudio(index) => self.select_audio(Some(index)),
            Action::ToggleReplay if self.replay.recording_active() => {
                // Turning the buffer off would cut the recording short.
                self.emit(AppEvent::StopRecordingFirst);
                return;
            }
            Action::ToggleReplay => {
                self.settings.replay_enabled = !self.settings.replay_enabled;
                self.sync_replay();
                self.emit(if self.settings.replay_enabled {
                    AppEvent::ReplayOn
                } else {
                    AppEvent::ReplayOff
                });
            }
            Action::SelectMonitor(index) => {
                if let Some(monitor) = self.sources.monitors.get(index) {
                    let (handle, name) = (monitor.handle, monitor.device_name.clone());
                    self.start_screen(CaptureTarget::Monitor(handle), SourceKind::Monitor(handle));
                    self.settings.last_monitor = Some(index as u32);
                    self.settings.last_monitor_name = Some(name);
                }
            }
            Action::SelectWindow(index) => {
                if let Some(window) = self.sources.windows.get(index).cloned() {
                    self.start_screen(
                        CaptureTarget::Window(window.handle),
                        SourceKind::Window {
                            handle: window.handle,
                            pid: window.process_id,
                        },
                    );
                    self.settings.last_monitor = None;
                    self.settings.last_monitor_name = None;
                }
            }
            Action::ToggleRecord => {
                self.toggle_record();
                return;
            }
            Action::ToggleMic => {
                let on = !self.replay.mic_on();
                self.set_mic(on);
                if self.replay.mic_on() == on {
                    self.emit(if on {
                        AppEvent::MicOn
                    } else {
                        AppEvent::MicOff
                    });
                }
                return; // the microphone always starts off; nothing to save
            }
            Action::SelectMic(index) => {
                self.settings.last_mic_device_id =
                    mic_device_id(index, &self.sources.audio_devices);
                if self.replay.mic_on() {
                    // Reopen on the new device.
                    self.set_mic(false);
                    self.set_mic(true);
                }
            }
            Action::ReplayLength(delta) => {
                self.settings.replay_seconds =
                    settings::step_replay_seconds(self.settings.replay_seconds, delta);
                self.clamp_replay_length();
                self.replay.set_keep_seconds(f64::from(
                    self.settings.replay_seconds + settings::REPLAY_MARGIN_SECONDS,
                ));
            }
            Action::SaveReplay => {
                self.save_replay();
                return;
            }
            Action::OpenReplayFolder => {
                self.show_saved();
                return;
            }
            Action::ChooseFolder => self.choose_folder(),
            Action::SetQuality(quality) => {
                if self.replay.recording_active() {
                    // A new encoder would cut the recording short.
                    self.emit(AppEvent::StopRecordingFirst);
                    return;
                }
                if self.settings.replay_bitrate_mbps != quality.mbps() {
                    self.settings.replay_bitrate_mbps = quality.mbps();
                    self.clamp_replay_length();
                    if self.replay.is_on() {
                        self.restart_recorder();
                        self.emit(AppEvent::QualityChanged);
                    }
                }
            }
            Action::ToggleStats => {
                self.view.stats_open = !self.view.stats_open;
                return;
            }
            Action::ToggleStudio => {
                self.settings.studio_open = !self.settings.studio_open;
                self.settings_changed();
                return;
            }
            Action::SetVideoMode(mode) => {
                self.settings.video_mode = mode;
                // A deliberate choice ends any automatic switch and allows a
                // new automatic attempt later (for example after changing port).
                self.measures.auto.reset();
                self.select_video(self.sources.video_index);
            }
            Action::SetPreset(preset) => {
                self.settings.render_preset = preset;
                self.host.set_render_preset(preset);
            }
            Action::SetTheme(mode) => {
                self.settings.theme = mode;
                self.cli.theme = None; // an explicit choice wins over --theme
            }
            Action::SetLang(mode) => {
                self.settings.language = mode;
                self.cli.lang = None;
            }
        }
        self.settings_changed();
    }

    /// Carries out what a key asked for.
    pub fn on_key(&mut self, command: KeyCommand) {
        match command {
            KeyCommand::ToggleFullscreen => self.set_fullscreen(!self.view.fullscreen),
            KeyCommand::Escape => {
                // The window closes its popover first; this is what is left.
                if self.view.fullscreen {
                    self.set_fullscreen(false);
                }
            }
            KeyCommand::VolumeUp => self.change_volume(VOLUME_STEP),
            KeyCommand::VolumeDown => self.change_volume(-VOLUME_STEP),
            KeyCommand::SyncEarlier => self.change_offset(-OFFSET_STEP_MS),
            KeyCommand::SyncLater => self.change_offset(OFFSET_STEP_MS),
            KeyCommand::CyclePreset => self.cycle_preset(),
            KeyCommand::ToggleStats => self.view.stats_open = !self.view.stats_open,
            KeyCommand::NextVideo => {
                let next = next_index(self.sources.video_index, self.sources.video_devices.len());
                self.select_video(next);
                // Follow the new video device with its matching audio.
                if let Some(audio) = self.sources.match_audio(next) {
                    self.select_audio(Some(audio));
                }
                self.settings_changed();
            }
            KeyCommand::NextAudio => {
                let next = next_index(self.sources.audio_index, self.sources.audio_devices.len());
                self.select_audio(next);
                self.settings_changed();
            }
        }
    }
}
