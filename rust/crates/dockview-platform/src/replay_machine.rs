//! The real implementation of the replay ports: the GPU recorder, the
//! microphone, the global hotkeys, the disk and the shell. `dockview-app` only
//! sees the traits in `dockview_domain::ports`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dockview_domain::audio_status::AudioStatus;
use dockview_domain::frame::FrameSlot;
use dockview_domain::pcm::AudioTap;
use dockview_domain::ports::{MicInput, ReplayBuffer, ReplayHotkeys, ReplayMachine};
use dockview_domain::recording::{
    RecorderStatus, RecordingInfo, RecordingOutcome, ReplaySettings, SaveHandle,
};

use crate::clips;
use crate::hotkey::{self, Hotkey};
use crate::recorder::ReplayRecorder;
use crate::recording::disk_free_bytes;
use crate::tap_capture::{CaptureSource, TapCapture};

/// The machine the app really runs on.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlatformReplay;

impl ReplayMachine for PlatformReplay {
    fn start_buffer(
        &self,
        slot: Arc<FrameSlot>,
        settings: ReplaySettings,
    ) -> Box<dyn ReplayBuffer> {
        Box::new(ReplayRecorder::start(slot, settings))
    }

    fn open_mic(&self, device: Option<String>, tap: Arc<AudioTap>) -> Box<dyn MicInput> {
        Box::new(TapCapture::start(CaptureSource::Device(device), 1.0, tap))
    }

    fn register_hotkeys(&self) -> Box<dyn ReplayHotkeys> {
        let hotkey = Hotkey::register();
        if !hotkey.is_registered() {
            eprintln!(
                "replay hotkey {} is taken by another program",
                hotkey::LABEL
            );
        }
        if !hotkey.record_registered() {
            eprintln!(
                "record hotkey {} is taken by another program",
                hotkey::RECORD_LABEL
            );
        }
        Box::new(hotkey)
    }

    fn disk_free_bytes(&self, folder: &Path) -> Option<u64> {
        disk_free_bytes(folder)
    }

    fn new_clip_path(&self, folder: &Path) -> PathBuf {
        let name = clips::clip_file_name(clips::local_time_now());
        clips::unique_path(folder, &name)
    }

    fn new_recording_path(&self, folder: &Path) -> PathBuf {
        let name = clips::recording_file_name(clips::local_time_now());
        clips::unique_path(folder, &name)
    }

    fn create_folder(&self, folder: &Path) -> bool {
        std::fs::create_dir_all(folder).is_ok()
    }

    fn default_folder(&self) -> PathBuf {
        clips::default_folder()
    }

    fn is_writable_folder(&self, folder: &Path) -> bool {
        clips::is_writable_folder(folder)
    }

    fn hotkey_labels(&self) -> (String, String) {
        (hotkey::LABEL.to_owned(), hotkey::RECORD_LABEL.to_owned())
    }

    fn show_saved(&self, newest: Option<&Path>, folder: &Path) {
        match newest {
            Some(file) if file.exists() => clips::reveal_file(file),
            _ => clips::open_folder(folder),
        }
    }
}

impl ReplayBuffer for ReplayRecorder {
    fn status(&self) -> RecorderStatus {
        ReplayRecorder::status(self)
    }

    fn buffered_seconds(&self) -> f64 {
        ReplayRecorder::buffered_seconds(self)
    }

    fn buffered_bytes(&self) -> usize {
        ReplayRecorder::buffered_bytes(self)
    }

    fn set_keep_seconds(&self, seconds: f64) {
        ReplayRecorder::set_keep_seconds(self, seconds);
    }

    fn save(&self, seconds: f64, path: PathBuf) -> SaveHandle {
        ReplayRecorder::save(self, seconds, path)
    }

    fn recording(&self) -> RecordingInfo {
        ReplayRecorder::recording(self)
    }

    fn start_recording(&self, path: PathBuf) -> Result<(), String> {
        ReplayRecorder::start_recording(self, path)
    }

    fn stop_recording(&self) {
        ReplayRecorder::stop_recording(self);
    }

    fn take_recording_result(&self) -> Option<RecordingOutcome> {
        ReplayRecorder::take_recording_result(self)
    }

    fn audio_tap(&self) -> Arc<AudioTap> {
        ReplayRecorder::audio_tap(self)
    }

    fn mic_tap(&self) -> Arc<AudioTap> {
        ReplayRecorder::mic_tap(self)
    }

    fn clear_mic(&self) {
        ReplayRecorder::clear_mic(self);
    }
}

impl MicInput for TapCapture {
    fn status(&self) -> AudioStatus {
        TapCapture::status(self)
    }

    fn level(&self) -> f32 {
        TapCapture::level(self)
    }
}

impl ReplayHotkeys for Hotkey {
    fn take_save_pressed(&self) -> bool {
        self.take_pressed()
    }

    fn take_record_pressed(&self) -> bool {
        Hotkey::take_record_pressed(self)
    }
}
