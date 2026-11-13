//! Plugging and unplugging while the program runs: the lists of cards and audio
//! inputs follow what Windows reports, and a device that comes back is used
//! again without restarting.

use super::*;

impl Core {
    /// Compares the devices Windows lists now with the ones known, and acts on
    /// the difference. Cheap when nothing changed.
    pub(super) fn refresh_devices(&mut self) {
        if self.cli.test_pattern {
            return;
        }
        let lists = self.machine.devices();
        let video_key = self
            .video_index
            .and_then(|i| self.video_devices.get(i))
            .map(|d| d.symbolic_link.clone());
        let audio_key = self
            .audio_index
            .and_then(|i| self.audio_devices.get(i))
            .map(|d| d.device_id.clone());
        let video = reconcile(
            &self.video_devices,
            &lists.video,
            video_key.as_deref(),
            |d| &d.symbolic_link,
        );
        let audio = reconcile(
            &self.audio_devices,
            &lists.audio,
            audio_key.as_deref(),
            |d| &d.device_id,
        );
        if !video.changed && !audio.changed {
            return;
        }
        self.video_devices = lists.video;
        self.audio_devices = lists.audio;
        self.video_index = video.selected;
        self.audio_index = audio.selected;
        // While a screen is shown, the lists are kept but nothing is started.
        if self.source_kind == SourceKind::Capture {
            self.restore_video_device();
            self.restore_audio_device();
        }
    }

    /// Opens a card when none is selected: the first one the very first time,
    /// and afterwards only the one that was in use (when it comes back), so
    /// that unplugging the capture card never moves the picture to a webcam.
    fn restore_video_device(&mut self) {
        if self.video_index.is_some() {
            return;
        }
        let last = self.settings.last_video_device_symbolic_link.as_deref();
        let found = if matches!(self.source, Source::None) {
            pick_video(&self.video_devices, None, last)
        } else {
            last.and_then(|link| {
                self.video_devices
                    .iter()
                    .position(|d| d.symbolic_link == link)
            })
        };
        let Some(index) = found else {
            return;
        };
        self.select_video(Some(index));
        let matched = self.match_audio(Some(index));
        let last_audio = self.settings.last_audio_device_id.as_deref();
        let audio = pick_audio(&self.audio_devices, last_audio, matched);
        self.select_audio(audio);
    }

    /// Same for the audio input, when the card's own did not come with it.
    fn restore_audio_device(&mut self) {
        if self.audio_index.is_some() {
            return;
        }
        let last = self.settings.last_audio_device_id.as_deref();
        let found = if self.audio.is_none() {
            pick_audio(
                &self.audio_devices,
                last,
                self.match_audio(self.video_index),
            )
        } else {
            last.and_then(|id| self.audio_devices.iter().position(|d| d.device_id == id))
        };
        if found.is_some() {
            self.select_audio(found);
        }
    }
}
