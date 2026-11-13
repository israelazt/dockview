//! What the services tell the rest of the app. An event says what happened, in
//! the application's words; it knows nothing about how it will be shown (a
//! toast today, perhaps a sound or a plugin tomorrow).

use crate::recording::EndReason;

/// Something that happened and the user should know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEvent {
    /// A clip or a recording was asked for but the replay buffer is off.
    ReplayIsOff,
    ClipSaving,
    ClipSaved,
    ClipFailed,
    RecordingStarted,
    RecordingFailed,
    /// There is not enough free disk to start a recording.
    RecordingNoSpace,
    RecordingSaved,
    /// The picture changed size: the file was closed and a new one started.
    RecordingNewPart,
    RecordingDiskFull,
    RecordingNoSignal,
    /// The disk is running low while recording.
    LowDisk,
    /// Something cannot be changed while a recording is running.
    StopRecordingFirst,
    ReplayOn,
    ReplayOff,
    MicOn,
    MicOff,
    /// The recording quality changed and the buffer was restarted.
    QualityChanged,
    /// The chosen folder cannot be written to.
    FolderNotWritable,
    /// A source of the scene could not be started.
    SceneMissing,
    /// The picture in the corner went away.
    PipLost,
    /// The window or screen being captured is gone.
    SourceLost,
    /// The screen or window could not be captured.
    SourceFailed,
}

/// What to tell the user when a recording (or one of its files) ends, if
/// anything. `saved` says whether the file could be written.
pub fn outcome_event(reason: EndReason, saved: bool) -> Option<AppEvent> {
    match (reason, saved) {
        // The user cancelled before anything was recorded: nothing to say.
        (EndReason::Cancelled, _) => None,
        (EndReason::ResolutionChanged, true) => Some(AppEvent::RecordingNewPart),
        (EndReason::DiskFull, true) => Some(AppEvent::RecordingDiskFull),
        (EndReason::NoSignal, _) => Some(AppEvent::RecordingNoSignal),
        (_, true) => Some(AppEvent::RecordingSaved),
        (_, false) => Some(AppEvent::RecordingFailed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_way_a_recording_can_end_says_the_right_thing() {
        assert_eq!(outcome_event(EndReason::Cancelled, false), None);
        assert_eq!(
            outcome_event(EndReason::Stopped, true),
            Some(AppEvent::RecordingSaved)
        );
        assert_eq!(
            outcome_event(EndReason::ResolutionChanged, true),
            Some(AppEvent::RecordingNewPart)
        );
        assert_eq!(
            outcome_event(EndReason::DiskFull, true),
            Some(AppEvent::RecordingDiskFull)
        );
        assert_eq!(
            outcome_event(EndReason::NoSignal, false),
            Some(AppEvent::RecordingNoSignal)
        );
        assert_eq!(
            outcome_event(EndReason::Failed, false),
            Some(AppEvent::RecordingFailed)
        );
    }
}
