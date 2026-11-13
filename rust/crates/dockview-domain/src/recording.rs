//! Recording and the replay buffer, as data: what a recording is and how it
//! ends, what the buffer was asked to keep, how full the disk is allowed to
//! get and how the parts of a long recording are named. The recorder itself
//! (threads, the encoder, the files) is in `dockview-platform`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Where a recording is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    /// Asked to start; waiting for the next keyframe to begin the file.
    Starting,
    Recording,
}

/// What the interface shows about a recording.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecordingInfo {
    pub state: RecordingState,
    /// Footage written so far.
    pub seconds: f64,
    /// Size of the file so far.
    pub bytes: u64,
    /// The disk is running low; the recording will stop if it keeps filling.
    pub low_disk: bool,
}

/// Why a recording (or one file of it) ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// The user stopped it, or the program closed.
    Stopped,
    /// Stopped before anything had been recorded (nothing to report).
    Cancelled,
    /// The disk was about to fill up.
    DiskFull,
    /// The picture changed size, which a file cannot follow: the file was
    /// closed and recording went on in a new one.
    ResolutionChanged,
    /// No video arrived after asking to start.
    NoSignal,
    /// Something went wrong (for example the disk could not be written).
    Failed,
}

/// How a recording, or one file of it, ended.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordingOutcome {
    pub result: Result<SavedClip, String>,
    pub reason: EndReason,
    /// Recording goes on in a new file.
    pub continues: bool,
}

/// Free space below which a recording stops, to leave room to close the file.
pub const DISK_STOP_BYTES: u64 = 1 << 30; // 1 GB
/// Free space below which the user is warned (about 15 minutes at 40 Mbps).
pub const DISK_WARN_BYTES: u64 = 5 << 30; // 5 GB
/// Free space needed to start a recording at all.
pub const DISK_START_MIN_BYTES: u64 = 2 << 30; // 2 GB

/// How full the disk is, for a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskLevel {
    Ok,
    Low,
    Full,
}

/// Classifies `free` bytes; `stop` is the level at which recording must end.
pub fn disk_level(free: u64, stop: u64) -> DiskLevel {
    if free < stop {
        DiskLevel::Full
    } else if free < DISK_WARN_BYTES {
        DiskLevel::Low
    } else {
        DiskLevel::Ok
    }
}

/// What to record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReplaySettings {
    /// Footage to keep, in seconds.
    pub keep_seconds: f64,
    /// Video bitrate in megabits per second.
    pub bitrate_mbps: u32,
    /// Nominal frame rate (sets the keyframe interval and frame duration).
    pub fps: u32,
}

impl Default for ReplaySettings {
    fn default() -> Self {
        Self {
            keep_seconds: 120.0,
            bitrate_mbps: 40,
            fps: 60,
        }
    }
}

/// State of the recorder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecorderStatus {
    /// Waiting for the first frame.
    Starting,
    Recording,
    /// This PC cannot record (no hardware encoder, or an unsupported format).
    Unsupported(String),
    Failed(String),
}

/// A finished clip.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedClip {
    pub path: PathBuf,
    pub frames: usize,
    pub seconds: f64,
    pub bytes: u64,
}

/// Result of a save running in the background.
pub struct SaveHandle {
    result: Arc<Mutex<Option<Result<SavedClip, String>>>>,
}

impl SaveHandle {
    /// A handle on a result that a background thread will fill in.
    pub fn new(result: Arc<Mutex<Option<Result<SavedClip, String>>>>) -> Self {
        Self { result }
    }

    /// The outcome once the save has finished.
    pub fn poll(&self) -> Option<Result<SavedClip, String>> {
        self.result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

/// The file for part `part` of a recording: part 1 is `path` itself, later
/// parts are `name (part 2).ext`, `name (part 3).ext`…
pub fn part_path(path: &Path, part: u32) -> PathBuf {
    if part <= 1 {
        return path.to_owned();
    }
    let stem = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let name = match path.extension() {
        Some(ext) => format!("{stem} (part {part}).{}", ext.to_string_lossy()),
        None => format!("{stem} (part {part})"),
    };
    path.with_file_name(name)
}

/// How many audio tracks a new recording gets: all three when any audio has
/// been heard, none otherwise.
pub fn audio_track_count(game_has_audio: bool, mic_has_audio: bool) -> usize {
    if game_has_audio || mic_has_audio {
        3
    } else {
        0
    }
}

/// Up to which instant audio may be written. Normally the audio that has
/// actually arrived (never ahead of the video); when `flush`ing at the end,
/// everything up to the last frame, with silence where audio is missing.
pub fn audio_limit(
    video_end: i64,
    game_end: Option<i64>,
    mic_end: Option<i64>,
    flush: bool,
) -> i64 {
    if flush {
        return video_end;
    }
    match (game_end, mic_end) {
        (None, None) => video_end,
        (a, b) => video_end.min(a.unwrap_or(i64::MIN).max(b.unwrap_or(i64::MIN))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_levels_follow_the_thresholds() {
        assert_eq!(disk_level(10 << 30, DISK_STOP_BYTES), DiskLevel::Ok);
        assert_eq!(disk_level(DISK_WARN_BYTES, DISK_STOP_BYTES), DiskLevel::Ok);
        assert_eq!(
            disk_level(DISK_WARN_BYTES - 1, DISK_STOP_BYTES),
            DiskLevel::Low
        );
        assert_eq!(disk_level(DISK_STOP_BYTES, DISK_STOP_BYTES), DiskLevel::Low);
        assert_eq!(
            disk_level(DISK_STOP_BYTES - 1, DISK_STOP_BYTES),
            DiskLevel::Full
        );
        assert_eq!(disk_level(0, DISK_STOP_BYTES), DiskLevel::Full);
        // A raised floor (used by tests) turns any disk "full".
        assert_eq!(disk_level(100 << 30, u64::MAX), DiskLevel::Full);
        const {
            assert!(
                DISK_START_MIN_BYTES > DISK_STOP_BYTES && DISK_WARN_BYTES > DISK_START_MIN_BYTES
            );
        }
    }

    #[test]
    fn later_parts_get_numbered_names() {
        let base = Path::new(r"C:\Videos\Dockview Recording 2026-11-05 09-04-03.mp4");
        assert_eq!(part_path(base, 1), base);
        assert_eq!(
            part_path(base, 2),
            Path::new(r"C:\Videos\Dockview Recording 2026-11-05 09-04-03 (part 2).mp4")
        );
        assert_eq!(part_path(Path::new("clip"), 3), Path::new("clip (part 3)"));
    }

    #[test]
    fn tracks_exist_only_when_some_audio_was_heard() {
        assert_eq!(audio_track_count(false, false), 0);
        assert_eq!(audio_track_count(true, false), 3);
        assert_eq!(audio_track_count(false, true), 3);
        assert_eq!(audio_track_count(true, true), 3);
    }

    #[test]
    fn audio_is_never_written_ahead_of_what_arrived_or_of_the_video() {
        // Video has reached 10; the game audio only 8, the mic nothing yet.
        assert_eq!(audio_limit(10, Some(8), None, false), 8);
        // Whichever source is further along counts.
        assert_eq!(audio_limit(10, Some(8), Some(9), false), 9);
        // Audio ahead of the video waits for the video.
        assert_eq!(audio_limit(10, Some(20), None, false), 10);
        // No audio at all: nothing holds it back.
        assert_eq!(audio_limit(10, None, None, false), 10);
        // At the end everything up to the last frame is written (silence fills).
        assert_eq!(audio_limit(10, Some(8), None, true), 10);
    }
}
