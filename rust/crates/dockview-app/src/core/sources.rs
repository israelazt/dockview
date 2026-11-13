//! Where the picture and the sound come from: the capture card, a screen or a
//! window, and where the game's audio goes.

use super::*;

impl Core {
    /// Points the recorder's game-sound input at the right place.
    pub(super) fn route_audio(&mut self) {
        let tap = self.replay.audio_tap();
        self.sources.route_audio(tap);
    }

    /// Hides Dockview's own window from the capture, so a captured screen does
    /// not show the viewer inside itself.
    pub(super) fn set_capture_exclusion(&self, on: bool) {
        self.host.exclude_from_capture(on);
    }

    /// Captures a monitor or a window instead of the capture card.
    pub(super) fn start_screen(&mut self, target: CaptureTarget, kind: SourceKind) {
        if self.cli.test_pattern {
            return;
        }
        self.sources.release(); // releases the capture card
        self.sources.stop_audio(); // the card's sound is not needed
        self.view.video_size = (0, 0);
        self.set_capture_exclusion(true);
        self.sources.open_screen(target, kind);
        self.route_audio();
    }

    /// Lists the monitors and the windows again.
    pub fn refresh_sources(&mut self) {
        self.sources.refresh();
    }

    /// Starts on a monitor or a window when the command line or the last
    /// session asked for one.
    pub(super) fn restore_screen(&mut self) {
        if let Some(text) = self.cli.capture_window.clone() {
            let found = find_window(&self.sources.windows, &text, std::process::id()).cloned();
            match found {
                Some(w) => self.start_screen(
                    CaptureTarget::Window(w.handle),
                    SourceKind::Window {
                        handle: w.handle,
                        pid: w.process_id,
                    },
                ),
                None => dockview_domain::log_warn!("no window matches '{text}'"),
            }
            return;
        }
        let wanted = self.cli.screen.or_else(|| {
            monitor_to_restore(
                &self.sources.monitors,
                self.settings.last_monitor_name.as_deref(),
                self.settings.last_monitor,
            )
        });
        if let Some(monitor) = wanted.and_then(|i| self.sources.monitors.get(i)) {
            let handle = monitor.handle;
            self.start_screen(CaptureTarget::Monitor(handle), SourceKind::Monitor(handle));
        }
    }

    /// Starts capturing from the video device at `index` (or stops).
    pub(super) fn select_video(&mut self, index: Option<usize>) {
        self.sources.video_index = index;
        if self.cli.test_pattern {
            return; // the synthetic source stays; only the selection changes
        }
        let from_screen = self.sources.source_kind != SourceKind::Capture;
        if from_screen {
            self.sources.source_kind = SourceKind::Capture;
            self.settings.last_monitor = None;
            self.settings.last_monitor_name = None;
            self.set_capture_exclusion(self.wants_exclusion());
        }
        self.view.video_size = (0, 0);
        // Drop the old capture first so it releases the device.
        self.sources.release();
        let Some(device) = index.and_then(|i| self.sources.video_devices.get(i)) else {
            self.route_audio();
            return;
        };
        let link = device.symbolic_link.clone();
        self.settings.last_video_device_symbolic_link = Some(link.clone());
        self.sources.open_card(&link, self.capture_options());
        if from_screen {
            // The card's sound was stopped while a screen was captured.
            self.select_audio(self.sources.audio_index);
        }
    }

    /// How the capture card is opened, as the settings ask.
    pub(super) fn capture_options(&self) -> CaptureOptions {
        CaptureOptions {
            prefer_compressed: self.settings.video_mode == VideoModePref::Compressed,
            ..CaptureOptions::default()
        }
    }

    /// Starts the audio pipeline for the audio device at `index` (or stops).
    pub(super) fn select_audio(&mut self, index: Option<usize>) {
        if self.cli.test_pattern {
            self.sources.audio_index = index; // only the choice changes
            return;
        }
        let tap = self.replay.audio_tap();
        let (volume, offset_ms) = (self.settings.volume, self.settings.audio_offset_ms);
        if let Some(id) = self.sources.play(index, volume, offset_ms, tap) {
            self.settings.last_audio_device_id = Some(id);
        }
    }

    /// Starts the card's sound again when it stopped (the device went away or
    /// the speakers changed), after a pause, and tells the user once.
    pub(super) fn maintain_audio(&mut self, now: Instant) {
        if self.cli.test_pattern {
            return;
        }
        let check = self.sources.check_audio(now);
        if let Some(event) = check.event {
            self.emit(event);
        }
        if check.restart {
            self.select_audio(self.sources.audio_index);
        }
    }

    /// Keeps the main source alive. A card that dropped out is opened again by
    /// the service; a screen or window that ended sends the picture back to
    /// the card, and the user is told.
    pub(super) fn maintain_source(&mut self, now: Instant) {
        let options = self.capture_options();
        if let Maintained::ScreenLost(event) = self.sources.maintain(now, options) {
            self.emit(event);
            self.select_video(self.sources.video_index);
        }
    }
}
