//! Where the picture and the sound come from: the capture card, a screen or a
//! window, and where the game's audio goes.

use super::*;

impl Source {
    /// What the diagnosis and the status light should know about this source.
    pub(super) fn video_state(&self) -> VideoState {
        match self {
            Self::Test(_) => VideoState::Running,
            Self::None => VideoState::NoDevice,
            Self::Capture { capture, .. } => video_state_of_capture(&capture.status()),
            Self::Screen(capture) => video_state_of_screen(&capture.status()),
        }
    }
}

impl App {
    /// Points the recorder's game-sound input at the right place: the capture
    /// card's audio, or the loopback of the PC (a monitor) or of one program (a
    /// window). Neither is played back by Dockview.
    pub(super) fn route_audio(&mut self) {
        self.loopback = None;
        let tap = self.replay.audio_tap();
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

    /// Hides Dockview's own window from the capture, so a captured screen does
    /// not show the viewer inside itself.
    pub(super) fn set_capture_exclusion(&self, on: bool) {
        if let Some(hwnd) = self.window_hwnd() {
            let affinity = if on { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
            // SAFETY: a plain call on our own window; it fails harmlessly on
            // Windows versions that do not support it.
            unsafe {
                let _ = SetWindowDisplayAffinity(hwnd, affinity);
            }
        }
    }

    /// Captures a monitor or a window instead of the capture card.
    pub(super) fn start_screen(&mut self, target: CaptureTarget, kind: SourceKind) {
        if self.cli.test_pattern {
            return;
        }
        self.source = Source::None; // releases the capture card
        self.audio = None; // the card's sound is not needed
        self.video_size = (0, 0);
        self.source_kind = kind;
        self.set_capture_exclusion(true);
        self.source = Source::Screen(self.machine.start_screen(
            target,
            ScreenOptions::default(),
            Arc::clone(&self.input_slot),
        ));
        self.route_audio();
    }

    pub(super) fn refresh_sources(&mut self) {
        self.monitors = self.machine.list_monitors();
        let own = std::process::id();
        self.windows = self
            .machine
            .list_windows()
            .into_iter()
            .filter(|w| w.process_id != own)
            .collect();
        self.sources_refreshed = Instant::now();
    }

    /// Starts on a monitor or a window when the command line or the last
    /// session asked for one.
    pub(super) fn restore_screen(&mut self) {
        if let Some(text) = self.cli.capture_window.clone() {
            let found = find_window(&self.windows, &text, std::process::id()).cloned();
            match found {
                Some(w) => self.start_screen(
                    CaptureTarget::Window(w.handle),
                    SourceKind::Window {
                        handle: w.handle,
                        pid: w.process_id,
                    },
                ),
                None => eprintln!("no window matches '{text}'"),
            }
            return;
        }
        let wanted = self
            .cli
            .screen
            .or_else(|| self.settings.last_monitor.map(|i| i as usize));
        if let Some(monitor) = wanted.and_then(|i| self.monitors.get(i)) {
            let handle = monitor.handle;
            self.start_screen(CaptureTarget::Monitor(handle), SourceKind::Monitor(handle));
        }
    }

    /// The monitors and windows for the source popover.
    pub(super) fn sources_view(&self, lang: Lang) -> SourcesView {
        let monitors = self
            .monitors
            .iter()
            .enumerate()
            .map(|(i, m)| monitor_label(lang, i, m))
            .collect();
        let windows = self.windows.iter().map(|w| w.title.clone()).collect();
        let current = self.source_kind.current(&self.monitors, &self.windows);
        SourcesView {
            monitors,
            windows,
            current,
        }
    }

    pub(super) fn match_audio(&self, video: Option<usize>) -> Option<usize> {
        let matched = match_audio_to_video(
            video.and_then(|i| self.video_devices.get(i)),
            &self.audio_devices,
        )?;
        self.audio_devices.iter().position(|a| a == matched)
    }

    /// Starts capturing from the video device at `index` (or stops).
    pub(super) fn select_video(&mut self, index: Option<usize>) {
        self.video_index = index;
        if self.cli.test_pattern {
            return; // the synthetic source stays; only the selection changes
        }
        let from_screen = self.source_kind != SourceKind::Capture;
        if from_screen {
            self.source_kind = SourceKind::Capture;
            self.settings.last_monitor = None;
            self.set_capture_exclusion(self.wants_exclusion());
        }
        self.video_size = (0, 0);
        // Drop the old capture first so it releases the device.
        self.source = Source::None;
        let Some(device) = index.and_then(|i| self.video_devices.get(i)) else {
            self.route_audio();
            return;
        };
        self.settings.last_video_device_symbolic_link = Some(device.symbolic_link.clone());
        self.source = Source::Capture {
            capture: self.start_capture(&device.symbolic_link.clone()),
            reconnect: Reconnect::new(),
        };
        if from_screen {
            // The card's sound was stopped while a screen was captured.
            self.select_audio(self.audio_index);
        }
    }

    pub(super) fn start_capture(&self, symbolic_link: &str) -> Box<dyn CaptureHandle> {
        self.machine.start_capture(
            symbolic_link.to_owned(),
            CaptureOptions {
                prefer_compressed: self.settings.video_mode == VideoModePref::Compressed,
                ..CaptureOptions::default()
            },
            Arc::clone(&self.input_slot),
        )
    }

    /// Starts the audio pipeline for the audio device at `index` (or stops).
    pub(super) fn select_audio(&mut self, index: Option<usize>) {
        self.audio_index = index;
        if self.cli.test_pattern || self.source_kind != SourceKind::Capture {
            return; // while a screen is captured, its own sound is used
        }
        self.audio = None;
        let Some(device) = index.and_then(|i| self.audio_devices.get(i)) else {
            return;
        };
        self.settings.last_audio_device_id = Some(device.device_id.clone());
        let audio = self.machine.start_playback(
            device.device_id.clone(),
            self.settings.volume,
            self.settings.audio_offset_ms,
        );
        if let Some(tap) = self.replay.audio_tap() {
            audio.set_tap(Some(tap));
        }
        self.audio = Some(audio);
    }

    /// Whether the open device also offers a compressed mode.
    pub(super) fn capture_has_compressed(&self) -> bool {
        match &self.source {
            Source::Capture { capture, .. } => matches!(
                capture.status(),
                CaptureStatus::Running {
                    has_compressed: true,
                    ..
                }
            ),
            _ => false,
        }
    }

    /// The device mode in use (a stand-in for the synthetic source).
    pub(super) fn native_mode(&self) -> Option<StreamInfo> {
        match &self.source {
            Source::Test(_) => Some(StreamInfo {
                type_index: 0,
                format_name: "NV12".into(),
                width: 1920,
                height: 1080,
                fps: 30,
                is_compressed: false,
            }),
            Source::Capture { capture, .. } => match capture.status() {
                CaptureStatus::Running { native, .. } => Some(native),
                _ => None,
            },
            Source::Screen(_) | Source::None => None,
        }
    }

    /// Restarts the capture thread if the device was lost.
    pub(super) fn maintain_source(&mut self) {
        if let Source::Screen(capture) = &self.source {
            // The window went away or cannot be captured: back to the card.
            let lost = match capture.status() {
                ScreenStatus::Ended(_) => Some(i18n::Key::ToastSourceLost),
                ScreenStatus::Failed(e) => {
                    eprintln!("screen capture failed: {e}");
                    Some(i18n::Key::ToastSourceFailed)
                }
                _ => None,
            };
            if let Some(key) = lost {
                self.toast(key, ToastKind::Error);
                self.select_video(self.video_index);
            }
            return;
        }
        let Some(device) = self.video_index.and_then(|i| self.video_devices.get(i)) else {
            return;
        };
        let link = device.symbolic_link.clone();
        let Source::Capture { capture, reconnect } = &mut self.source else {
            return;
        };
        let lost = matches!(
            capture.status(),
            CaptureStatus::Disconnected { .. } | CaptureStatus::Failed(_)
        );
        if reconnect.tick(lost, Instant::now()) {
            let capture = self.start_capture(&link);
            self.source = Source::Capture {
                capture,
                reconnect: Reconnect::new(),
            };
        }
    }
}
