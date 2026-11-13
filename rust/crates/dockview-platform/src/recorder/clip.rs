//! Writing a clip: the saved frames and audio tracks go into an MP4.

use super::*;

/// The audio tracks of a clip, in file order.
///
/// With a microphone the clip carries three: the mix of both (what every player
/// plays by default), then the game on its own and the microphone on its own so
/// an editor can rebalance them. Without one, just the game.
pub(super) fn audio_tracks(game: Option<Vec<i16>>, mic: Option<Vec<i16>>) -> Vec<Vec<i16>> {
    match (game, mic) {
        (Some(game), Some(mic)) => vec![dockview_domain::pcm::mix_tracks(&game, &mic), game, mic],
        (Some(only), None) | (None, Some(only)) => vec![only],
        (None, None) => Vec::new(),
    }
}

/// Writes `frames` to an MP4 at `path`, on a thread of its own so Media
/// Foundation gets the multithreaded apartment it needs.
pub(super) fn write_clip(
    frames: &[EncodedFrame],
    tracks: &[Vec<i16>],
    header: Option<&[u8]>,
    stream: Option<EncodeSettings>,
    path: &Path,
) -> Result<SavedClip, String> {
    let (Some(first), Some(last)) = (frames.first(), frames.last()) else {
        return Err("nothing has been recorded yet".into());
    };
    let Some(settings) = stream else {
        return Err("the recorder has not started a stream".into());
    };
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create the folder: {e}"))?;
    }
    let session = MfSession::start().map_err(|e| e.message())?;
    let mut remux = Mp4Remuxer::create(&session, path, &settings, header, tracks.len(), false)
        .map_err(|e| e.message())?;

    // Interleave audio and video by time so the MP4 is written in order.
    const AUDIO_BLOCK_FRAMES: usize = 1024;
    let channels = dockview_domain::audio_sync::CHANNELS;
    let mut audio_at = vec![0usize; tracks.len()];
    let write_audio_until = |remux: &mut Mp4Remuxer, audio_at: &mut [usize], until: i64| {
        for (track, samples) in tracks.iter().enumerate() {
            let total = samples.len() / channels;
            while audio_at[track] < total && frames_to_ticks(audio_at[track] as i64) <= until {
                let start = audio_at[track];
                let end = (start + AUDIO_BLOCK_FRAMES).min(total);
                remux
                    .write_audio(
                        track,
                        frames_to_ticks(start as i64),
                        &samples[start * channels..end * channels],
                    )
                    .map_err(|e| e.message())?;
                audio_at[track] = end;
            }
        }
        Ok::<(), String>(())
    };
    for frame in frames {
        write_audio_until(&mut remux, &mut audio_at, frame.timestamp - first.timestamp)?;
        remux.write(frame).map_err(|e| e.message())?;
    }
    write_audio_until(&mut remux, &mut audio_at, i64::MAX)?;
    remux.finish().map_err(|e| e.message())?;
    Ok(SavedClip {
        path: path.to_owned(),
        frames: frames.len(),
        seconds: (last.timestamp + last.duration - first.timestamp) as f64 / 1e7,
        bytes: std::fs::metadata(path).map_or(0, |m| m.len()),
    })
}
