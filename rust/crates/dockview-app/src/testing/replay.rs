//! Fakes of the replay ports: a buffer, a microphone and the hotkeys that need
//! no GPU, no sound card and no keyboard. The test sets up what they report in
//! [`World`] and reads back what the code under test asked of them.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dockview_domain::audio_status::AudioStatus;
use dockview_domain::frame::FrameSlot;
use dockview_domain::pcm::AudioTap;
use dockview_domain::ports::{MicInput, ReplayBuffer, ReplayHotkeys, ReplayMachine};
use dockview_domain::recording::{
    RecorderStatus, RecordingInfo, RecordingOutcome, RecordingState, ReplaySettings, SaveHandle,
    SavedClip,
};

/// What the fakes share with the test: the test sets it up and reads it back.
#[derive(Default)]
pub struct World {
    pub status: Option<RecorderStatus>,
    pub info: Option<RecordingInfo>,
    pub outcomes: VecDeque<RecordingOutcome>,
    pub started: Vec<PathBuf>,
    pub start_error: Option<String>,
    pub stops: u32,
    pub saves: Vec<(f64, PathBuf)>,
    pub save_result: Arc<Mutex<Option<Result<SavedClip, String>>>>,
    pub keep: Vec<f64>,
    pub mics_opened: Vec<Option<String>>,
    pub mic_cleared: u32,
    pub mic_level: f32,
    pub hotkeys: (bool, bool),
    pub free_bytes: Option<u64>,
    pub folder_ok: bool,
    pub shown: Vec<(Option<PathBuf>, PathBuf)>,
    pub buffers_started: u32,
}

pub type Shared = Arc<Mutex<World>>;

/// A world where the buffer is recording, the folder can be made and the disk
/// has plenty of room.
pub fn world() -> Shared {
    Arc::new(Mutex::new(World {
        status: Some(RecorderStatus::Recording),
        info: Some(idle_info()),
        folder_ok: true,
        free_bytes: Some(100 << 30),
        ..World::default()
    }))
}

pub fn idle_info() -> RecordingInfo {
    RecordingInfo {
        state: RecordingState::Idle,
        seconds: 0.0,
        bytes: 0,
        low_disk: false,
    }
}

pub struct FakeMachine(pub Shared);
pub struct FakeBuffer(Shared);
pub struct FakeMic(Shared);
pub struct FakeHotkeys(Shared);

impl ReplayMachine for FakeMachine {
    fn start_buffer(&self, _: Arc<FrameSlot>, _: ReplaySettings) -> Box<dyn ReplayBuffer> {
        self.0.lock().unwrap().buffers_started += 1;
        Box::new(FakeBuffer(Arc::clone(&self.0)))
    }
    fn open_mic(&self, device: Option<String>, _: Arc<AudioTap>) -> Box<dyn MicInput> {
        self.0.lock().unwrap().mics_opened.push(device);
        Box::new(FakeMic(Arc::clone(&self.0)))
    }
    fn register_hotkeys(&self) -> Box<dyn ReplayHotkeys> {
        Box::new(FakeHotkeys(Arc::clone(&self.0)))
    }
    fn disk_free_bytes(&self, _: &Path) -> Option<u64> {
        self.0.lock().unwrap().free_bytes
    }
    fn new_clip_path(&self, folder: &Path) -> PathBuf {
        folder.join("clip.mp4")
    }
    fn new_recording_path(&self, folder: &Path) -> PathBuf {
        folder.join("recording.mp4")
    }
    fn create_folder(&self, _: &Path) -> bool {
        self.0.lock().unwrap().folder_ok
    }
    fn default_folder(&self) -> PathBuf {
        PathBuf::from("default-clips")
    }
    fn is_writable_folder(&self, folder: &Path) -> bool {
        folder != Path::new("read-only")
    }
    fn hotkey_labels(&self) -> (String, String) {
        ("Ctrl+S".into(), "Ctrl+R".into())
    }
    fn show_saved(&self, newest: Option<&Path>, folder: &Path) {
        self.0
            .lock()
            .unwrap()
            .shown
            .push((newest.map(Path::to_owned), folder.to_owned()));
    }
}

impl ReplayBuffer for FakeBuffer {
    fn status(&self) -> RecorderStatus {
        self.0.lock().unwrap().status.clone().unwrap()
    }
    fn buffered_seconds(&self) -> f64 {
        12.5
    }
    fn buffered_bytes(&self) -> usize {
        3_000_000
    }
    fn set_keep_seconds(&self, seconds: f64) {
        self.0.lock().unwrap().keep.push(seconds);
    }
    fn save(&self, seconds: f64, path: PathBuf) -> SaveHandle {
        let mut w = self.0.lock().unwrap();
        w.saves.push((seconds, path));
        SaveHandle::new(Arc::clone(&w.save_result))
    }
    fn recording(&self) -> RecordingInfo {
        self.0.lock().unwrap().info.unwrap()
    }
    fn start_recording(&self, path: PathBuf) -> Result<(), String> {
        let mut w = self.0.lock().unwrap();
        if let Some(e) = w.start_error.clone() {
            return Err(e);
        }
        w.started.push(path);
        Ok(())
    }
    fn stop_recording(&self) {
        self.0.lock().unwrap().stops += 1;
    }
    fn take_recording_result(&self) -> Option<RecordingOutcome> {
        self.0.lock().unwrap().outcomes.pop_front()
    }
    fn audio_tap(&self) -> Arc<AudioTap> {
        Arc::new(AudioTap::new(4))
    }
    fn mic_tap(&self) -> Arc<AudioTap> {
        Arc::new(AudioTap::new(4))
    }
    fn clear_mic(&self) {
        self.0.lock().unwrap().mic_cleared += 1;
    }
}

impl MicInput for FakeMic {
    fn status(&self) -> AudioStatus {
        AudioStatus::Running
    }
    fn level(&self) -> f32 {
        self.0.lock().unwrap().mic_level
    }
}

impl ReplayHotkeys for FakeHotkeys {
    fn take_save_pressed(&self) -> bool {
        std::mem::take(&mut self.0.lock().unwrap().hotkeys.0)
    }
    fn take_record_pressed(&self) -> bool {
        std::mem::take(&mut self.0.lock().unwrap().hotkeys.1)
    }
}
