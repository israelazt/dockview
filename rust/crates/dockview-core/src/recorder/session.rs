//! The recording the user asked for (as opposed to the replay buffer): waiting
//! for a keyframe, writing the file, splitting it when the picture changes size
//! and ending it cleanly.

use super::*;

/// How often the free disk space is checked while recording.
pub(super) const DISK_CHECK_EVERY: Duration = Duration::from_secs(2);

/// The recording's state on the recorder thread.
pub(super) struct RecCtx {
    /// Asked to start; waiting for a keyframe.
    pub(super) pending: Option<PathBuf>,
    /// When the wait began, to give up if no video arrives.
    pub(super) pending_since: Option<Instant>,
    pub(super) active: Option<ActiveRecording>,
    /// The path the user asked for; later parts are named after it.
    pub(super) base: Option<PathBuf>,
    /// Which file of the recording is being written (1 is the first).
    pub(super) part: u32,
    pub(super) last_disk_check: Instant,
}

impl Default for RecCtx {
    fn default() -> Self {
        Self {
            pending: None,
            pending_since: None,
            active: None,
            base: None,
            part: 1,
            last_disk_check: Instant::now(),
        }
    }
}

/// Ends the recording (or one of its files). With `continues`, the next file is
/// already waiting, so the state goes back to "starting" instead of idle.
pub(super) fn rec_end(
    shared: &Shared,
    result: Result<SavedClip, String>,
    reason: EndReason,
    continues: bool,
) {
    *shared.rec_state.lock().unwrap_or_else(|e| e.into_inner()) = if continues {
        RecordingState::Starting
    } else {
        RecordingState::Idle
    };
    shared.rec_seconds.store(0f64.to_bits(), Ordering::Relaxed);
    shared
        .rec_result
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push_back(RecordingOutcome {
            result,
            reason,
            continues,
        });
}

pub(super) fn rec_finish(
    active: ActiveRecording,
    shared: &Shared,
    reason: EndReason,
    continues: bool,
) {
    let result = {
        let game = shared.pcm.lock().unwrap_or_else(|e| e.into_inner());
        let mic = shared.mic_pcm.lock().unwrap_or_else(|e| e.into_inner());
        active.finish(&game, &mic)
    };
    let reason = if result.is_err() {
        EndReason::Failed
    } else {
        reason
    };
    rec_end(shared, result, reason, continues);
}

/// Applies the start and stop requests, in order.
pub(super) fn rec_commands(rec: &mut RecCtx, shared: &Shared) {
    let queued: Vec<RecCommand> = shared
        .rec_commands
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .drain(..)
        .collect();
    for command in queued {
        match command {
            RecCommand::Start(path) if rec.active.is_none() && rec.pending.is_none() => {
                rec.base = Some(path.clone());
                rec.part = 1;
                rec.pending = Some(path);
                rec.pending_since = Some(Instant::now());
            }
            RecCommand::Start(_) => {}
            RecCommand::Stop => {
                if let Some(active) = rec.active.take() {
                    rec_finish(active, shared, EndReason::Stopped, false);
                } else if rec.pending.take().is_some() {
                    rec_end(
                        shared,
                        Err("stopped before the first frame".into()),
                        EndReason::Cancelled,
                        false,
                    );
                }
            }
        }
    }
}

/// Writes the audio that has arrived to the recording, and watches the disk
/// and the video signal.
pub(super) fn rec_pump(rec: &mut RecCtx, shared: &Shared) {
    // Give up on a start that never gets video.
    if let (Some(_), Some(since)) = (&rec.pending, rec.pending_since) {
        let wait = Duration::from_millis(shared.no_signal_ms.load(Ordering::Relaxed));
        if since.elapsed() > wait {
            rec.pending = None;
            rec_end(
                shared,
                Err("no video signal arrived".into()),
                EndReason::NoSignal,
                false,
            );
        }
    }
    let Some(active) = rec.active.as_mut() else {
        return;
    };
    let result = {
        let game = shared.pcm.lock().unwrap_or_else(|e| e.into_inner());
        let mic = shared.mic_pcm.lock().unwrap_or_else(|e| e.into_inner());
        active.pump_audio(&game, &mic, false)
    };
    if let Err(e) = result {
        // The file stays valid up to the last fragment; just stop writing.
        rec.active = None;
        rec_end(shared, Err(e), EndReason::Failed, false);
        return;
    }
    if rec.last_disk_check.elapsed() >= DISK_CHECK_EVERY {
        rec.last_disk_check = Instant::now();
        let stop = shared.disk_stop_bytes.load(Ordering::Relaxed);
        match disk_free_bytes(active.path()).map(|free| disk_level(free, stop)) {
            Some(DiskLevel::Full) => {
                if let Some(active) = rec.active.take() {
                    rec_finish(active, shared, EndReason::DiskFull, false);
                }
            }
            Some(DiskLevel::Low) => shared.rec_low_disk.store(true, Ordering::Relaxed),
            _ => {}
        }
    }
}

/// Gives a freshly compressed frame to the recording: starts the file at the
/// first keyframe after the request, then writes every frame.
pub(super) fn rec_on_frame(
    rec: &mut RecCtx,
    shared: &Shared,
    session: &MfSession,
    settings: &EncodeSettings,
    encoded: &EncodedFrame,
) {
    if let Some(active) = rec.active.as_mut() {
        match active.write_frame(encoded) {
            Ok(()) => shared
                .rec_seconds
                .store(active.seconds().to_bits(), Ordering::Relaxed),
            Err(e) => {
                rec.active = None;
                rec_end(shared, Err(e), EndReason::Failed, false);
            }
        }
    } else if rec.pending.is_some() && encoded.keyframe {
        let Some(path) = rec.pending.take() else {
            return;
        };
        let header = shared
            .header
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let tracks = {
            let game = shared.pcm.lock().unwrap_or_else(|e| e.into_inner());
            let mic = shared.mic_pcm.lock().unwrap_or_else(|e| e.into_inner());
            audio_track_count(!game.is_empty(), !mic.is_empty())
        };
        match ActiveRecording::begin(session, &path, settings, header.as_deref(), tracks, encoded) {
            Ok(active) => {
                *shared.rec_path.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(active.path().to_owned());
                *shared.rec_state.lock().unwrap_or_else(|e| e.into_inner()) =
                    RecordingState::Recording;
                rec.pending_since = None;
                rec.active = Some(active);
            }
            Err(e) => rec_end(shared, Err(e), EndReason::Failed, false),
        }
    }
}
