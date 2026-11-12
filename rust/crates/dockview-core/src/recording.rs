//! Recording a whole session to disk.
//!
//! The recorder thread already compresses every frame for the replay buffer.
//! A recording reuses those same compressed frames (nothing is encoded twice)
//! and writes them, together with the audio, to a *fragmented* MP4: the file is
//! valid after every fragment, so a crash or a power cut loses seconds, not the
//! whole session.
//!
//! Audio goes in as three tracks, like a replay clip with a microphone: the mix
//! (what players play), the game and the microphone. They are always all
//! present, so turning the microphone on halfway through still records it.

use std::path::{Path, PathBuf};

use crate::audio_sync::CHANNELS;
use crate::encode::{EncodeSettings, EncodedFrame, Mp4Remuxer};
use crate::mf::MfSession;
use crate::pcm::{frames_to_ticks, mix_tracks, PcmRing};
use crate::recorder::SavedClip;

/// Audio is written in blocks of this many frames (about 21 ms).
const AUDIO_BLOCK_FRAMES: i64 = 1024;

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

/// A recording in progress. Lives on the recorder thread.
pub struct ActiveRecording {
    remux: Mp4Remuxer,
    path: PathBuf,
    /// Timestamp of the first video frame, on the shared timeline.
    origin: i64,
    /// Next instant of audio to write.
    audio_cursor: i64,
    /// End of the newest video frame written.
    video_end: i64,
    frames: usize,
    audio_tracks: usize,
}

impl ActiveRecording {
    /// Starts the file at `first`, which must be a keyframe.
    pub fn begin(
        session: &MfSession,
        path: &Path,
        settings: &EncodeSettings,
        header: Option<&[u8]>,
        audio_tracks: usize,
        first: &EncodedFrame,
    ) -> Result<Self, String> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("could not create the folder: {e}"))?;
        }
        let remux = Mp4Remuxer::create(session, path, settings, header, audio_tracks, true)
            .map_err(|e| e.message())?;
        let mut recording = Self {
            remux,
            path: path.to_owned(),
            origin: first.timestamp,
            audio_cursor: first.timestamp,
            video_end: first.timestamp,
            frames: 0,
            audio_tracks,
        };
        recording.write_frame(first)?;
        Ok(recording)
    }

    pub fn write_frame(&mut self, frame: &EncodedFrame) -> Result<(), String> {
        self.remux.write(frame).map_err(|e| e.message())?;
        self.video_end = frame.timestamp + frame.duration;
        self.frames += 1;
        Ok(())
    }

    /// Footage written so far, in seconds.
    pub fn seconds(&self) -> f64 {
        (self.video_end - self.origin) as f64 / 1e7
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes the audio that is ready. With `flush`, writes everything up to
    /// the end of the video.
    pub fn pump_audio(&mut self, game: &PcmRing, mic: &PcmRing, flush: bool) -> Result<(), String> {
        if self.audio_tracks == 0 {
            return Ok(());
        }
        let limit = audio_limit(self.video_end, game.end_ts(), mic.end_ts(), flush);
        let block = frames_to_ticks(AUDIO_BLOCK_FRAMES);
        loop {
            let remaining = limit - self.audio_cursor;
            if remaining <= 0 || (!flush && remaining < block) {
                return Ok(());
            }
            let to = self.audio_cursor + remaining.min(block);
            let game_pcm = game.extract(self.audio_cursor, to);
            let mic_pcm = mic.extract(self.audio_cursor, to);
            if game_pcm.len() < CHANNELS {
                return Ok(());
            }
            let tracks = [mix_tracks(&game_pcm, &mic_pcm), game_pcm, mic_pcm];
            for (index, samples) in tracks.iter().enumerate().take(self.audio_tracks) {
                self.remux
                    .write_audio(index, self.audio_cursor - self.origin, samples)
                    .map_err(|e| e.message())?;
            }
            self.audio_cursor = to;
        }
    }

    /// Writes the last audio, closes the file and reports it.
    pub fn finish(mut self, game: &PcmRing, mic: &PcmRing) -> Result<SavedClip, String> {
        self.pump_audio(game, mic, true)?;
        let (path, frames, seconds) = (self.path.clone(), self.frames, self.seconds());
        self.remux.finish().map_err(|e| e.message())?;
        Ok(SavedClip {
            bytes: std::fs::metadata(&path).map_or(0, |m| m.len()),
            path,
            frames,
            seconds,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
