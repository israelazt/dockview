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

use windows::core::HSTRING;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

use crate::encode::{EncodeSettings, Mp4Remuxer};
use crate::mf::MfSession;
use dockview_domain::audio_sync::CHANNELS;
use dockview_domain::encoded::EncodedFrame;
use dockview_domain::pcm::{frames_to_ticks, mix_tracks, PcmRing};
use dockview_domain::recording::audio_limit;
use dockview_domain::recording::SavedClip;

/// Audio is written in blocks of this many frames (about 21 ms).
const AUDIO_BLOCK_FRAMES: i64 = 1024;

/// Free bytes on the disk that holds `path` (which need not exist yet), or
/// `None` if Windows does not say.
pub fn disk_free_bytes(path: &Path) -> Option<u64> {
    // Walk up to a folder that exists, so a not-yet-created folder still works.
    let mut dir = path;
    while !dir.is_dir() {
        dir = dir.parent()?;
    }
    let mut free = 0u64;
    // SAFETY: valid NUL-terminated path and a valid out-pointer.
    unsafe {
        GetDiskFreeSpaceExW(&HSTRING::from(dir.as_os_str()), Some(&mut free), None, None).ok()?;
    }
    Some(free)
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
    fn free_space_is_found_even_for_a_folder_that_does_not_exist_yet() {
        let existing = std::env::temp_dir();
        assert!(disk_free_bytes(&existing).is_some_and(|b| b > 0));
        let missing = existing
            .join("dockview-no-such-folder")
            .join("deeper")
            .join("x.mp4");
        assert!(disk_free_bytes(&missing).is_some());
    }
}
